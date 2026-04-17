//! Non-blocking / streaming HTTP fetch for Oxide guest modules.
//!
//! The legacy `api_fetch` import (in [`crate::capabilities`]) blocks the guest
//! until the entire response body has been downloaded. That prevents guests
//! from rendering frames during large downloads and makes LLM-style
//! token-streaming, chunked feeds, or progressive image loads impossible.
//!
//! This module exposes a second fetch API that is fully async from the guest's
//! perspective. The shape mirrors [`crate::websocket`]:
//!
//! * `api_fetch_begin` dispatches a request and returns a handle immediately.
//! * `api_fetch_state` / `api_fetch_status` report progress.
//! * `api_fetch_recv` pulls the next body chunk from an in-memory queue.
//! * `api_fetch_abort` cancels an in-flight request.
//! * `api_fetch_remove` frees host-side resources once the guest is done.
//!
//! A background tokio task per request drives `reqwest`'s `bytes_stream()`,
//! pushing chunks into a `VecDeque<Vec<u8>>` drained by the guest.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::runtime::Runtime;
use wasmtime::{Caller, Linker};

use crate::capabilities::{
    console_log, read_guest_bytes, read_guest_string, write_guest_bytes, ConsoleLevel, HostState,
};

// ── Ready-state constants (mirrored in oxide-sdk) ──────────────────────────

/// Request dispatched; waiting for response headers.
pub const FETCH_PENDING: u32 = 0;
/// Headers received; body chunks may be streaming in.
pub const FETCH_STREAMING: u32 = 1;
/// Body fully delivered and the queue may still contain trailing chunks.
pub const FETCH_DONE: u32 = 2;
/// Request failed. Use `api_fetch_error` to retrieve the message.
pub const FETCH_ERROR: u32 = 3;
/// Request was aborted by the guest.
pub const FETCH_ABORTED: u32 = 4;

// ── Recv return-code sentinels (i64, negative) ─────────────────────────────

const RECV_PENDING: i64 = -1;
const RECV_EOF: i64 = -2;
const RECV_ERROR: i64 = -3;
const RECV_UNKNOWN: i64 = -4;

/// Per-request state shared between the host API and the background driver task.
struct FetchInner {
    /// One of the `FETCH_*` constants.
    state: AtomicU32,
    /// HTTP status code once headers arrive, 0 until then.
    status: AtomicU32,
    /// Guest-side signal to stop streaming. Polled by the driver task between chunks.
    aborted: AtomicBool,
    /// Queued body chunks. Each `Vec<u8>` is one network chunk; the guest may
    /// drain them in smaller pieces — see [`FetchState::recv`] for splitting.
    chunks: Mutex<VecDeque<Vec<u8>>>,
    /// Last error message (set once before transitioning to [`FETCH_ERROR`]).
    error: Mutex<Option<String>>,
}

impl FetchInner {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(FETCH_PENDING),
            status: AtomicU32::new(0),
            aborted: AtomicBool::new(false),
            chunks: Mutex::new(VecDeque::new()),
            error: Mutex::new(None),
        }
    }

    fn set_error(&self, msg: impl Into<String>) {
        *self.error.lock().unwrap() = Some(msg.into());
        self.state.store(FETCH_ERROR, Ordering::SeqCst);
    }
}

/// All streaming-fetch state for a host. Lazily initialised on the first
/// `api_fetch_begin` call.
pub struct FetchState {
    runtime: Runtime,
    handles: HashMap<u32, Arc<FetchInner>>,
    next_id: u32,
}

