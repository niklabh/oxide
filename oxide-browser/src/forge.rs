//! Oxide Forge — Claude-powered guest app generation layer.
//!
//! Turns a user's natural-language prompt into a compiled guest `.wasm`
//! module that can be hot-loaded into an Oxide tab. Driven entirely by
//! the native host UI (`oxide://forge`) — no guest-side API yet.
//!
//! Pipeline:
//!
//! 1. [`ForgeState::start`] — scaffolds a fresh project under
//!    `target/forge/<slug>/`, copying `forge/templates/base/` and
//!    spawning a background task that streams a Claude response into the
//!    session's `code` buffer.
//! 2. The UI polls [`ForgeState::snapshot`] each frame to render
//!    progress.
//! 3. When streaming completes, the session's `src/lib.rs` is written
//!    to disk. The UI may then call [`ForgeState::build`], which spawns
//!    `cargo build --target wasm32-unknown-unknown --release` in the
//!    session directory and populates either `artifact_path` or
//!    `build_log` on completion.
//!
//! The Anthropic API key is read from the `ANTHROPIC_API_KEY` environment
//! variable on process start. The system prompt is composed at boot from
//! the markdown files in `forge/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::runtime::Runtime;

// ── Configuration ──────────────────────────────────────────────────────────

/// Default model. Override with `OXIDE_FORGE_MODEL` env var.
pub const DEFAULT_MODEL: &str = "claude-opus-4-20250514";

/// Max output tokens per generation. Opus 4 allows up to 8192 (non-beta).
const MAX_TOKENS: u32 = 8192;

const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";

/// How many times Forge will auto-retry on `cargo build` failure before
/// giving up and surfacing the error to the user.
const MAX_AUTO_RETRIES: u32 = 3;

// ── Public phase / snapshot types ──────────────────────────────────────────

/// Coarse-grained state machine for a single Forge session, surfaced to the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgePhase {
    /// Session has been created but streaming has not started.
    Idle,
    /// Claude is currently streaming tokens into `code`.
    Streaming,
    /// Streaming finished cleanly; `code` is the complete generated file.
    StreamComplete,
    /// `cargo build` is running in the background.
    Building,
    /// Build succeeded; `artifact_path` points at the `.wasm`.
    BuildOk,
    /// Streaming or build failed; inspect `error` / `build_log`.
    Error,
}

/// Snapshot of a session for the UI to render. Cheap to clone (a few
/// strings).
#[derive(Clone, Debug)]
pub struct ForgeSnapshot {
    pub id: u64,
    pub slug: String,
    pub prompt: String,
    pub code: String,
    pub phase: ForgePhase,
    pub build_log: String,
    pub artifact_path: Option<PathBuf>,
    pub error: Option<String>,
    /// Monotonically increases each time the session is mutated, so the UI
    /// can skip re-rendering identical snapshots.
    pub revision: u64,
    /// Number of auto-fix retries already consumed (0..=MAX_AUTO_RETRIES).
    pub retries_used: u32,
    /// Maximum number of auto-fix retries for this session.
    pub max_retries: u32,
    /// Whether auto-retry on build failure is enabled for this session.
    pub auto_fix: bool,
}

// ── Internal session state ─────────────────────────────────────────────────

struct Session {
    id: u64,
    slug: String,
    prompt: String,
    code: String,
    phase: ForgePhase,
    build_log: String,
    artifact_path: Option<PathBuf>,
    error: Option<String>,
    revision: u64,
    /// Absolute path to `target/forge/<slug>/`.
    project_dir: PathBuf,
    retries_used: u32,
    auto_fix: bool,
}

