use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use wasmtime::*;

use crate::capabilities::{register_host_functions, HostState};
use crate::engine::{SandboxPolicy, WasmEngine};

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
        let wasm_engine = WasmEngine::new(policy)?;

        Ok(Self {
            wasm_engine,
            status: Arc::new(Mutex::new(PageStatus::Idle)),
            host_state: HostState::default(),
        })
    }

    /// Recreate a BrowserHost sharing existing state (used by background worker threads).
    pub fn recreate(host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let policy = SandboxPolicy::default();
        let wasm_engine = WasmEngine::new(policy).expect("failed to create engine");
        Self {
            wasm_engine,
            status,
            host_state,
        }
    }

    /// Fetch a .wasm binary from a URL, compile it, and run its `start_app` entry point.
    pub async fn fetch_and_run(&mut self, url: &str) -> Result<()> {
        *self.status.lock().unwrap() = PageStatus::Loading(url.to_string());
        self.host_state.canvas.lock().unwrap().commands.clear();
        self.host_state.console.lock().unwrap().clear();

        let wasm_bytes = fetch_wasm(url).await?;

        *self.status.lock().unwrap() = PageStatus::Running(url.to_string());

        self.run_module(&wasm_bytes)?;

        Ok(())
    }

    /// Load a .wasm binary from raw bytes (useful for local files).
    pub fn run_bytes(&mut self, wasm_bytes: &[u8]) -> Result<()> {
        self.host_state.canvas.lock().unwrap().commands.clear();
        self.host_state.console.lock().unwrap().clear();
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
        anyhow::bail!(
            "server returned HTTP {} for {}",
            response.status(),
            url
        );
    }

    let bytes = response
        .bytes()
        .await
        .context("failed to read response body")?;

    Ok(bytes.to_vec())
}
