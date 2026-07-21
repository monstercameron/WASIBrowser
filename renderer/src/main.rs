//! WASIBrowser renderer — Rust host process.
//!
//! Blitz window (Stylo/Taffy/Vello) + wasmtime guest. The window carries a
//! host-owned devtools-style console; guest stdout/stderr stream into it.
//! Everything is mirrored to logs/system.log (live-tailable) and panics
//! produce logs/crash-*.log black boxes.
//!
//! Debug switches:
//!   renderer.exe [guest.wasm]     — run guest (default hello.wasm)
//!   renderer.exe --crash-test     — deliberately panic on a worker thread
//!                                   after 1s to exercise crash reporting

mod abi;
mod guest;
mod logger;
mod manifest;
mod script;
mod ui;

use anyhow::Result;
use anyrender_vello::VelloWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellProxy, WindowConfig, create_default_event_loop};
use winit::dpi::LogicalSize;
use winit::window::WindowAttributes;

use crate::guest::spawn_guest;
use crate::ui::{GwbApplication, GwbDocument};

fn main() -> Result<()> {
    let log_path = logger::init()?;
    logger::install_panic_hook();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let crash_test = args.iter().any(|a| a == "--crash-test");
    let script_path = args
        .iter()
        .position(|a| a == "--script")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let manifest_root = args
        .iter()
        .position(|a| a == "--manifest-root")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "manifests".to_string());
    // Navigation target: a `web://name` address or a `.wasm` path.
    let target = args
        .iter()
        .find(|a| !a.starts_with("--") && Some(a.as_str()) != script_path.as_deref()
            && Some(a.as_str()) != Some(manifest_root.as_str()))
        .cloned()
        .unwrap_or_else(|| "hello.wasm".to_string());

    logger::log(
        "sys",
        &format!(
            "starting, initial target='{target}' crash_test={crash_test} cwd={}",
            std::env::current_dir()?.display()
        ),
    );

    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());

    // One process-wide wasmtime Engine, shared across every tab's guest —
    // Engines are designed to be cheaply shared across many Stores; creating
    // one per guest (the old per-navigate behavior) wasted JIT/codegen setup.
    let engine = wasmtime::Engine::default();

    let mut doc = GwbDocument::new(engine, proxy.clone(), manifest_root.clone());
    // navigate() resolves the manifest, loads the guest, and starts it, all
    // inside the tab created by GwbDocument::new. If the initial target
    // isn't a GWB guest, fall back to the legacy console-mode host (matches
    // the original single-guest behavior) — tabs opened later to a non-GWB
    // target are not supported (see navigate()'s Ok(None) log).
    if !doc.navigate_active(&target) {
        if let Ok(resolved) = manifest::resolve(&target, &manifest_root) {
            logger::log("sys", &format!("falling back to legacy console-mode for '{target}'"));
            spawn_guest(resolved.bundle_path, proxy.clone());
        }
    }

    let win_title = "WASIBrowser".to_string();
    let attrs = WindowAttributes::default()
        .with_title(win_title)
        .with_surface_size(LogicalSize::new(1100.0, 800.0));
    let window = WindowConfig::with_attributes(Box::new(doc) as _, VelloWindowRenderer::new(), attrs);

    let mut app = GwbApplication::new(BlitzApplication::new(proxy.clone(), receiver));
    app.add_window(window);

    ui::host_console_line(&proxy, &format!("[host] system log: {}", log_path.display()));
    if let Some(path) = script_path {
        script::run(path, proxy.clone());
    }

    if crash_test {
        std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(1));
                panic!("deliberate --crash-test panic");
            })?;
    }

    let result = event_loop.run_app(app);
    match &result {
        Ok(()) => logger::log("sys", "event loop exited"),
        Err(e) => logger::log("crash", &format!("event loop error: {e}")),
    }
    logger::mark_clean_exit();
    result?;
    Ok(())
}