impl Session {
    fn snapshot(&self) -> ForgeSnapshot {
        ForgeSnapshot {
            id: self.id,
            slug: self.slug.clone(),
            prompt: self.prompt.clone(),
            code: self.code.clone(),
            phase: self.phase,
            build_log: self.build_log.clone(),
            artifact_path: self.artifact_path.clone(),
            error: self.error.clone(),
            revision: self.revision,
            retries_used: self.retries_used,
            max_retries: MAX_AUTO_RETRIES,
            auto_fix: self.auto_fix,
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

type SharedSession = Arc<Mutex<Session>>;

// ── ForgeState ─────────────────────────────────────────────────────────────

pub struct ForgeState {
    runtime: Runtime,
    sessions: HashMap<u64, SharedSession>,
    next_id: AtomicU64,
    forge_dir: PathBuf,
    template_dir: PathBuf,
    system_prompt: String,
    api_key: String,
    model: String,
}

impl ForgeState {
    /// Initialise Forge. Returns `None` if:
    /// - the `ANTHROPIC_API_KEY` env var is unset, or
    /// - the tokio runtime cannot be created.
    ///
    /// The repo layout is resolved from `CARGO_MANIFEST_DIR` at compile time,
    /// so Forge works for `cargo run -p oxide-browser` straight out of the
    /// repo. Production/installed builds need a different resolution path
    /// (left for a future ticket).
    pub fn new() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        let runtime = Runtime::new().ok()?;

        let repo_root = repo_root();
        let forge_dir = repo_root.join("target").join("forge");
        let template_dir = repo_root.join("forge").join("templates").join("base");

        // Best-effort: create target/forge/ up front so the UI can deep-link.
        let _ = std::fs::create_dir_all(&forge_dir);

        let system_prompt = build_system_prompt(&repo_root).unwrap_or_else(|_| {
            "You are Oxide Forge. Produce a single Rust `src/lib.rs` in \
             one fenced code block. Import only from `oxide_sdk`. Export \
             `start_app` and `on_frame`."
                .to_string()
        });

        let model =
            std::env::var("OXIDE_FORGE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        Some(Self {
            runtime,
            sessions: HashMap::new(),
            next_id: AtomicU64::new(1),
            forge_dir,
            template_dir,
            system_prompt,
            api_key,
            model,
        })
    }

    /// Create a new session and start a background Claude stream.
    /// Returns the session id (always `> 0`).
    pub fn start(&mut self, prompt: String) -> Result<u64> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let slug = make_slug(id);
        let project_dir = self.forge_dir.join(&slug);
        scaffold_project(&self.template_dir, &project_dir)
            .with_context(|| format!("scaffold {project_dir:?}"))?;

        let session = Arc::new(Mutex::new(Session {
            id,
            slug,
            prompt: prompt.clone(),
            code: String::new(),
            phase: ForgePhase::Idle,
            build_log: String::new(),
            artifact_path: None,
            error: None,
            revision: 0,
            project_dir,
            retries_used: 0,
            auto_fix: true,
        }));

        self.sessions.insert(id, session.clone());

        let system = self.system_prompt.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();

        self.runtime.spawn(async move {
            run_stream_then_build(session, system, api_key, model, prompt).await;
        });

        Ok(id)
    }

    /// Fetch a read-only snapshot of a session, if it exists.
    pub fn snapshot(&self, id: u64) -> Option<ForgeSnapshot> {
        let s = self.sessions.get(&id)?;
        let s = s.lock().ok()?;
        Some(s.snapshot())
    }

    /// List all known session ids, oldest-first.
    pub fn list_ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.sessions.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Kick off a `cargo build` for a session whose streaming is done.
    /// Subsequent calls while building are no-ops. Does NOT consume an
    /// auto-retry — this is an explicit manual rebuild (e.g. after the
    /// user edits an error message and wants another shot).
    pub fn build(&mut self, id: u64) -> Result<()> {
        let session = self
            .sessions
            .get(&id)
            .ok_or_else(|| anyhow!("unknown forge session {id}"))?
            .clone();

        {
            let s = session.lock().unwrap();
            if !matches!(
                s.phase,
                ForgePhase::StreamComplete | ForgePhase::Error | ForgePhase::BuildOk
            ) {
                bail!("session {id} is not ready to build (phase={:?})", s.phase);
            }
        }

        // A manual build does not trigger auto-retry on failure; we just
        // run `cargo` once and surface the result.
        self.runtime.spawn(async move {
            run_build(session).await;
        });

        Ok(())
    }