impl FetchState {
    pub fn new() -> Option<Self> {
        let runtime = Runtime::new().ok()?;
        Some(Self {
            runtime,
            handles: HashMap::new(),
            next_id: 1,
        })
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Dispatch a new request. Returns a handle (`> 0`) or `0` on init failure.
    fn begin(&mut self, method: String, url: String, content_type: String, body: Vec<u8>) -> u32 {
        let id = self.alloc_id();
        let inner = Arc::new(FetchInner::new());
        let driver_inner = inner.clone();

        self.runtime.spawn(async move {
            drive_request(driver_inner, method, url, content_type, body).await;
        });

        self.handles.insert(id, inner);
        id
    }

    fn state(&self, id: u32) -> u32 {
        self.handles
            .get(&id)
            .map(|h| h.state.load(Ordering::SeqCst))
            .unwrap_or(FETCH_ERROR)
    }

    fn status(&self, id: u32) -> u32 {
        self.handles
            .get(&id)
            .map(|h| h.status.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn error(&self, id: u32) -> Option<String> {
        self.handles
            .get(&id)
            .and_then(|h| h.error.lock().unwrap().clone())
    }

    /// Pop up to `cap` bytes from the head of the body queue.
    ///
    /// Returns one of the negative sentinels (`RECV_*`) or a non-negative byte
    /// count. When a single queued chunk is larger than `cap`, the extra bytes
    /// are re-queued at the front so the next call returns them — no data is
    /// dropped.
    fn recv(&self, id: u32, cap: usize) -> RecvResult {
        let Some(inner) = self.handles.get(&id) else {
            return RecvResult::Sentinel(RECV_UNKNOWN);
        };

        let mut q = inner.chunks.lock().unwrap();
        if let Some(mut chunk) = q.pop_front() {
            if chunk.len() > cap {
                let remainder = chunk.split_off(cap);
                q.push_front(remainder);
            }
            return RecvResult::Data(chunk);
        }
        drop(q);

        match inner.state.load(Ordering::SeqCst) {
            FETCH_DONE => RecvResult::Sentinel(RECV_EOF),
            FETCH_ERROR | FETCH_ABORTED => RecvResult::Sentinel(RECV_ERROR),
            _ => RecvResult::Sentinel(RECV_PENDING),
        }
    }

    fn abort(&self, id: u32) -> bool {
        if let Some(inner) = self.handles.get(&id) {
            inner.aborted.store(true, Ordering::SeqCst);
            // Flip state immediately so `state()` reports the intent even
            // before the driver task observes the flag.
            let _ = inner.state.compare_exchange(
                FETCH_PENDING,
                FETCH_ABORTED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            let _ = inner.state.compare_exchange(
                FETCH_STREAMING,
                FETCH_ABORTED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            true
        } else {
            false
        }
    }

    fn remove(&mut self, id: u32) {
        if let Some(inner) = self.handles.remove(&id) {
            inner.aborted.store(true, Ordering::SeqCst);
        }
    }
}

enum RecvResult {
    Data(Vec<u8>),
    Sentinel(i64),
}

/// Background task body: run the request and feed chunks into `inner`.
async fn drive_request(
    inner: Arc<FetchInner>,
    method: String,
    url: String,
    content_type: String,
    body: Vec<u8>,
) {
    if inner.aborted.load(Ordering::SeqCst) {
        inner.state.store(FETCH_ABORTED, Ordering::SeqCst);
        return;
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            inner.set_error(format!("client build failed: {e}"));
            return;
        }
    };

    let parsed_method: reqwest::Method = method.parse().unwrap_or(reqwest::Method::GET);
    let mut req = client.request(parsed_method, &url);
    if !content_type.is_empty() {
        req = req.header("Content-Type", &content_type);
    }
    if !body.is_empty() {
        req = req.body(body);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            inner.set_error(e.to_string());
            return;
        }
    };

    if inner.aborted.load(Ordering::SeqCst) {
        inner.state.store(FETCH_ABORTED, Ordering::SeqCst);
        return;
    }

    inner
        .status
        .store(resp.status().as_u16() as u32, Ordering::SeqCst);
    inner.state.store(FETCH_STREAMING, Ordering::SeqCst);

    let mut stream = resp.bytes_stream();
    while let Some(next) = stream.next().await {
        if inner.aborted.load(Ordering::SeqCst) {
            inner.state.store(FETCH_ABORTED, Ordering::SeqCst);
            return;
        }
        match next {
            Ok(chunk) => {
                if !chunk.is_empty() {
                    inner.chunks.lock().unwrap().push_back(chunk.to_vec());
                }
            }
            Err(e) => {
                inner.set_error(e.to_string());
                return;
            }
        }
    }

    // Preserve ABORTED if the guest aborted between the final chunk and here.
    let _ = inner.state.compare_exchange(
        FETCH_STREAMING,
        FETCH_DONE,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

fn ensure_fetch(state: &Arc<Mutex<Option<FetchState>>>) -> bool {
    let mut g = state.lock().unwrap();
    if g.is_none() {
        *g = FetchState::new();
    }
    g.is_some()
}

/// Register all `api_fetch_*` streaming host functions on the given linker.
pub fn register_fetch_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // ── fetch_begin ──────────────────────────────────────────────────────
    // api_fetch_begin(method_ptr, method_len, url_ptr, url_len,
    //                 ct_ptr, ct_len, body_ptr, body_len) -> u32
    //   Returns a handle (> 0), or 0 on error.
    linker.func_wrap(
        "oxide",
        "api_fetch_begin",
        |caller: Caller<'_, HostState>,
         method_ptr: u32,
         method_len: u32,
         url_ptr: u32,
         url_len: u32,
         ct_ptr: u32,
         ct_len: u32,
         body_ptr: u32,
         body_len: u32|
         -> u32 {
            let console = caller.data().console.clone();
            let fetch = caller.data().fetch.clone();
            if !ensure_fetch(&fetch) {
                console_log(&console, ConsoleLevel::Error, "[FETCH] Init failed".into());
                return 0;
            }
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let method =
                read_guest_string(&mem, &caller, method_ptr, method_len).unwrap_or_default();
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();
            let content_type = read_guest_string(&mem, &caller, ct_ptr, ct_len).unwrap_or_default();
            let body = if body_len > 0 {
                read_guest_bytes(&mem, &caller, body_ptr, body_len).unwrap_or_default()
            } else {
                Vec::new()
            };

            let id = fetch.lock().unwrap().as_mut().unwrap().begin(
                method.clone(),
                url.clone(),
                content_type,
                body,
            );

            console_log(
                &console,
                ConsoleLevel::Log,
                format!("[FETCH] {method} {url} (id={id})"),
            );
            id
        },
    )?;

    // ── fetch_state ──────────────────────────────────────────────────────
    // api_fetch_state(id) -> u32  (one of FETCH_* constants)
    linker.func_wrap(
        "oxide",
        "api_fetch_state",
        |caller: Caller<'_, HostState>, id: u32| -> u32 {
            let fetch = caller.data().fetch.clone();
            let g = fetch.lock().unwrap();
            g.as_ref().map(|s| s.state(id)).unwrap_or(FETCH_ERROR)
        },
    )?;

    // ── fetch_status ─────────────────────────────────────────────────────
    // api_fetch_status(id) -> u32  (HTTP status, 0 before headers)
    linker.func_wrap(
        "oxide",
        "api_fetch_status",
        |caller: Caller<'_, HostState>, id: u32| -> u32 {
            let fetch = caller.data().fetch.clone();
            let g = fetch.lock().unwrap();
            g.as_ref().map(|s| s.status(id)).unwrap_or(0)
        },
    )?;

    // ── fetch_recv ───────────────────────────────────────────────────────
    // api_fetch_recv(id, out_ptr, out_cap) -> i64
    //   >= 0 : bytes written into `out_ptr` (one chunk, possibly partial)
    //   -1   : pending (no chunk available, more may arrive)
    //   -2   : end of stream (body fully delivered)
    //   -3   : error (see api_fetch_error)
    //   -4   : unknown handle
    linker.func_wrap(
        "oxide",
        "api_fetch_recv",
        |mut caller: Caller<'_, HostState>, id: u32, out_ptr: u32, out_cap: u32| -> i64 {
            let fetch = caller.data().fetch.clone();
            let result = {
                let g = fetch.lock().unwrap();
                match g.as_ref() {
                    Some(s) => s.recv(id, out_cap as usize),
                    None => return RECV_UNKNOWN,
                }
            };
            match result {
                RecvResult::Sentinel(code) => code,
                RecvResult::Data(bytes) => {
                    let mem = match caller.data().memory {
                        Some(m) => m,
                        None => return RECV_ERROR,
                    };
                    if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes).is_err() {
                        return RECV_ERROR;
                    }
                    bytes.len() as i64
                }
            }
        },
    )?;

