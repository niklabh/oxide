//! Server-Sent Events for Oxide guest modules.
//!
//! Guests call `api_sse_open` to start an EventSource-style stream. The host
//! drives the HTTP connection on a background tokio task, parses the SSE
//! wire format, and queues events for `api_sse_recv`. The stream reconnects
//! automatically with `Last-Event-ID`, matching the web `EventSource` contract.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use tokio::runtime::Runtime;
use wasmtime::{Caller, Linker};

use crate::capabilities::{
    console_log, read_guest_string, write_guest_bytes, ConsoleLevel, HostState,
};

/// Connection is being established (or reconnecting).
pub const SSE_CONNECTING: u32 = 0;
/// Stream is open; events may be queued.
pub const SSE_OPEN: u32 = 1;
/// Stream was closed by the guest or a terminal HTTP status (e.g. 204).
pub const SSE_CLOSED: u32 = 2;
/// Last attempt failed. Use `api_sse_error` for the message; reconnect may still run.
pub const SSE_ERROR: u32 = 3;

const RECV_PENDING: i64 = -1;
const RECV_CLOSED: i64 = -2;
const RECV_ERROR: i64 = -3;
const RECV_UNKNOWN: i64 = -4;

const MAX_QUEUE: usize = 128;
const MAX_EVENT_BYTES: usize = 256 * 1024;
const DEFAULT_RETRY_MS: u64 = 3_000;
const MIN_RETRY_MS: u64 = 500;
const MAX_RETRY_MS: u64 = 60_000;

/// One parsed SSE event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub name: String,
    pub id: String,
    pub data: String,
}

/// Incremental SSE parser (WHATWG HTML Living Standard, event stream).
pub struct SseParser {
    buf: Vec<u8>,
    name: String,
    id: String,
    data_lines: Vec<String>,
    /// Last `retry:` value seen, if any.
    pub retry_ms: Option<u64>,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            name: String::new(),
            id: String::new(),
            data_lines: Vec::new(),
            retry_ms: None,
        }
    }

    /// Feed bytes and return every complete event dispatched.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(line) = self.take_line() {
            if line.is_empty() {
                if let Some(ev) = self.dispatch() {
                    events.push(ev);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = split_field(&line);
            match field {
                "event" => self.name = value.to_string(),
                "data" => self.data_lines.push(value.to_string()),
                "id" if !value.contains('\0') => self.id = value.to_string(),
                "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                    if let Ok(ms) = value.parse::<u64>() {
                        self.retry_ms = Some(ms.clamp(MIN_RETRY_MS, MAX_RETRY_MS));
                    }
                }
                _ => {}
            }
        }
        events
    }

    fn take_line(&mut self) -> Option<String> {
        let pos = self.buf.iter().position(|&b| b == b'\n' || b == b'\r')?;
        let line = self.buf.drain(..pos).collect::<Vec<u8>>();
        if self.buf.first() == Some(&b'\r') {
            self.buf.drain(..1);
            if self.buf.first() == Some(&b'\n') {
                self.buf.drain(..1);
            }
        } else if self.buf.first() == Some(&b'\n') {
            self.buf.drain(..1);
        }
        String::from_utf8(line).ok()
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data_lines.is_empty() {
            self.name.clear();
            return None;
        }
        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        let name = if self.name.is_empty() {
            "message".to_string()
        } else {
            std::mem::take(&mut self.name)
        };
        let id = self.id.clone();
        if data.len() > MAX_EVENT_BYTES {
            return None;
        }
        Some(SseEvent { name, id, data })
    }
}

fn split_field(line: &str) -> (&str, &str) {
    match line.split_once(':') {
        Some((field, rest)) => {
            let value = rest.strip_prefix(' ').unwrap_or(rest);
            (field, value)
        }
        None => (line, ""),
    }
}

