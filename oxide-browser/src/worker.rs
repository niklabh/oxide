//! Background WebAssembly workers for Oxide guest modules.
//!
//! A worker is a separate `.wasm` module instantiated on its own OS thread with
//! its own Wasmtime [`Store`], fuel budget, and linear memory. It never shares
//! Rust types or memory with the spawning guest — communication is pure message
//! passing of byte slices, mirroring the rest of the FFI boundary.
//!
//! The spawning ("parent") guest calls:
//! - `api_spawn_worker(url)` → handle
//! - `api_worker_post_message(handle, bytes)` — parent → worker inbox
//! - `api_worker_recv(handle)` — drain one message from the worker's outbox
//! - `api_worker_terminate(handle)`
//!
//! The worker module exports `start_app()` (run once on spawn) and optionally
//! `on_message(len)` (run for each inbound message). Inside `on_message` it reads
//! the payload with `api_worker_message_read` and replies with `api_worker_post`.
//!
//! All of this is driven by polling from the guest's frame loop, matching the
//! WebSocket / fetch / RTC subsystems.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use wasmtime::*;

use crate::capabilities::{
    console_log, read_guest_bytes, read_guest_string, register_host_functions, write_guest_bytes,
    ConsoleEntry, ConsoleLevel, HostState,
};
use crate::engine::ModuleLoader;
use crate::url::OxideUrl;

/// Message handed to a worker thread over its inbox channel.
enum Inbox {
    /// A byte payload to deliver to the worker's `on_message` export.
    Message(Vec<u8>),
    /// Request the worker thread to stop.
    Terminate,
}

/// Parent-side handle to a single running worker.
struct Worker {
    /// Sender for the worker's inbox (parent → worker).
    inbox_tx: Sender<Inbox>,
    /// Messages the worker posted back (worker → parent), drained by `api_worker_recv`.
    outbox: Arc<Mutex<VecDeque<Vec<u8>>>>,
    /// Cleared on terminate so the worker loop exits after its current callback.
    alive: Arc<AtomicBool>,
}

/// Registry of workers spawned by one guest. Lazily created on first spawn.
pub struct WorkerState {
    workers: HashMap<u32, Worker>,
    next_id: u32,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            workers: HashMap::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Spawn a worker for `url`, deriving its sandbox state from `parent`.
    ///
    /// Returns a handle (`> 0`) or `0` if no module loader is available. The
    /// worker shares only the persistent KV store and console with its parent;
    /// canvas, input, timers, and memory are isolated.
    fn spawn(&mut self, url: String, parent: &HostState) -> u32 {
        let loader = match parent.module_loader.clone() {
            Some(l) => l,
            None => return 0,
        };

        let id = self.alloc_id();
        let (inbox_tx, inbox_rx) = std::sync::mpsc::channel::<Inbox>();
        let outbox: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::new()));
        let alive = Arc::new(AtomicBool::new(true));

        let child = HostState {
            module_loader: Some(loader.clone()),
            kv_db: parent.kv_db.clone(),
            console: parent.console.clone(),
            current_url: Arc::new(Mutex::new(url.clone())),
            worker_outbox: Some(outbox.clone()),
            worker_current_msg: Arc::new(Mutex::new(None)),
            ..Default::default()
        };

        let console = parent.console.clone();
        let alive_thread = alive.clone();
        std::thread::spawn(move || {
            worker_main(url, loader, child, inbox_rx, alive_thread, console);
        });

        self.workers.insert(
            id,
            Worker {
                inbox_tx,
                outbox,
                alive,
            },
        );
        id
    }

    fn post(&self, id: u32, data: Vec<u8>) -> bool {
        match self.workers.get(&id) {
            Some(w) => w.inbox_tx.send(Inbox::Message(data)).is_ok(),
            None => false,
        }
    }

    fn recv(&self, id: u32) -> Option<Vec<u8>> {
        self.workers.get(&id)?.outbox.lock().unwrap().pop_front()
    }

    fn terminate(&mut self, id: u32) -> bool {
        match self.workers.remove(&id) {
            Some(w) => {
                w.alive.store(false, Ordering::Relaxed);
                let _ = w.inbox_tx.send(Inbox::Terminate);
                true
            }
            None => false,
        }
    }
}

fn ensure_workers(state: &Arc<Mutex<Option<WorkerState>>>) {
    let mut g = state.lock().unwrap();
    if g.is_none() {
        *g = Some(WorkerState::new());
    }
}

