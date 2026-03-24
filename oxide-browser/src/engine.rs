//! WebAssembly engine configuration for Oxide.
//!
//! This module configures [Wasmtime](https://wasmtime.dev/) for running guest modules in a
//! sandboxed environment: bounded linear memory, instruction fuel metering, and a
//! [`SandboxPolicy`] that gates host capabilities (filesystem, environment variables, network
//! sockets)—all denied unless explicitly enabled.
//!
//! Default [`SandboxPolicy`] limits: **16 MiB** linear memory (256 × 64 KiB pages) and **~500M**
//! Wasm instructions of fuel per [`Store`] before the guest is halted.
//!
//! [`WasmEngine`] owns a shared [`Engine`] plus policy and is the main entry point for creating
//! stores, bounded memory, and compiled modules. [`ModuleLoader`] is a lighter bundle of engine
//! plus limits for scenarios such as loading child or dynamically linked modules.

use anyhow::{Context, Result};
use wasmtime::*;

const MAX_MEMORY_PAGES: u32 = 256; // 256 * 64KB = 16MB
const FUEL_LIMIT: u64 = 500_000_000; // ~500M instructions before forced halt

/// Policy describing what resources a Wasm guest may use and the hard limits applied at runtime.
///
/// Limits (`max_memory_pages`, `fuel_limit`) are enforced when building stores and memory via
/// [`WasmEngine`]. Capability flags (`allow_*`) express intent for host integrations; by default
/// all are `false` so filesystem, environment, and network access are denied unless the embedding
/// layer opts in.
#[allow(dead_code)]
#[derive(Clone)]
pub struct SandboxPolicy {
    /// Maximum number of 64 KiB Wasm memory pages the guest may grow to (default: 256 → 16 MiB).
    pub max_memory_pages: u32,
    /// Maximum Wasm “fuel” (instruction budget) for a single [`Store`] before execution stops.
    pub fuel_limit: u64,
    /// When `true`, the embedding may expose filesystem-backed host APIs to the guest.
    pub allow_filesystem: bool,
    /// When `true`, the embedding may expose environment variable access to the guest.
    pub allow_env_vars: bool,
    /// When `true`, the embedding may expose network socket APIs to the guest.
    pub allow_network_sockets: bool,
}

/// Minimal engine + limit bundle for loading additional Wasm modules (e.g. dynamic imports).
///
/// Holds a shared [`Engine`] alongside the same memory and fuel caps as [`SandboxPolicy`] so
/// child modules can be compiled consistently without carrying the full policy struct.
pub struct ModuleLoader {
    /// Wasmtime engine instance shared with the parent embedding.
    pub engine: Engine,
    /// Upper bound on guest linear memory, in 64 KiB pages.
    pub max_memory_pages: u32,
    /// Instruction budget (fuel) aligned with the parent sandbox.
    pub fuel_limit: u64,
}

impl Default for SandboxPolicy {
    /// Returns the default policy: 256 memory pages (16 MiB cap), ~500M instruction fuel, all
    /// `allow_*` flags `false`.
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

/// Sandbox-aware wrapper around a Wasmtime [`Engine`].
///
/// Configures the engine for fuel-metered execution and compiles/instantiates modules according
/// to the associated [`SandboxPolicy`].
pub struct WasmEngine {
    engine: Engine,
    policy: SandboxPolicy,
}

impl WasmEngine {
    /// Builds a [`WasmEngine`] with fuel metering enabled and Cranelift optimizations for speed.
    ///
    /// The returned engine is ready to compile modules; per-guest limits come from `policy` when
    /// calling [`create_store`](Self::create_store) and [`create_bounded_memory`](Self::create_bounded_memory).
    pub fn new(policy: SandboxPolicy) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.cranelift_opt_level(OptLevel::Speed);

        let engine = Engine::new(&config).context("failed to create wasmtime engine")?;
        Ok(Self { engine, policy })
    }

    /// Returns the underlying Wasmtime [`Engine`] for compilation and linking.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Returns the active [`SandboxPolicy`] (memory cap, fuel, and capability flags).
    #[allow(dead_code)]
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Creates a new [`Store`] with `data` as host state and sets fuel to `policy.fuel_limit`.
    pub fn create_store<T>(&self, data: T) -> Result<Store<T>> {
        let mut store = Store::new(&self.engine, data);
        store
            .set_fuel(self.policy.fuel_limit)
            .context("failed to set fuel limit")?;
        Ok(store)
    }

    /// Allocates a new linear [`Memory`] with minimum 1 page and maximum `policy.max_memory_pages`.
    pub fn create_bounded_memory(&self, store: &mut Store<impl Send>) -> Result<Memory> {
        let mem_type = MemoryType::new(1, Some(self.policy.max_memory_pages));
        Memory::new(store, mem_type).context("failed to create bounded linear memory")
    }

    /// Compiles raw Wasm bytes into a [`Module`] using this engine’s configuration.
    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<Module> {
        Module::new(&self.engine, wasm_bytes).context("failed to compile wasm module")
    }
}