    /// Toggle auto-fix on a session. No-op if the session doesn't exist.
    pub fn set_auto_fix(&mut self, id: u64, enabled: bool) {
        if let Some(s) = self.sessions.get(&id) {
            let mut s = s.lock().unwrap();
            s.auto_fix = enabled;
            s.bump();
        }
    }

    /// Read the built artifact bytes for a session, if the build succeeded.
    pub fn artifact_bytes(&self, id: u64) -> Option<Vec<u8>> {
        let snap = self.snapshot(id)?;
        let path = snap.artifact_path?;
        std::fs::read(&path).ok()
    }

    /// Convenience for the UI: the system prompt length (tokens ~ chars/4),
    /// so we can show a rough "context used" indicator.
    pub fn system_prompt_len(&self) -> usize {
        self.system_prompt.len()
    }
}

// ── Orchestration: stream → build → maybe auto-retry ───────────────────────

/// Run one full "generate → compile" cycle. On compile failure, if the
/// session still has auto-retry budget, loop with a correction prompt.
async fn run_stream_then_build(
    session: SharedSession,
    system: String,
    api_key: String,
    model: String,
    initial_prompt: String,
) {
    // First pass uses the user's prompt as-is.
    if !stream_one_attempt(&session, &system, &api_key, &model, &initial_prompt).await {
        return;
    }

    loop {
        run_build(session.clone()).await;

        // Inspect outcome and decide whether to auto-retry.
        let (should_retry, retry_prompt) = {
            let s = session.lock().unwrap();
            let can_retry = matches!(s.phase, ForgePhase::Error)
                && s.auto_fix
                && s.retries_used < MAX_AUTO_RETRIES;
            if can_retry {
                (true, build_retry_prompt(&s))
            } else {
                (false, String::new())
            }
        };

        if !should_retry {
            break;
        }

        {
            let mut s = session.lock().unwrap();
            s.retries_used += 1;
            s.code.clear();
            s.bump();
        }

        if !stream_one_attempt(&session, &system, &api_key, &model, &retry_prompt).await {
            break;
        }
    }
}

/// Stream a single generation, write the resulting code to disk, and set
/// phase to [`ForgePhase::StreamComplete`]. Returns `true` on success.
async fn stream_one_attempt(
    session: &SharedSession,
    system: &str,
    api_key: &str,
    model: &str,
    user_prompt: &str,
) -> bool {
    {
        let mut s = session.lock().unwrap();
        s.phase = ForgePhase::Streaming;
        s.bump();
    }

    if let Err(e) = drive_anthropic_stream(session, system, api_key, model, user_prompt).await {
        let mut s = session.lock().unwrap();
        s.phase = ForgePhase::Error;
        s.error = Some(e.to_string());
        s.bump();
        return false;
    }

    let code_on_disk = {
        let s = session.lock().unwrap();
        extract_rust_block(&s.code)
    };

    let project_dir = session.lock().unwrap().project_dir.clone();
    if let Err(e) = write_lib_rs(&project_dir, &code_on_disk) {
        let mut s = session.lock().unwrap();
        s.phase = ForgePhase::Error;
        s.error = Some(format!("failed to write lib.rs: {e}"));
        s.bump();
        return false;
    }

    let mut s = session.lock().unwrap();
    s.code = code_on_disk;
    s.phase = ForgePhase::StreamComplete;
    s.bump();
    true
}

