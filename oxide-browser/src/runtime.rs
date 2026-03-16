use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use wasmtime::*;

use crate::capabilities::{register_host_functions, HostState};
use crate::engine::{ModuleLoader, SandboxPolicy, WasmEngine};
use crate::url::OxideUrl;

#[derive(Clone, Debug, PartialEq)]
pub enum PageStatus {
    Idle,
    Loading(String),
    Running(String),
    Error(String),
}

pub struct BrowserHost {
    wasm_engine: WasmEngine,
    pub status: Arc<Mutex<PageStatus>>,
    pub host_state: HostState,
}

impl BrowserHost {
    pub fn new() -> Result<Self> {
        let policy = SandboxPolicy::default();
        let wasm_engine = WasmEngine::new(policy.clone())?;

        let loader = Arc::new(ModuleLoader {
            engine: wasm_engine.engine().clone(),
            max_memory_pages: policy.max_memory_pages,
            fuel_limit: policy.fuel_limit,
        });

        let kv_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("oxide")
            .join("kv_store.db");
        let kv_db = sled::open(&kv_path)
            .with_context(|| format!("failed to open KV store at {}", kv_path.display()))?;

        let host_state = HostState {
            module_loader: Some(loader),
            kv_db: Some(Arc::new(kv_db)),
            ..Default::default()
        };

        Ok(Self {
            wasm_engine,
            status: Arc::new(Mutex::new(PageStatus::Idle)),
            host_state,
        })
    }

    /// Recreate a BrowserHost sharing existing state (used by background worker threads).
    pub fn recreate(mut host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let policy = SandboxPolicy::default();
        let wasm_engine = WasmEngine::new(policy.clone()).expect("failed to create engine");

        if host_state.module_loader.is_none() {
            host_state.module_loader = Some(Arc::new(ModuleLoader {
                engine: wasm_engine.engine().clone(),
                max_memory_pages: policy.max_memory_pages,
                fuel_limit: policy.fuel_limit,
            }));
        }

        Self {
            wasm_engine,
            status,
            host_state,
        }
    }

    /// Fetch a .wasm binary from a URL, compile it, and run its `start_app`
    /// entry point.  Supports http(s) and file:// URLs via WHATWG parsing.
    pub async fn fetch_and_run(&mut self, url: &str) -> Result<()> {
        *self.status.lock().unwrap() = PageStatus::Loading(url.to_string());
        self.host_state.canvas.lock().unwrap().commands.clear();
        self.host_state.console.lock().unwrap().clear();
        self.host_state.hyperlinks.lock().unwrap().clear();
        *self.host_state.current_url.lock().unwrap() = url.to_string();

        let parsed = OxideUrl::parse(url).map_err(|e| anyhow::anyhow!("{e}"))?;

        let wasm_bytes = if parsed.is_fetchable() {
            fetch_wasm(parsed.as_str()).await?
        } else if parsed.is_local_file() {
            let path = parsed
                .to_file_path()
                .ok_or_else(|| anyhow::anyhow!("cannot convert file URL to path: {url}"))?;
            std::fs::read(&path)
                .with_context(|| format!("failed to read local file: {}", path.display()))?
        } else if parsed.is_internal() {
            anyhow::bail!("oxide:// internal pages are not yet implemented");
        } else {
            anyhow::bail!("unsupported URL scheme: {}", parsed.scheme());
        };

        *self.status.lock().unwrap() = PageStatus::Running(url.to_string());

        self.run_module(&wasm_bytes)?;

        Ok(())
    }

    /// Load a .wasm binary from raw bytes (useful for local files).
    pub fn run_bytes(&mut self, wasm_bytes: &[u8]) -> Result<()> {
        self.host_state.canvas.lock().unwrap().commands.clear();
        self.host_state.console.lock().unwrap().clear();
        self.host_state.hyperlinks.lock().unwrap().clear();
        *self.status.lock().unwrap() = PageStatus::Running("(local)".to_string());
        self.run_module(wasm_bytes)
    }

    fn run_module(&mut self, wasm_bytes: &[u8]) -> Result<()> {
        let module = self.wasm_engine.compile_module(wasm_bytes)?;

        let mut linker = Linker::new(self.wasm_engine.engine());
        register_host_functions(&mut linker)?;

        let mut host_state = self.host_state.clone();
        let mut store = self.wasm_engine.create_store(host_state.clone())?;

        let memory = self.wasm_engine.create_bounded_memory(&mut store)?;
        linker.define(&store, "oxide", "memory", memory)?;

        host_state.memory = Some(memory);
        *store.data_mut() = host_state;

        let instance = linker
            .instantiate(&mut store, &module)
            .context("failed to instantiate wasm module")?;

        // The guest module defines its own linear memory (memory 0) and
        // exports it as "memory".  String pointers from the guest refer to
        // THIS memory, not the oxide::memory we defined in the linker.
        if let Some(guest_mem) = instance.get_memory(&mut store, "memory") {
            store.data_mut().memory = Some(guest_mem);
        }

        let start_app = instance
            .get_typed_func::<(), ()>(&mut store, "start_app")
            .context("module must export `start_app` as extern \"C\" fn()")?;

        match start_app.call(&mut store, ()) {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = if e.to_string().contains("fuel") {
                    "Execution halted: fuel limit exceeded (possible infinite loop)".to_string()
                } else {
                    format!("Runtime error: {e}")
                };
                *self.status.lock().unwrap() = PageStatus::Error(msg.clone());
                Err(anyhow::anyhow!(msg))
            }
        }
    }
}

/// Maximum size of a `.wasm` module that can be fetched over the network.
const MAX_WASM_MODULE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

async fn fetch_wasm(url: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let response = client
        .get(url)
        .header("Accept", "application/wasm")
        .send()
        .await
        .context("network request failed")?;

    if !response.status().is_success() {
        anyhow::bail!("server returned HTTP {} for {}", response.status(), url);
    }

    // Reject responses with an obviously wrong Content-Type.
    if let Some(ct) = response.headers().get("content-type") {
        let ct_str = ct.to_str().unwrap_or("");
        if !ct_str.is_empty()
            && !ct_str.contains("application/wasm")
            && !ct_str.contains("application/octet-stream")
        {
            anyhow::bail!("unexpected Content-Type for .wasm module: {ct_str}");
        }
    }

    // Enforce size limit early via Content-Length when available.
    if let Some(len) = response.content_length() {
        anyhow::ensure!(
            len <= MAX_WASM_MODULE_SIZE,
            "module too large ({len} bytes, limit is {MAX_WASM_MODULE_SIZE})"
        );
    }

    let bytes = response
        .bytes()
        .await
        .context("failed to read response body")?;

    // Content-Length can be absent or spoofed, so check actual size too.
    anyhow::ensure!(
        (bytes.len() as u64) <= MAX_WASM_MODULE_SIZE,
        "module body exceeds size limit ({} bytes)",
        bytes.len()
    );

    Ok(bytes.to_vec())
}