/// Synchronously fetch a worker module's bytes. Supports `http(s)` and `file://`.
fn fetch_worker_bytes(url: &str) -> Result<Vec<u8>> {
    let parsed = OxideUrl::parse(url).map_err(|e| anyhow::anyhow!("{e}"))?;
    if parsed.is_fetchable() {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let resp = client
            .get(parsed.as_str())
            .header("Accept", "application/wasm")
            .send()?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}", resp.status());
        }
        Ok(resp.bytes()?.to_vec())
    } else if parsed.is_local_file() {
        let path = parsed
            .to_file_path()
            .ok_or_else(|| anyhow::anyhow!("cannot convert file URL to path: {url}"))?;
        Ok(std::fs::read(&path)?)
    } else {
        anyhow::bail!("unsupported worker URL scheme: {}", parsed.scheme())
    }
}

/// Body of a worker thread: fetch, compile, instantiate, run `start_app`, then
/// loop delivering inbound messages to `on_message` until terminated.
fn worker_main(
    url: String,
    loader: Arc<ModuleLoader>,
    host_state: HostState,
    inbox_rx: Receiver<Inbox>,
    alive: Arc<AtomicBool>,
    console: Arc<Mutex<Vec<ConsoleEntry>>>,
) {
    let wasm_bytes = match fetch_worker_bytes(&url) {
        Ok(b) => b,
        Err(e) => {
            console_log(
                &console,
                ConsoleLevel::Error,
                format!("[WORKER] fetch failed for {url}: {e}"),
            );
            return;
        }
    };

    let module = match Module::new(&loader.engine, &wasm_bytes) {
        Ok(m) => m,
        Err(e) => {
            console_log(
                &console,
                ConsoleLevel::Error,
                format!("[WORKER] compile failed: {e}"),
            );
            return;
        }
    };

    let mut store = Store::new(&loader.engine, host_state);
    if store.set_fuel(loader.fuel_limit).is_err() {
        return;
    }

    let mut linker = Linker::new(&loader.engine);
    if register_host_functions(&mut linker).is_err() {
        return;
    }

    let mem_type = MemoryType::new(1, Some(loader.max_memory_pages));
    let memory = match Memory::new(&mut store, mem_type) {
        Ok(m) => m,
        Err(_) => return,
    };
    if linker.define(&store, "oxide", "memory", memory).is_err() {
        return;
    }
    store.data_mut().memory = Some(memory);

    let instance = match linker.instantiate(&mut store, &module) {
        Ok(i) => i,
        Err(e) => {
            console_log(
                &console,
                ConsoleLevel::Error,
                format!("[WORKER] instantiate failed: {e}"),
            );
            return;
        }
    };

    if let Some(guest_mem) = instance.get_memory(&mut store, "memory") {
        store.data_mut().memory = Some(guest_mem);
    }

    if let Ok(start_app) = instance.get_typed_func::<(), ()>(&mut store, "start_app") {
        let _ = store.set_fuel(loader.fuel_limit);
        if let Err(e) = start_app.call(&mut store, ()) {
            console_log(
                &console,
                ConsoleLevel::Error,
                format!("[WORKER] start_app trapped: {e}"),
            );
            return;
        }
    }

    let on_message = instance
        .get_typed_func::<u32, ()>(&mut store, "on_message")
        .ok();

    while alive.load(Ordering::Relaxed) {
        let bytes = match inbox_rx.recv() {
            Ok(Inbox::Message(b)) => b,
            Ok(Inbox::Terminate) | Err(_) => break,
        };
        let Some(ref on_message) = on_message else {
            continue;
        };
        let len = bytes.len() as u32;
        *store.data().worker_current_msg.lock().unwrap() = Some(bytes);
        let _ = store.set_fuel(loader.fuel_limit);
        if let Err(e) = on_message.call(&mut store, len) {
            let msg = if e.to_string().contains("fuel") {
                "[WORKER] on_message fuel limit exceeded".to_string()
            } else {
                format!("[WORKER] on_message trapped: {e}")
            };
            console_log(&console, ConsoleLevel::Error, msg);
            break;
        }
        *store.data().worker_current_msg.lock().unwrap() = None;
    }
}