/// Compose a "please fix this" prompt from the last failed attempt.
fn build_retry_prompt(s: &Session) -> String {
    // Truncate excessively long logs to keep the context window bounded.
    let log = truncate_middle(&s.build_log, 6_000);
    format!(
        "Your previous attempt at this app did not compile. Fix it.\n\n\
         Original request:\n{}\n\n\
         Previous lib.rs:\n```rust\n{}\n```\n\n\
         Compiler output:\n```\n{}\n```\n\n\
         Reply with the complete corrected lib.rs in one ```rust fenced block.",
        s.prompt, s.code, log
    )
}

/// If `text` exceeds `max_bytes`, keep the first and last halves with an
/// elision marker in between. Byte-safe (won't split a multi-byte char).
fn truncate_middle(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let half = max_bytes / 2;
    let mut head_end = half;
    while head_end < text.len() && !text.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = text.len().saturating_sub(half);
    while tail_start < text.len() && !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    format!(
        "{}\n…[truncated {} bytes]…\n{}",
        &text[..head_end],
        text.len() - head_end - (text.len() - tail_start),
        &text[tail_start..]
    )
}

async fn drive_anthropic_stream(
    session: &SharedSession,
    system: &str,
    api_key: &str,
    model: &str,
    user_prompt: &str,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build http client")?;

    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": system,
        "messages": [{ "role": "user", "content": user_prompt }],
    });
    let body_bytes = serde_json::to_vec(&body).context("serialise request body")?;

    let resp = client
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .body(body_bytes)
        .send()
        .await
        .context("POST /v1/messages")?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        bail!("anthropic {}: {}", status, err_body);
    }

    let mut stream = resp.bytes_stream();
    // SSE is line-delimited with `\n\n` separators. We buffer across chunks
    // and emit completed events (those ending with a blank line).
    let mut buf = Vec::<u8>::new();
    while let Some(next) = stream.next().await {
        let chunk = next.context("stream read")?;
        buf.extend_from_slice(&chunk);

        // Look for `\n\n` event boundaries and process each.
        while let Some(pos) = find_event_boundary(&buf) {
            let event = buf.drain(..pos).collect::<Vec<u8>>();
            // Drop the boundary bytes themselves.
            let skip = if buf.starts_with(b"\r\n\r\n") { 4 } else { 2 };
            buf.drain(..skip.min(buf.len()));

            if let Some(delta) = parse_sse_event(&event) {
                if !delta.is_empty() {
                    let mut s = session.lock().unwrap();
                    s.code.push_str(&delta);
                    s.bump();
                }
            }
        }
    }

    Ok(())
}

/// Find the end of the first complete SSE event (`\n\n` or `\r\n\r\n`)
/// and return the index of the first byte of the separator.
fn find_event_boundary(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i);
        }
        if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

/// Parse a single SSE event (already stripped of trailing blank line).
/// Returns the text delta contribution, if any.
fn parse_sse_event(event: &[u8]) -> Option<String> {
    // Only care about `data: {...}` lines. Concatenate multi-line data values.
    let text = std::str::from_utf8(event).ok()?;
    let mut data = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.trim_start());
            data.push('\n');
        }
    }
    if data.is_empty() {
        return None;
    }
    let data = data.trim();
    if data == "[DONE]" {
        return None;
    }
    let v: Value = serde_json::from_str(data).ok()?;
    let kind = v.get("type")?.as_str()?;
    match kind {
        "content_block_delta" => {
            let t = v.get("delta")?.get("text")?.as_str()?;
            Some(t.to_string())
        }
        "message_delta"
        | "message_start"
        | "message_stop"
        | "content_block_start"
        | "content_block_stop"
        | "ping" => None,
        _ => None,
    }
}

