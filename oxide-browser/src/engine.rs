use anyhow::{Context, Result};
use wasmtime::*;

const MAX_MEMORY_PAGES: u32 = 256; // 256 * 64KB = 16MB
const FUEL_LIMIT: u64 = 500_000_000; // ~500M instructions before forced halt

pub struct SandboxPolicy {
    pub max_memory_pages: u32,
    pub fuel_limit: u64,
    pub allow_filesystem: bool,
    pub allow_env_vars: bool,
    pub allow_network_sockets: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            max_memory_pages: MAX_MEMORY_PAGES,
            fuel_limit: FUEL_LIMIT,
            allow_filesystem: false,
            allow_env_vars: false,
            allow_network_sockets: false,
        }
    }
}

pub struct WasmEngine {
    engine: Engine,
    policy: SandboxPolicy,
}

impl WasmEngine {
    pub fn new(policy: SandboxPolicy) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.cranelift_opt_level(OptLevel::Speed);

        let engine = Engine::new(&config).context("failed to create wasmtime engine")?;
        Ok(Self { engine, policy })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    pub fn create_store<T>(&self, data: T) -> Result<Store<T>> {
        let mut store = Store::new(&self.engine, data);
        store
            .set_fuel(self.policy.fuel_limit)
            .context("failed to set fuel limit")?;
        Ok(store)
    }

    pub fn create_bounded_memory(&self, store: &mut Store<impl Send>) -> Result<Memory> {
        let mem_type = MemoryType::new(1, Some(self.policy.max_memory_pages));
        Memory::new(store, mem_type).context("failed to create bounded linear memory")
    }

    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<Module> {
        Module::new(&self.engine, wasm_bytes).context("failed to compile wasm module")
    }
}