/// Register all `api_worker_*` / `api_spawn_worker` host functions.
pub fn register_worker_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // ── spawn_worker ────────────────────────────────────────────────────────
    // api_spawn_worker(url_ptr: u32, url_len: u32) -> i32
    //   Returns a worker handle (> 0), or -1 on error.
    linker.func_wrap(
        "oxide",
        "api_spawn_worker",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> i32 {
            let console = caller.data().console.clone();
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let url = match read_guest_string(&mem, &caller, url_ptr, url_len) {
                Ok(s) => s,
                Err(_) => return -1,
            };
            let workers = caller.data().workers.clone();
            ensure_workers(&workers);
            let id = workers
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .spawn(url.clone(), caller.data());
            if id == 0 {
                console_log(
                    &console,
                    ConsoleLevel::Error,
                    "[WORKER] spawn failed (no module loader)".into(),
                );
                return -1;
            }
            console_log(
                &console,
                ConsoleLevel::Log,
                format!("[WORKER] spawned {url} (handle={id})"),
            );
            id as i32
        },
    )?;

    // ── worker_post_message ───────────────────────────────────────────────
    // api_worker_post_message(handle: u32, ptr: u32, len: u32) -> i32
    //   Parent → worker. Returns 0 on success, -1 if the handle is unknown.
    linker.func_wrap(
        "oxide",
        "api_worker_post_message",
        |caller: Caller<'_, HostState>, handle: u32, ptr: u32, len: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let data = match read_guest_bytes(&mem, &caller, ptr, len) {
                Ok(b) => b,
                Err(_) => return -1,
            };
            let workers = caller.data().workers.clone();
            let g = workers.lock().unwrap();
            match g.as_ref() {
                Some(s) if s.post(handle, data) => 0,
                _ => -1,
            }
        },
    )?;

    // ── worker_recv ─────────────────────────────────────────────────────────
    // api_worker_recv(handle: u32, out_ptr: u32, out_cap: u32) -> i64
    //   Worker → parent. -1 if no message is queued, else the byte length written.
    linker.func_wrap(
        "oxide",
        "api_worker_recv",
        |mut caller: Caller<'_, HostState>, handle: u32, out_ptr: u32, out_cap: u32| -> i64 {
            let workers = caller.data().workers.clone();
            let msg = {
                let g = workers.lock().unwrap();
                g.as_ref().and_then(|s| s.recv(handle))
            };
            let msg = match msg {
                Some(m) => m,
                None => return -1,
            };
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let to_write = if msg.len() > out_cap as usize {
                &msg[..out_cap as usize]
            } else {
                &msg[..]
            };
            if write_guest_bytes(&mem, &mut caller, out_ptr, to_write).is_err() {
                return -1;
            }
            to_write.len() as i64
        },
    )?;

    // ── worker_terminate ──────────────────────────────────────────────────
    // api_worker_terminate(handle: u32) -> i32
    //   Returns 1 if the worker was running, 0 if the handle is unknown.
    linker.func_wrap(
        "oxide",
        "api_worker_terminate",
        |caller: Caller<'_, HostState>, handle: u32| -> i32 {
            let workers = caller.data().workers.clone();
            let mut g = workers.lock().unwrap();
            let terminated = g.as_mut().map(|s| s.terminate(handle)).unwrap_or(false);
            i32::from(terminated)
        },
    )?;

    // ── worker_post (worker side) ─────────────────────────────────────────
    // api_worker_post(ptr: u32, len: u32) -> i32
    //   Worker → its spawning parent. -1 if not running inside a worker.
    linker.func_wrap(
        "oxide",
        "api_worker_post",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -1,
            };
            let data = match read_guest_bytes(&mem, &caller, ptr, len) {
                Ok(b) => b,
                Err(_) => return -1,
            };
            match caller.data().worker_outbox.clone() {
                Some(outbox) => {
                    outbox.lock().unwrap().push_back(data);
                    0
                }
                None => -1,
            }
        },
    )?;

    // ── worker_message_read (worker side) ─────────────────────────────────
    // api_worker_message_read(out_ptr: u32, out_cap: u32) -> u32
    //   Copies the current inbound message into guest memory during on_message.
    //   Returns the number of bytes written (0 if no message is active).
    linker.func_wrap(
        "oxide",
        "api_worker_message_read",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let msg_arc = caller.data().worker_current_msg.clone();
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let guard = msg_arc.lock().unwrap();
            let bytes = match guard.as_ref() {
                Some(b) => b,
                None => return 0,
            };
            let to_write = if bytes.len() > out_cap as usize {
                &bytes[..out_cap as usize]
            } else {
                &bytes[..]
            };
            if write_guest_bytes(&mem, &mut caller, out_ptr, to_write).is_err() {
                return 0;
            }
            to_write.len() as u32
        },
    )?;

    Ok(())
}