/// Extract the Rust source from a response that may (but need not) include
/// a ```rust … ``` fence. Falls back to returning the raw response.
fn extract_rust_block(reply: &str) -> String {
    if let Some(start) = reply.find("```rust") {
        let after = &reply[start + "```rust".len()..];
        // Skip optional newline.
        let after = after.strip_prefix('\n').unwrap_or(after);
        if let Some(end) = after.find("```") {
            return after[..end].trim_end().to_string();
        }
    }
    if let Some(start) = reply.find("```") {
        let after = &reply[start + 3..];
        let after = after.strip_prefix('\n').unwrap_or(after);
        if let Some(end) = after.find("```") {
            return after[..end].trim_end().to_string();
        }
    }
    reply.trim().to_string()
}

// ── Project scaffolding ────────────────────────────────────────────────────

fn scaffold_project(template: &Path, project_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(project_dir.join("src"))?;

    let cargo_toml = template.join("Cargo.toml");
    let lib_rs = template.join("src").join("lib.rs");

    if cargo_toml.is_file() {
        std::fs::copy(&cargo_toml, project_dir.join("Cargo.toml"))?;
    } else {
        bail!("template Cargo.toml missing at {cargo_toml:?}");
    }
    if lib_rs.is_file() {
        std::fs::copy(&lib_rs, project_dir.join("src").join("lib.rs"))?;
    } else {
        bail!("template src/lib.rs missing at {lib_rs:?}");
    }

    Ok(())
}

fn write_lib_rs(project_dir: &Path, code: &str) -> Result<()> {
    let path = project_dir.join("src").join("lib.rs");
    std::fs::write(&path, code).with_context(|| format!("write {path:?}"))?;
    Ok(())
}

// ── Cargo build ────────────────────────────────────────────────────────────

async fn run_build(session: SharedSession) {
    let project_dir = {
        let mut s = session.lock().unwrap();
        s.phase = ForgePhase::Building;
        s.build_log.clear();
        s.error = None;
        s.bump();
        s.project_dir.clone()
    };

    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("build")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .arg("--release")
        .arg("--quiet")
        .arg("--color")
        .arg("never")
        .env("CARGO_TERM_COLOR", "never")
        .current_dir(&project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let mut s = session.lock().unwrap();
            s.phase = ForgePhase::Error;
            s.error = Some(format!("spawn cargo: {e}"));
            s.bump();
            return;
        }
    };

    // Drain stderr (cargo diagnostics) into the session log.
    let mut stderr_buf = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_buf).await;
    }
    // Ignore stdout — quiet mode produces nothing on success.
    if let Some(mut stdout) = child.stdout.take() {
        let mut _discard = String::new();
        let _ = stdout.read_to_string(&mut _discard).await;
    }

    let status = child.wait().await;

    let mut s = session.lock().unwrap();
    s.build_log = stderr_buf;

    match status {
        Ok(st) if st.success() => {
            let artifact = project_dir
                .join("target")
                .join("wasm32-unknown-unknown")
                .join("release")
                .join("forge_app.wasm");
            if artifact.is_file() {
                s.artifact_path = Some(artifact);
                s.phase = ForgePhase::BuildOk;
            } else {
                s.phase = ForgePhase::Error;
                s.error = Some(format!("cargo returned success but {artifact:?} missing"));
            }
        }
        Ok(st) => {
            s.phase = ForgePhase::Error;
            s.error = Some(format!("cargo exited with {st}"));
        }
        Err(e) => {
            s.phase = ForgePhase::Error;
            s.error = Some(format!("cargo wait failed: {e}"));
        }
    }
    s.bump();
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    // `oxide-browser` sits at repo_root/oxide-browser. One up from its
    // manifest dir is the repo root during `cargo run`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn make_slug(id: u64) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("s{secs:010}-{id:04}")
}

