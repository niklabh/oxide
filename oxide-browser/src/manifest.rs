//! App manifests: optional TOML metadata shipped next to a `.wasm` module.
//!
//! For a module at `https://host/apps/app.wasm` the browser looks for
//! `https://host/apps/app.toml` (same URL, `.wasm` → `.toml`). The manifest is optional —
//! apps without one keep the legacy behavior (no metadata, every sensitive API may prompt).
//!
//! ```toml
//! name = "Media Capture Demo"
//! description = "Camera preview, mic meter, and screenshots"
//! version = "0.1.0"
//! permissions = ["camera", "microphone", "screen-capture"]
//! ```
//!
//! When a manifest is present it acts as a capability declaration: sensitive APIs **not**
//! listed in `permissions` are denied without prompting (the prompt is only shown for
//! declared permissions). Valid permission names are the [`PermissionKind::name`] values:
//! `camera`, `microphone`, `geolocation`, `screen-capture`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::permissions::PermissionKind;
use crate::url::OxideUrl;

/// Maximum size of a manifest fetched over the network.
const MAX_MANIFEST_SIZE: usize = 64 * 1024; // 64 KiB

/// Parsed app manifest. All fields except `name` are optional in the TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct AppManifest {
    /// Human-readable app name, shown as the tab title.
    pub name: String,
    /// Short description of the app.
    #[serde(default)]
    pub description: String,
    /// App version string (informational).
    #[serde(default)]
    pub version: String,
    /// Sensitive capabilities the app may request (see [`PermissionKind::name`]).
    /// Anything not listed here is denied without a prompt.
    #[serde(default)]
    pub permissions: Vec<String>,
}

impl AppManifest {
    /// Parse a TOML manifest.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Whether the manifest declares `kind` in its `permissions` list.
    pub fn allows(&self, kind: PermissionKind) -> bool {
        self.permissions.iter().any(|p| p == kind.name())
    }
}

/// Shared handle stored in `HostState`; `None` when the current app has no manifest.
pub type SharedManifest = Arc<Mutex<Option<AppManifest>>>;

/// Whether the current app may request `kind`.
///
/// `None` (no manifest) keeps the legacy prompt-on-first-use behavior; a present manifest
/// must declare the permission explicitly.
pub fn manifest_allows(manifest: &SharedManifest, kind: PermissionKind) -> bool {
    match manifest.lock().unwrap().as_ref() {
        Some(m) => m.allows(kind),
        None => true,
    }
}

/// Manifest URL for a `.wasm` module URL (`…/app.wasm` → `…/app.toml`).
///
/// The query string is preserved so signed or versioned module URLs
/// (`app.wasm?token=…`) keep working for the sibling manifest; the fragment is dropped
/// (never sent to the server). Returns `None` when the path doesn't end in `.wasm`.
pub fn manifest_url_for(wasm_url: &OxideUrl) -> Option<String> {
    let url = wasm_url.as_str();
    let without_fragment = url.split('#').next().unwrap_or(url);
    let (path, query) = match without_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (without_fragment, None),
    };
    path.strip_suffix(".wasm").map(|base| match query {
        Some(query) => format!("{base}.toml?{query}"),
        None => format!("{base}.toml"),
    })
}

/// Sibling manifest path for a local `.wasm` file path.
pub fn manifest_path_for(wasm_path: &Path) -> Option<PathBuf> {
    if wasm_path.extension().and_then(|e| e.to_str()) == Some("wasm") {
        Some(wasm_path.with_extension("toml"))
    } else {
        None
    }
}

/// Loads the sibling manifest for a local `.wasm` file.
///
/// Returns `Ok(None)` when no manifest exists, `Err` when one exists but is invalid.
pub fn load_local_manifest(wasm_path: &Path) -> Result<Option<AppManifest>, String> {
    let Some(path) = manifest_path_for(wasm_path) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    AppManifest::parse(&text)
        .map(Some)
        .map_err(|e| format!("invalid manifest {}: {e}", path.display()))
}