/// Encode an event for the guest: `u16le name | name | u16le id | id | u32le data | data`.
pub fn encode_event(ev: &SseEvent) -> Vec<u8> {
    let name = ev.name.as_bytes();
    let id = ev.id.as_bytes();
    let data = ev.data.as_bytes();
    let name_len = (name.len().min(u16::MAX as usize)) as u16;
    let id_len = (id.len().min(u16::MAX as usize)) as u16;
    let data_len = data.len() as u32;
    let mut out = Vec::with_capacity(8 + name.len() + id.len() + data.len());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&name[..name_len as usize]);
    out.extend_from_slice(&id_len.to_le_bytes());
    out.extend_from_slice(&id[..id_len as usize]);
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(data);
    out
}

struct SseInner {
    state: AtomicU32,
    aborted: AtomicBool,
    events: Mutex<VecDeque<SseEvent>>,
    error: Mutex<Option<String>>,
    last_id: Mutex<String>,
    retry_ms: AtomicU64,
}

impl SseInner {
    fn new() -> Self {
        Self {
            state: AtomicU32::new(SSE_CONNECTING),
            aborted: AtomicBool::new(false),
            events: Mutex::new(VecDeque::new()),
            error: Mutex::new(None),
            last_id: Mutex::new(String::new()),
            retry_ms: AtomicU64::new(DEFAULT_RETRY_MS),
        }
    }

    fn push_events(&self, events: Vec<SseEvent>) {
        if events.is_empty() {
            return;
        }
        let mut q = self.events.lock().unwrap();
        for ev in events {
            if !ev.id.is_empty() {
                *self.last_id.lock().unwrap() = ev.id.clone();
            }
            if q.len() >= MAX_QUEUE {
                q.pop_front();
            }
            q.push_back(ev);
        }
    }

    fn set_error(&self, msg: impl Into<String>) {
        *self.error.lock().unwrap() = Some(msg.into());
        self.state.store(SSE_ERROR, Ordering::SeqCst);
    }
}

/// All SSE streams for a tab. Lazily initialised on the first `api_sse_*` call.
pub struct SseState {
    runtime: Runtime,
    streams: HashMap<u32, Arc<SseInner>>,
    next_id: u32,
}

