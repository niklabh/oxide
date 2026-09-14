//! WebAssembly engine configuration for Oxide.
//!
//! This module configures [Wasmtime](https://wasmtime.dev/) for running guest modules in a
//! sandboxed environment: bounded linear memory, instruction fuel metering, and a
//! [`SandboxPolicy`] that gates host capabilities (filesystem, environment variables, network
//! sockets)—all denied unless explicitly enabled.
//!
//! Default [`SandboxPolicy`] limits: **256 MiB** linear memory (4096 × 64 KiB pages) and **~500M**
//! Wasm instructions of fuel per [`Store`] before the guest is halted.
//!
//! [`WasmEngine`] owns a shared [`Engine`] plus policy and is the main entry point for creating
//! stores, bounded memory, and compiled modules. [`ModuleLoader`] is a lighter bundle of engine
//! plus limits for scenarios such as loading child or dynamically linked modules.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use wasmtime::*;

const MAX_MEMORY_PAGES: u32 = 4096; // 4096 * 64KB = 256MB
const FUEL_LIMIT: u64 = 500_000_000; // ~500M instructions before forced halt
/// Bump when [`Config`] changes in a way that would invalidate cached artifacts.
const AOT_CACHE_FORMAT: &str = "v1";

/// Policy describing what resources a Wasm guest may use and the hard limits applied at runtime.
///
/// Limits (`max_memory_pages`, `fuel_limit`) are enforced when building stores and memory via
/// [`WasmEngine`]. Capability flags (`allow_*`) express intent for host integrations; by default
/// all are `false` so filesystem, environment, and network access are denied unless the embedding
/// layer opts in.
#[allow(dead_code)]
#[derive(Clone)]
pub struct SandboxPolicy {
    /// Maximum number of 64 KiB Wasm memory pages the guest may grow to (default: 4096 → 256 MiB).
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
    /// Returns the default policy: 4096 memory pages (256 MiB cap), ~500M instruction fuel, all
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

    /// Compiles raw Wasm bytes into a [`Module`], reusing a disk AOT cache when possible.
    pub fn compile_module(&self, wasm_bytes: &[u8]) -> Result<Module> {
        compile_cached(&self.engine, wasm_bytes)
    }
}

/// Compiles `wasm_bytes` with `engine`, loading a serialized artifact from the
/// on-disk AOT cache on subsequent visits.
///
/// Set `OXIDE_AOT_CACHE=off` to disable the cache, or `OXIDE_AOT_CACHE=/path`
/// to store artifacts under a custom root. Stale or corrupt entries are
/// discarded and the module is compiled from source.
pub fn compile_cached(engine: &Engine, wasm_bytes: &[u8]) -> Result<Module> {
    compile_cached_in(engine, wasm_bytes, cache_root().as_deref())
}

fn compile_cached_in(
    engine: &Engine,
    wasm_bytes: &[u8],
    cache_dir: Option<&Path>,
) -> Result<Module> {
    let hash = module_hash(wasm_bytes);
    if let Some(dir) = cache_dir {
        if let Some(module) = load_cached(engine, dir, &hash) {
            tracing::debug!("AOT cache hit {}", hash);
            return Ok(module);
        }
    }

    let module = Module::new(engine, wasm_bytes).context("failed to compile wasm module")?;
    if let Some(dir) = cache_dir {
        if let Err(e) = store_cached(dir, &hash, &module) {
            tracing::debug!("AOT cache store failed: {e}");
        }
    }
    Ok(module)
}

fn cache_root() -> Option<PathBuf> {
    match std::env::var("OXIDE_AOT_CACHE") {
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("off") => return None,
        Ok(v) if !v.is_empty() => return Some(PathBuf::from(v).join(AOT_CACHE_FORMAT)),
        _ => {}
    }
    Some(
        dirs::cache_dir()?
            .join("oxide")
            .join("aot")
            .join(AOT_CACHE_FORMAT),
    )
}

fn module_hash(wasm_bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(wasm_bytes))
}

fn cache_path(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!("{hash}.cwasm"))
}