/// Fetches the manifest for a module URL (HTTP/HTTPS or `file://`).
///
/// Returns `Ok(None)` when the app has no manifest (missing file, HTTP 404, non-`.wasm`
/// URL), `Err` with a human-readable message when a manifest exists but cannot be parsed.
pub async fn fetch_manifest(wasm_url: &OxideUrl) -> Result<Option<AppManifest>, String> {
    if wasm_url.is_local_file() {
        let Some(path) = wasm_url.to_file_path() else {
            return Ok(None);
        };
        return load_local_manifest(&path);
    }

    if !wasm_url.is_fetchable() {
        return Ok(None);
    }
    let Some(url) = manifest_url_for(wasm_url) else {
        return Ok(None);
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut response = match client.get(&url).send().await {
        Ok(r) => r,
        // Network failure fetching an *optional* file: treat as absent.
        Err(_) => return Ok(None),
    };
    if !response.status().is_success() {
        return Ok(None);
    }
    // Enforce the size cap *while* downloading: reject a too-large Content-Length up
    // front, then stream chunks with a running byte budget so a missing or lying header
    // can't make the host buffer an arbitrarily large body.
    if response
        .content_length()
        .is_some_and(|len| usize::try_from(len).map_or(true, |len| len > MAX_MANIFEST_SIZE))
    {
        return Err(format!(
            "manifest too large (Content-Length exceeds limit {MAX_MANIFEST_SIZE})"
        ));
    }
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if bytes.len() + chunk.len() > MAX_MANIFEST_SIZE {
                    return Err(format!("manifest too large (limit {MAX_MANIFEST_SIZE})"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => return Err(format!("failed to read manifest body: {e}")),
        }
    }
    let text =
        String::from_utf8(bytes).map_err(|_| format!("manifest at {url} is not valid UTF-8"))?;
    AppManifest::parse(&text)
        .map(Some)
        .map_err(|e| format!("invalid manifest at {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_manifest() {
        let m = AppManifest::parse(
            r#"
name = "Demo"
description = "A demo app"
version = "1.2.3"
permissions = ["camera", "geolocation"]
"#,
        )
        .unwrap();
        assert_eq!(m.name, "Demo");
        assert_eq!(m.description, "A demo app");
        assert_eq!(m.version, "1.2.3");
        assert!(m.allows(PermissionKind::Camera));
        assert!(m.allows(PermissionKind::Geolocation));
        assert!(!m.allows(PermissionKind::Microphone));
        assert!(!m.allows(PermissionKind::ScreenCapture));
    }

    #[test]
    fn name_is_required() {
        assert!(AppManifest::parse("version = \"1.0\"").is_err());
    }

    #[test]
    fn defaults_for_optional_fields() {
        let m = AppManifest::parse("name = \"Min\"").unwrap();
        assert_eq!(m.description, "");
        assert_eq!(m.version, "");
        assert!(m.permissions.is_empty());
        assert!(!m.allows(PermissionKind::Camera));
    }

    #[test]
    fn manifest_url_swaps_extension() {
        let url = OxideUrl::parse("https://example.com/apps/demo.wasm").unwrap();
        assert_eq!(
            manifest_url_for(&url).as_deref(),
            Some("https://example.com/apps/demo.toml")
        );
        let not_wasm = OxideUrl::parse("https://example.com/apps/demo").unwrap();
        assert_eq!(manifest_url_for(&not_wasm), None);
    }

    #[test]
    fn manifest_url_preserves_query() {
        let url = OxideUrl::parse("https://example.com/apps/demo.wasm?token=abc&v=2").unwrap();
        assert_eq!(
            manifest_url_for(&url).as_deref(),
            Some("https://example.com/apps/demo.toml?token=abc&v=2")
        );
    }

    #[test]
    fn manifest_path_is_sibling_toml() {
        assert_eq!(
            manifest_path_for(Path::new("/tmp/app.wasm")),
            Some(PathBuf::from("/tmp/app.toml"))
        );
        assert_eq!(manifest_path_for(Path::new("/tmp/app.txt")), None);
    }

    #[test]
    fn no_manifest_allows_everything() {
        let shared: SharedManifest = Arc::new(Mutex::new(None));
        assert!(manifest_allows(&shared, PermissionKind::Camera));
    }

    #[test]
    fn manifest_denies_undeclared() {
        let m = AppManifest::parse("name = \"x\"\npermissions = [\"microphone\"]").unwrap();
        let shared: SharedManifest = Arc::new(Mutex::new(Some(m)));
        assert!(manifest_allows(&shared, PermissionKind::Microphone));
        assert!(!manifest_allows(&shared, PermissionKind::Camera));
    }

    #[test]
    fn load_local_manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let wasm = dir.path().join("app.wasm");
        std::fs::write(dir.path().join("app.toml"), "name = \"Local\"").unwrap();
        let m = load_local_manifest(&wasm).unwrap().unwrap();
        assert_eq!(m.name, "Local");

        let no_manifest = dir.path().join("other.wasm");
        assert!(load_local_manifest(&no_manifest).unwrap().is_none());

        std::fs::write(dir.path().join("bad.toml"), "name = [broken").unwrap();
        assert!(load_local_manifest(&dir.path().join("bad.wasm")).is_err());
    }
}