impl SseState {
    pub fn new() -> Option<Self> {
        let runtime = Runtime::new().ok()?;
        Some(Self {
            runtime,
            streams: HashMap::new(),
            next_id: 1,
        })
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn open(&mut self, url: String) -> u32 {
        let id = self.alloc_id();
        let inner = Arc::new(SseInner::new());
        let driver = inner.clone();
        self.runtime.spawn(async move {
            drive_stream(driver, url).await;
        });
        self.streams.insert(id, inner);
        id
    }

    fn state(&self, id: u32) -> u32 {
        self.streams
            .get(&id)
            .map(|s| s.state.load(Ordering::SeqCst))
            .unwrap_or(SSE_CLOSED)
    }

    fn pop(&self, id: u32) -> Option<SseEvent> {
        self.streams
            .get(&id)
            .and_then(|s| s.events.lock().unwrap().pop_front())
    }

    fn peek_front(&self, id: u32) -> Option<SseEvent> {
        self.streams
            .get(&id)
            .and_then(|s| s.events.lock().unwrap().front().cloned())
    }

    fn empty_recv_code(&self, id: u32) -> i64 {
        let Some(inner) = self.streams.get(&id) else {
            return RECV_UNKNOWN;
        };
        match inner.state.load(Ordering::SeqCst) {
            SSE_CLOSED => RECV_CLOSED,
            SSE_ERROR => RECV_ERROR,
            _ => RECV_PENDING,
        }
    }

    fn close(&self, id: u32) -> bool {
        let Some(inner) = self.streams.get(&id) else {
            return false;
        };
        inner.aborted.store(true, Ordering::SeqCst);
        inner.state.store(SSE_CLOSED, Ordering::SeqCst);
        true
    }

    fn remove(&mut self, id: u32) {
        if let Some(inner) = self.streams.remove(&id) {
            inner.aborted.store(true, Ordering::SeqCst);
        }
    }

    fn error(&self, id: u32) -> Option<String> {
        self.streams
            .get(&id)
            .and_then(|s| s.error.lock().unwrap().clone())
    }
}

async fn drive_stream(inner: Arc<SseInner>, url: String) {
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(0)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            inner.set_error(format!("client build failed: {e}"));
            return;
        }
    };

    loop {
        if inner.aborted.load(Ordering::SeqCst) {
            inner.state.store(SSE_CLOSED, Ordering::SeqCst);
            return;
        }

        inner.state.store(SSE_CONNECTING, Ordering::SeqCst);
        let last_id = inner.last_id.lock().unwrap().clone();
        let mut req = client
            .get(&url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");
        if !last_id.is_empty() {
            req = req.header("Last-Event-ID", &last_id);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                if inner.aborted.load(Ordering::SeqCst) {
                    inner.state.store(SSE_CLOSED, Ordering::SeqCst);
                    return;
                }
                inner.set_error(e.to_string());
                sleep_retry(&inner).await;
                continue;
            }
        };

        let status = resp.status();
        if status.as_u16() == 204 {
            inner.state.store(SSE_CLOSED, Ordering::SeqCst);
            return;
        }
        if !status.is_success() {
            inner.set_error(format!("HTTP {status}"));
            if inner.aborted.load(Ordering::SeqCst) {
                inner.state.store(SSE_CLOSED, Ordering::SeqCst);
                return;
            }
            sleep_retry(&inner).await;
            continue;
        }

        inner.state.store(SSE_OPEN, Ordering::SeqCst);
        let mut parser = SseParser::new();
        let mut stream = resp.bytes_stream();
        while let Some(next) = stream.next().await {
            if inner.aborted.load(Ordering::SeqCst) {
                inner.state.store(SSE_CLOSED, Ordering::SeqCst);
                return;
            }
            match next {
                Ok(chunk) => {
                    let events = parser.push(&chunk);
                    if let Some(ms) = parser.retry_ms.take() {
                        inner.retry_ms.store(ms, Ordering::SeqCst);
                    }
                    inner.push_events(events);
                }
                Err(e) => {
                    if inner.aborted.load(Ordering::SeqCst) {
                        inner.state.store(SSE_CLOSED, Ordering::SeqCst);
                        return;
                    }
                    inner.set_error(e.to_string());
                    break;
                }
            }
        }

        if inner.aborted.load(Ordering::SeqCst) {
            inner.state.store(SSE_CLOSED, Ordering::SeqCst);
            return;
        }
        // Stream ended — reconnect unless the guest closed it.
        sleep_retry(&inner).await;
    }
}

