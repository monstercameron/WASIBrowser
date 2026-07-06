//! wasmtime guest hosting: runs the wasip1 module on its own thread with
//! stdout/stderr piped into the in-window console via the shell proxy.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use blitz_shell::{BlitzShellEvent, BlitzShellProxy};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::ui::{ConsoleMsg, Source};

/// A guest output stream that forwards every write to the UI thread.
#[derive(Clone)]
struct ConsolePipe {
    source: Source,
    proxy: BlitzShellProxy,
}

impl ConsolePipe {
    fn send(&self, bytes: &[u8]) {
        // Mirror every chunk to the system log at emission time, so output
        // survives even if the UI never gets to display it.
        let tag = match self.source {
            Source::Stdout => "guest:out",
            Source::Stderr => "guest:err",
            Source::Host => "host",
        };
        crate::logger::log(tag, String::from_utf8_lossy(bytes).trim_end_matches('\n'));

        self.proxy.send_event(BlitzShellEvent::Embedder(Arc::new(ConsoleMsg {
            source: self.source,
            bytes: bytes.to_vec(),
        })));
    }
}

impl IsTerminal for ConsolePipe {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for ConsolePipe {
    fn async_stream(&self) -> Box<dyn tokio::io::AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }
}

impl tokio::io::AsyncWrite for ConsolePipe {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.send(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Run the guest on a background thread; host status lines go to the console
/// on the Host stream.
pub fn spawn_guest(wasm_path: String, proxy: BlitzShellProxy) {
    std::thread::spawn(move || {
        let host = ConsolePipe {
            source: Source::Host,
            proxy: proxy.clone(),
        };
        let host_line = |s: String| host.send(format!("{s}\n").as_bytes());

        host_line(format!("[host] loading {wasm_path}"));
        let started = std::time::Instant::now();
        match run_guest(&wasm_path, &proxy) {
            Ok(status) => host_line(format!(
                "[host] guest exited with status {status} after {:.1?}",
                started.elapsed()
            )),
            Err(e) => host_line(format!("[host] guest failed: {e:#}")),
        }
    });
}

fn run_guest(path: &str, proxy: &BlitzShellProxy) -> Result<i32> {
    let engine = Engine::default();
    let compile_started = std::time::Instant::now();
    let module = Module::from_file(&engine, path)?;
    crate::logger::log(
        "guest",
        &format!("module compiled from {path} in {:.1?}", compile_started.elapsed()),
    );

    let mut linker: Linker<WasiP1Ctx> = Linker::new(&engine);
    wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |cx| cx)?;

    // stdout/stderr only — no filesystem, no network, no env.
    let wasi = WasiCtxBuilder::new()
        .stdout(ConsolePipe {
            source: Source::Stdout,
            proxy: proxy.clone(),
        })
        .stderr(ConsolePipe {
            source: Source::Stderr,
            proxy: proxy.clone(),
        })
        .build_p1();
    let mut store = Store::new(&engine, wasi);

    let instance = linker.instantiate(&mut store, &module)?;
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    match start.call(&mut store, ()) {
        Ok(()) => Ok(0),
        Err(e) => match e.downcast::<wasmtime_wasi::I32Exit>() {
            Ok(wasmtime_wasi::I32Exit(n)) => Ok(n),
            Err(e) => Err(e.into()),
        },
    }
}
