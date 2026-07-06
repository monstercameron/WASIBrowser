//! GoWebBrowser renderer — Rust host process.
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

mod guest;
mod logger;
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

    let arg = std::env::args().nth(1);
    let crash_test = arg.as_deref() == Some("--crash-test");
    let wasm_path = if crash_test {
        "hello.wasm".to_string()
    } else {
        arg.unwrap_or_else(|| "hello.wasm".to_string())
    };
    logger::log(
        "sys",
        &format!(
            "starting; guest={wasm_path} crash_test={crash_test} cwd={}",
            std::env::current_dir()?.display()
        ),
    );

    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());

    let doc = GwbDocument::new(&wasm_path);
    let attrs = WindowAttributes::default()
        .with_title("GoWebBrowser")
        .with_surface_size(LogicalSize::new(1100.0, 800.0));
    let window = WindowConfig::with_attributes(Box::new(doc) as _, VelloWindowRenderer::new(), attrs);

    let mut app = GwbApplication::new(BlitzApplication::new(proxy.clone(), receiver));
    app.add_window(window);

    ui::host_console_line(&proxy, &format!("[host] system log: {}", log_path.display()));
    spawn_guest(wasm_path, proxy.clone());

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
