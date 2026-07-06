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

use anyhow::{Context as _, Result};
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

    // Resolve the target into a bundle + service registry (docs/04-WEB-RPC.md §4).
    let resolved = manifest::resolve(&target, &manifest_root)
        .with_context(|| format!("resolving {target}"))?;
    let wasm_path = resolved.bundle_path.clone();
    if resolved.dev_unverified {
        logger::log("rpc", &format!("dev: loading unverified bundle '{wasm_path}' (no b3: check)"));
    }
    logger::log(
        "sys",
        &format!(
            "navigating to '{target}' -> bundle={wasm_path} title=\"{}\" crash_test={crash_test} cwd={}",
            resolved.title,
            std::env::current_dir()?.display()
        ),
    );

    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());

    let mut doc = GwbDocument::new(&wasm_path);

    // GWB guests (export gwb_abi_version) run in-process on the UI thread;
    // anything else falls back to legacy console mode (_start on a thread).
    let mut legacy = false;
    match abi::try_load(&wasm_path, proxy.clone(), resolved.services) {
        Ok(Some(mut rt)) => {
            rt.start(1100.0, 800.0, 1.0, 1)
                .context("gwb_start failed")?;
            doc.attach_guest(rt, 1100.0, 800.0, 1.0);
        }
        Ok(None) => legacy = true,
        Err(e) => {
            logger::log("crash", &format!("gwb guest load failed: {e:#}"));
            ui::host_console_line(&proxy, &format!("[host] guest load FAILED: {e:#}"));
        }
    }

    let win_title = format!("{} — WASIBrowser", resolved.title);
    let attrs = WindowAttributes::default()
        .with_title(win_title)
        .with_surface_size(LogicalSize::new(1100.0, 800.0));
    let window = WindowConfig::with_attributes(Box::new(doc) as _, VelloWindowRenderer::new(), attrs);

    let mut app = GwbApplication::new(BlitzApplication::new(proxy.clone(), receiver));
    app.add_window(window);

    ui::host_console_line(&proxy, &format!("[host] system log: {}", log_path.display()));
    if legacy {
        spawn_guest(wasm_path, proxy.clone());
    }
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