fn load_cached(engine: &Engine, dir: &Path, hash: &str) -> Option<Module> {
    let path = cache_path(dir, hash);
    if !path.is_file() {
        return None;
    }
    // SAFETY: `path` is only created by `store_cached` via `Module::serialize`.
    // A corrupt or cross-version file fails deserialize and we fall back.
    match unsafe { Module::deserialize_file(engine, &path) } {
        Ok(module) => Some(module),
        Err(e) => {
            tracing::debug!("AOT cache reject {}: {e}", path.display());
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

fn store_cached(dir: &Path, hash: &str, module: &Module) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("create AOT cache dir {}", dir.display()))?;
    let dest = cache_path(dir, hash);
    if dest.exists() {
        return Ok(());
    }
    let bytes = module.serialize().context("serialize compiled module")?;
    let tmp = dir.join(format!("{hash}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("rename into {}", dest.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_wasm() -> Vec<u8> {
        wat::parse_str("(module)").expect("empty module")
    }

    #[test]
    fn default_policy_limits() {
        let policy = SandboxPolicy::default();
        assert_eq!(policy.max_memory_pages, 4096);
        assert_eq!(policy.fuel_limit, 500_000_000);
        assert!(!policy.allow_filesystem);
        assert!(!policy.allow_env_vars);
        assert!(!policy.allow_network_sockets);
    }

    #[test]
    fn engine_creates_with_default_policy() {
        WasmEngine::new(SandboxPolicy::default()).expect("engine");
    }

    #[test]
    fn compile_minimal_module() {
        let engine = WasmEngine::new(SandboxPolicy::default()).unwrap();
        compile_cached_in(engine.engine(), &empty_wasm(), None).expect("compile");
    }

    #[test]
    fn compile_invalid_wasm_errors() {
        let engine = WasmEngine::new(SandboxPolicy::default()).unwrap();
        assert!(compile_cached_in(engine.engine(), b"not a wasm module", None).is_err());
    }

    #[test]
    fn fuel_exhaustion_traps_infinite_loop() {
        let policy = SandboxPolicy {
            fuel_limit: 10_000,
            ..SandboxPolicy::default()
        };
        let engine = WasmEngine::new(policy).unwrap();
        let wasm = wat::parse_str(
            r#"
            (module
              (func (export "run")
                (loop $l (br $l))))
            "#,
        )
        .unwrap();
        let module = compile_cached_in(engine.engine(), &wasm, None).unwrap();
        let mut store = engine.create_store(()).unwrap();
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let run = instance
            .get_typed_func::<(), ()>(&mut store, "run")
            .unwrap();
        let err = run.call(&mut store, ()).unwrap_err();
        let msg = format!("{err:#}").to_lowercase();
        assert!(msg.contains("fuel"), "expected a fuel trap, got: {err:#}");
    }

    #[test]
    fn memory_rejects_grow_past_policy_ceiling() {
        let policy = SandboxPolicy {
            max_memory_pages: 4,
            ..SandboxPolicy::default()
        };
        let engine = WasmEngine::new(policy).unwrap();
        let mut store = engine.create_store(()).unwrap();
        let mem = engine.create_bounded_memory(&mut store).unwrap();
        assert_eq!(mem.ty(&store).minimum(), 1);
        assert_eq!(mem.ty(&store).maximum(), Some(4));
        assert!(mem.grow(&mut store, 8).is_err());
    }

    #[test]
    fn aot_cache_writes_and_reloads() {
        let engine = WasmEngine::new(SandboxPolicy::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let wasm = empty_wasm();
        compile_cached_in(engine.engine(), &wasm, Some(dir.path())).unwrap();
        let path = cache_path(dir.path(), &module_hash(&wasm));
        assert!(
            path.is_file(),
            "expected cached artifact at {}",
            path.display()
        );
        compile_cached_in(engine.engine(), &wasm, Some(dir.path())).expect("cache hit compile");
    }

    #[test]
    fn aot_cache_corrupt_entry_falls_back() {
        let engine = WasmEngine::new(SandboxPolicy::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let wasm = empty_wasm();
        let path = cache_path(dir.path(), &module_hash(&wasm));
        std::fs::write(&path, b"not a compiled module").unwrap();
        compile_cached_in(engine.engine(), &wasm, Some(dir.path())).expect("fallback compile");
        let cached = std::fs::read(&path).unwrap();
        assert_ne!(cached, b"not a compiled module");
    }
}
