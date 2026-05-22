// runtime.rs — WASMtime runtime for ZethraOS sandboxed apps
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tracing::info;
use wasmtime::*;

struct MemoryLimiter {
    max_memory: usize,
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> std::result::Result<bool, wasmtime::Error> {
        Ok(desired <= self.max_memory)
    }
    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> std::result::Result<bool, wasmtime::Error> {
        Ok(desired <= 1000)
    }
}

pub struct SandboxRuntime {
    engine: Engine,
}

impl SandboxRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    pub async fn load_and_run(&self, wasm_bytes: &[u8], fuel: u64) -> Result<()> {
        let limiter = MemoryLimiter {
            max_memory: 64 * 1024 * 1024,
        };
        let mut store = Store::new(&self.engine, limiter);
        store.limiter(|s| s);
        store.set_fuel(fuel)?;

        let module = Module::from_binary(&self.engine, wasm_bytes)?;
        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &module)?;

        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .or_else(|_| instance.get_typed_func::<(), ()>(&mut store, "main"))
            .map_err(|_| anyhow::anyhow!("No entry point found in WASM module"))?;

        info!("Starting sandboxed app execution");
        start.call(&mut store, ())?;

        let fuel_consumed = fuel - store.get_fuel()?;
        info!(fuel_consumed, "App execution finished");

        Ok(())
    }
}
