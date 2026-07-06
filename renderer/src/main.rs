//! GoWebBrowser renderer — Rust host process.
//!
//! Current scope (pre-API-design checkpoint): load a Go wasip1 module in
//! wasmtime and run it with stdio inherited, proving the guest→host console
//! path. The Blitz document/GDOM integration lands behind this seam next.

use anyhow::Result;
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::WasiCtxBuilder;

fn main() -> Result<()> {
    let wasm_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hello.wasm".to_string());

    let engine = Engine::default();
    let module = Module::from_file(&engine, &wasm_path)?;

    // WASI preview1 with stdio only — no filesystem, no network, no clocks
    // beyond defaults. The guest gets exactly what we grant and nothing else.
    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
    wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |cx| cx)?;

    let wasi = WasiCtxBuilder::new().inherit_stdio().build_p1();
    let mut store = Store::new(&engine, wasi);

    let instance = linker.instantiate(&mut store, &module)?;

    // Go's wasip1 command mode exports _start. The Go runtime always ends by
    // calling proc_exit, which wasmtime surfaces as an I32Exit error — status 0
    // is success, not a failure.
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    match start.call(&mut store, ()) {
        Ok(()) => Ok(()),
        Err(e) => match e.downcast::<wasmtime_wasi::I32Exit>() {
            Ok(wasmtime_wasi::I32Exit(0)) => Ok(()),
            Ok(wasmtime_wasi::I32Exit(n)) => std::process::exit(n),
            Err(e) => Err(e.into()),
        },
    }
}