fn build_system_prompt(repo_root: &Path) -> Result<String> {
    let forge = repo_root.join("forge");
    let core =
        std::fs::read_to_string(forge.join("SYSTEM_PROMPT.md")).context("read SYSTEM_PROMPT.md")?;
    let caps =
        std::fs::read_to_string(forge.join("CAPABILITIES.md")).context("read CAPABILITIES.md")?;
    let patterns =
        std::fs::read_to_string(forge.join("PATTERNS.md")).context("read PATTERNS.md")?;
    let recipes = std::fs::read_to_string(forge.join("RECIPES.md")).context("read RECIPES.md")?;

    Ok(format!(
        "{core}\n\n\
         ---\n\n\
         # Reference: Capabilities\n\n\
         {caps}\n\n\
         ---\n\n\
         # Reference: Patterns\n\n\
         {patterns}\n\n\
         ---\n\n\
         # Reference: Recipes\n\n\
         {recipes}\n"
    ))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_fence() {
        let reply = "Here is your app:\n```rust\nuse oxide_sdk::*;\n```\n";
        assert_eq!(extract_rust_block(reply), "use oxide_sdk::*;");
    }

    #[test]
    fn extracts_plain_fence() {
        let reply = "```\nuse oxide_sdk::*;\n```\nfootnote";
        assert_eq!(extract_rust_block(reply), "use oxide_sdk::*;");
    }

    #[test]
    fn passthrough_when_no_fence() {
        let reply = "use oxide_sdk::*;\nfn main(){}";
        assert_eq!(extract_rust_block(reply), reply.trim());
    }

    #[test]
    fn slug_has_expected_shape() {
        let s = make_slug(42);
        assert!(s.starts_with('s'), "slug was: {s}");
        assert!(s.ends_with("-0042"), "slug was: {s}");
        // `s` + 10 digits of secs + `-` + 4 digits of id = 16 (until epoch >= 10^10).
        assert_eq!(s.len(), 16, "slug was: {s}");
    }

    #[test]
    fn parses_content_block_delta() {
        let event = b"event: content_block_delta\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}";
        assert_eq!(parse_sse_event(event).as_deref(), Some("hi"));
    }

    #[test]
    fn ignores_ping_events() {
        let event = b"event: ping\ndata: {\"type\":\"ping\"}";
        assert_eq!(parse_sse_event(event), None);
    }

    #[test]
    fn finds_sse_boundary() {
        let buf = b"data: x\n\ndata: y\n\n";
        assert_eq!(find_event_boundary(buf), Some(7));
    }

    #[test]
    fn scaffold_copies_template() {
        let tmp = tempfile::tempdir().unwrap();
        let template = repo_root().join("forge").join("templates").join("base");
        assert!(template.is_dir(), "template must exist");

        let project = tmp.path().join("sandbox-project");
        scaffold_project(&template, &project).expect("scaffold");

        assert!(project.join("Cargo.toml").is_file());
        assert!(project.join("src").join("lib.rs").is_file());

        // Overwrite with a guaranteed-valid tiny lib.rs and ensure it sticks.
        let code = "pub fn hi() -> i32 { 42 }";
        write_lib_rs(&project, code).expect("write lib.rs");
        let written = std::fs::read_to_string(project.join("src").join("lib.rs")).unwrap();
        assert_eq!(written, code);
    }

    #[test]
    fn repo_root_contains_forge_dir() {
        // Sanity check that `repo_root()` points at the workspace root.
        let root = repo_root();
        assert!(root.join("forge").join("SYSTEM_PROMPT.md").is_file());
        assert!(root.join("oxide-sdk").join("Cargo.toml").is_file());
    }

    #[test]
    fn build_system_prompt_non_empty_and_references_contract() {
        let prompt = build_system_prompt(&repo_root()).expect("build prompt");
        // Must be at least a few KB — the full reference is substantial.
        assert!(prompt.len() > 5_000, "prompt too small: {}", prompt.len());
        // Must embed the core rules section.
        assert!(prompt.contains("Oxide Forge — System Prompt"));
        assert!(prompt.contains("start_app"));
        assert!(prompt.contains("on_frame"));
        // And reference sections.
        assert!(prompt.contains("Reference: Capabilities"));
        assert!(prompt.contains("Reference: Patterns"));
        assert!(prompt.contains("Reference: Recipes"));
    }
}