async fn sleep_retry(inner: &SseInner) {
    let ms = inner.retry_ms.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn ensure_sse(state: &Arc<Mutex<Option<SseState>>>) -> bool {
    let mut g = state.lock().unwrap();
    if g.is_none() {
        *g = SseState::new();
    }
    g.is_some()
}

/// Register `api_sse_*` host functions.
pub fn register_sse_functions(linker: &mut Linker<HostState>) -> Result<()> {
    linker.func_wrap(
        "oxide",
        "api_sse_open",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> u32 {
            let console = caller.data().console.clone();
            let sse = caller.data().sse.clone();
            if !ensure_sse(&sse) {
                console_log(&console, ConsoleLevel::Error, "[SSE] Init failed".into());
                return 0;
            }
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let url = match read_guest_string(&mem, &caller, url_ptr, url_len) {
                Ok(s) if !s.is_empty() => s,
                _ => return 0,
            };
            let id = sse.lock().unwrap().as_mut().unwrap().open(url.clone());
            console_log(
                &console,
                ConsoleLevel::Log,
                format!("[SSE] Opening {url} (id={id})"),
            );
            id
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_sse_state",
        |caller: Caller<'_, HostState>, id: u32| -> u32 {
            let sse = caller.data().sse.clone();
            let g = sse.lock().unwrap();
            g.as_ref().map(|s| s.state(id)).unwrap_or(SSE_CLOSED)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_sse_recv",
        |mut caller: Caller<'_, HostState>, id: u32, out_ptr: u32, out_cap: u32| -> i64 {
            let sse = caller.data().sse.clone();
            let encoded = {
                let g = sse.lock().unwrap();
                let Some(state) = g.as_ref() else {
                    return RECV_UNKNOWN;
                };
                match state.peek_front(id) {
                    Some(ev) => encode_event(&ev),
                    None => return state.empty_recv_code(id),
                }
            };
            if encoded.len() > out_cap as usize {
                return encoded.len() as i64;
            }
            // Consume now that we know it fits.
            {
                let g = sse.lock().unwrap();
                let _ = g.as_ref().and_then(|s| s.pop(id));
            }
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return RECV_UNKNOWN,
            };
            if write_guest_bytes(&mem, &mut caller, out_ptr, &encoded).is_err() {
                return RECV_ERROR;
            }
            encoded.len() as i64
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_sse_error",
        |mut caller: Caller<'_, HostState>, id: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let sse = caller.data().sse.clone();
            let msg = {
                let g = sse.lock().unwrap();
                g.as_ref().and_then(|s| s.error(id)).unwrap_or_default()
            };
            if msg.is_empty() {
                return 0;
            }
            let bytes = msg.as_bytes();
            if bytes.len() > out_cap as usize {
                return bytes.len() as i32;
            }
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            if write_guest_bytes(&mem, &mut caller, out_ptr, bytes).is_err() {
                return 0;
            }
            bytes.len() as i32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_sse_close",
        |caller: Caller<'_, HostState>, id: u32| -> i32 {
            let sse = caller.data().sse.clone();
            let g = sse.lock().unwrap();
            i32::from(g.as_ref().is_some_and(|s| s.close(id)))
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_sse_remove",
        |caller: Caller<'_, HostState>, id: u32| {
            let sse = caller.data().sse.clone();
            let mut g = sse.lock().unwrap();
            if let Some(ref mut state) = *g {
                state.remove(id);
            }
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_message() {
        let mut p = SseParser::new();
        let ev = p.push(b"data: hello\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                name: "message".into(),
                id: String::new(),
                data: "hello".into(),
            }]
        );
    }

    #[test]
    fn parse_multiline_data_and_name() {
        let mut p = SseParser::new();
        let ev = p.push(b"event: ping\ndata: one\ndata: two\nid: 7\n\n");
        assert_eq!(
            ev,
            vec![SseEvent {
                name: "ping".into(),
                id: "7".into(),
                data: "one\ntwo".into(),
            }]
        );
    }

    #[test]
    fn comments_and_retry_are_handled() {
        let mut p = SseParser::new();
        let ev = p.push(b": keep-alive\nretry: 2500\ndata: x\n\n");
        assert_eq!(p.retry_ms, Some(2500));
        assert_eq!(ev[0].data, "x");
    }

    #[test]
    fn empty_dispatch_emits_nothing() {
        let mut p = SseParser::new();
        assert!(p.push(b"event: ignored\n\n").is_empty());
    }

    #[test]
    fn incremental_chunks_across_lines() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: hel").is_empty());
        assert!(p.push(b"lo\n").is_empty());
        let ev = p.push(b"\n");
        assert_eq!(ev[0].data, "hello");
    }

    #[test]
    fn crlf_lines() {
        let mut p = SseParser::new();
        let ev = p.push(b"data: crlf\r\n\r\n");
        assert_eq!(ev[0].data, "crlf");
    }

    #[test]
    fn encode_roundtrip_layout() {
        let ev = SseEvent {
            name: "update".into(),
            id: "42".into(),
            data: "abc".into(),
        };
        let bytes = encode_event(&ev);
        assert_eq!(&bytes[0..2], &6u16.to_le_bytes());
        assert_eq!(&bytes[2..8], b"update");
    }
}
