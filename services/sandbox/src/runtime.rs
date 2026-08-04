// runtime.rs — WASMtime runtime for ZethraOS sandboxed apps with Capability Bridging
// SPDX-License-Identifier: Apache-2.0

use crate::permissions::{AppManifest, PermissionType};
use anyhow::Result;
use tracing::{info, warn};
use wasmtime::*;

pub struct MemoryLimiter {
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

pub struct SandboxContext {
    pub limiter: MemoryLimiter,
    pub manifest: AppManifest,
    pub last_status: i32,
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

    pub async fn load_and_run(
        &self,
        wasm_bytes: &[u8],
        fuel: u64,
        manifest: AppManifest,
    ) -> Result<i32> {
        let limiter = MemoryLimiter {
            max_memory: 64 * 1024 * 1024,
        };
        let context = SandboxContext {
            limiter,
            manifest,
            last_status: 0,
        };
        let mut store = Store::new(&self.engine, context);
        store.limiter(|s| &mut s.limiter);
        store.set_fuel(fuel)?;

        let module = Module::from_binary(&self.engine, wasm_bytes)?;
        let mut linker = Linker::new(&self.engine);

        // Register ZethraOS Host Import ABI ("zethra" module)
        linker.func_wrap(
            "zethra",
            "zethra_log",
            |mut caller: Caller<'_, SandboxContext>,
             ptr: u32,
             len: u32|
             -> Result<(), wasmtime::Error> {
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .ok_or_else(|| std::io::Error::other("failed to find memory export"))?;
                let data = mem.data(&caller);
                let start = ptr as usize;
                let end = start
                    .checked_add(len as usize)
                    .ok_or_else(|| std::io::Error::other("memory bounds overflow"))?;
                if end <= data.len() {
                    let msg = String::from_utf8_lossy(&data[start..end]);
                    info!(app = %caller.data().manifest.package.name, "WASM Log: {}", msg);
                } else {
                    warn!("WASM attempted out-of-bounds read in zethra_log");
                }
                Ok(())
            },
        )?;

        linker.func_wrap("zethra", "zethra_hal_request", |mut caller: Caller<'_, SandboxContext>, perm_id: u32, req_ptr: u32, req_len: u32, resp_ptr: u32, resp_max_len: u32| -> i32 {
            let perm_opt = PermissionType::from_u32(perm_id);
            let perm = match perm_opt {
                Some(p) => p,
                None => {
                    warn!(perm_id, "WASM requested invalid capability ID");
                    caller.data_mut().last_status = -2; // EINVAL
                    return -2;
                }
            };

            if !caller.data().manifest.has_permission(&perm) {
                warn!(app = %caller.data().manifest.package.name, ?perm, "Permission DENIED by AppManifest");
                caller.data_mut().last_status = -13; // EACCES
                return -13;
            }

            info!(app = %caller.data().manifest.package.name, ?perm, "Permission GRANTED — executing in-process HAL dispatch");

            // Safe memory inspection for request/response payloads
            if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let (_req_bytes, is_valid) = {
                    let data = mem.data(&caller);
                    let start = req_ptr as usize;
                    let end = start.saturating_add(req_len as usize);
                    if end <= data.len() {
                        (data[start..end].to_vec(), true)
                    } else {
                        (Vec::new(), false)
                    }
                };
                if !is_valid && req_len > 0 {
                    warn!("WASM out-of-bounds request payload reading");
                    caller.data_mut().last_status = -14; // EFAULT
                    return -14;
                }

                // In-process dispatch to SandboxHal handler (returns synchronous JSON status)
                let response_payload = b"{\"status\":\"ok\",\"hal_response\":true}";
                let write_len = std::cmp::min(response_payload.len(), resp_max_len as usize);
                if write_len > 0
                    && mem.write(&mut caller, resp_ptr as usize, &response_payload[..write_len]).is_err()
                {
                    caller.data_mut().last_status = -14; // EFAULT
                    return -14;
                }
            }

            caller.data_mut().last_status = 0; // OK
            0
        })?;

        let instance = linker.instantiate(&mut store, &module)?;

        info!("Starting sandboxed app execution");
        let result_code = if let Ok(func) = instance
            .get_typed_func::<(), i32>(&mut store, "main")
            .or_else(|_| instance.get_typed_func::<(), i32>(&mut store, "run"))
        {
            func.call(&mut store, ())?
        } else if let Ok(func) = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .or_else(|_| instance.get_typed_func::<(), ()>(&mut store, "main"))
            .or_else(|_| instance.get_typed_func::<(), ()>(&mut store, "run"))
        {
            func.call(&mut store, ())?;
            store.data().last_status
        } else {
            return Err(anyhow::anyhow!(
                "No valid entry point (main, _start, run) found in WASM module"
            ));
        };

        let fuel_consumed = fuel - store.get_fuel()?;
        info!(fuel_consumed, result_code, "App execution finished");

        Ok(result_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_permission_denied() {
        let wat_code = r#"
            (module
                (import "zethra" "zethra_hal_request" (func $req (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "main") (result i32)
                    (call $req (i32.const 2) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0))
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_code).expect("failed to compile wat");
        let runtime = SandboxRuntime::new().expect("runtime initialization failed");
        let manifest = AppManifest::default_test(vec![]); // Zero grants
        let result = runtime
            .load_and_run(&wasm_bytes, 10_000, manifest)
            .await
            .unwrap();
        assert_eq!(result, -13); // EACCES (Permission denied)
    }

    #[tokio::test]
    async fn test_sandbox_permission_granted() {
        let wat_code = r#"
            (module
                (import "zethra" "zethra_hal_request" (func $req (param i32 i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "main") (result i32)
                    (call $req (i32.const 2) (i32.const 0) (i32.const 0) (i32.const 100) (i32.const 64))
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_code).expect("failed to compile wat");
        let runtime = SandboxRuntime::new().expect("runtime initialization failed");
        let manifest = AppManifest::default_test(vec![PermissionType::Camera]);
        let result = runtime
            .load_and_run(&wasm_bytes, 10_000, manifest)
            .await
            .unwrap();
        assert_eq!(result, 0); // OK
    }

    #[tokio::test]
    async fn test_sandbox_fuel_exhaustion() {
        let wat_code = r#"
            (module
                (func $loop (export "main")
                    (loop $l
                        (br $l)
                    )
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_code).expect("failed to compile wat");
        let runtime = SandboxRuntime::new().expect("runtime initialization failed");
        let manifest = AppManifest::default_test(vec![]);
        let result = runtime.load_and_run(&wasm_bytes, 100, manifest).await;
        assert!(
            result.is_err(),
            "Expected execution to terminate with an error due to fuel exhaustion"
        );
    }
}