    // ── fetch_error ──────────────────────────────────────────────────────
    // api_fetch_error(id, out_ptr, out_cap) -> i32
    //   >= 0 : number of UTF-8 bytes written (possibly truncated)
    //   -1   : no error set
    linker.func_wrap(
        "oxide",
        "api_fetch_error",
        |mut caller: Caller<'_, HostState>, id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let fetch = caller.data().fetch.clone();
            let msg = {
                let g = fetch.lock().unwrap();
                g.as_ref().and_then(|s| s.error(id))
            };
            let msg = match msg {
                Some(m) => m,
                None => return -1,
            };
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let bytes = msg.as_bytes();
            let n = bytes.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..n]).is_err() {
                return -1;
            }
            n as i32
        },
    )?;

    // ── fetch_abort ──────────────────────────────────────────────────────
    // api_fetch_abort(id) -> i32  (1 if aborted, 0 if unknown handle)
    linker.func_wrap(
        "oxide",
        "api_fetch_abort",
        |caller: Caller<'_, HostState>, id: u32| -> i32 {
            let fetch = caller.data().fetch.clone();
            let g = fetch.lock().unwrap();
            match g.as_ref() {
                Some(s) => i32::from(s.abort(id)),
                None => 0,
            }
        },
    )?;

    // ── fetch_remove ─────────────────────────────────────────────────────
    // api_fetch_remove(id)  — free host-side resources.
    linker.func_wrap(
        "oxide",
        "api_fetch_remove",
        |caller: Caller<'_, HostState>, id: u32| {
            let fetch = caller.data().fetch.clone();
            let mut g = fetch.lock().unwrap();
            if let Some(ref mut state) = *g {
                state.remove(id);
            }
        },
    )?;

    Ok(())
}
