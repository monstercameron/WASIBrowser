//! Window shell + in-window console.
//!
//! The console is host-owned DOM inside the Blitz document — debugging UI
//! rides the same Stylo/Vello pipeline as everything else, no second toolkit.
//! Guest stdout/stderr arrive as [`ConsoleMsg`] payloads on
//! `BlitzShellEvent::Embedder` and get appended as console lines.

use std::collections::VecDeque;
use std::sync::Arc;

use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{
    DocGuard, DocGuardMut, Document, DocumentConfig, EventDriver, EventHandler, NoopEventHandler,
};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_shell::{BlitzApplication, BlitzShellEvent, WindowConfig};
use blitz_traits::events::{DomEvent, DomEventData, EventState, UiEvent};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Which stream a console chunk came from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Stdout,
    Stderr,
    Host,
}

/// Raw stream bytes, shipped to the UI thread as a
/// `BlitzShellEvent::Embedder` payload.
pub struct ConsoleMsg {
    pub source: Source,
    pub bytes: Vec<u8>,
}

const MAX_LINES: usize = 500;

/// Send a host-stream line to the in-window console (and system log).
pub fn host_console_line(proxy: &blitz_shell::BlitzShellProxy, line: &str) {
    crate::logger::log("host", line);
    proxy.send_event(BlitzShellEvent::Embedder(Arc::new(ConsoleMsg {
        source: Source::Host,
        bytes: format!("{line}\n").into_bytes(),
    })));
}

const SHELL_HTML: &str = r#"<!DOCTYPE html>
<html><head><style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  html, body { height: 100%; }
  body { display: flex; flex-direction: column; background: #1e1f22; color: #e8e8e8;
         font-family: 'Segoe UI', sans-serif; }
  #app { flex: 1 1 auto; padding: 24px; overflow-y: auto; }
  #app h1 { font-size: 22px; margin-bottom: 8px; }
  #app p { color: #9a9fa6; font-size: 14px; margin-bottom: 4px; }
  #mount { margin-top: 20px; }
  #mount button { padding: 8px 18px; background: #2f6feb; color: #fff; border: 1px solid #4a86f5;
                  border-radius: 6px; font-size: 14px; cursor: pointer; }
  #mount button:hover { background: #4a86f5; }
  #console { flex: 0 0 45%; display: flex; flex-direction: column;
             border-top: 1px solid #3a3d41; background: #161719; }
  #console-title { padding: 6px 12px; font-size: 12px; color: #9a9fa6;
                   background: #222427; border-bottom: 1px solid #3a3d41;
                   font-family: Consolas, monospace; }
  #console-log { flex: 1 1 auto; overflow-y: auto; display: flex;
                 flex-direction: column-reverse; padding: 8px 12px;
                 font-family: Consolas, 'Cascadia Mono', monospace;
                 font-size: 13px; line-height: 1.5; }
  .ln { white-space: pre-wrap; }
  .out { color: #d7d9dd; }
  .err { color: #ff8a80; }
  .host { color: #82aaff; }
</style></head>
<body>
  <main id="app">
    <h1>GoWebBrowser</h1>
    <p>Rust renderer shell — Blitz + wasmtime, zero JavaScript.</p>
    <p>Guest module: {{GUEST}}. Its stdout/stderr stream into the console below.</p>
    <div id="mount"></div>
  </main>
  <footer id="console">
    <div id="console-title">console — {{GUEST}}</div>
    <div id="console-log"></div>
  </footer>
</body></html>
"#;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The renderer's document: host-owned shell chrome + console, with the guest
/// app region to be wired to GDOM batches in the next milestone.
pub struct GwbDocument {
    doc: HtmlDocument,
    log_node: usize,
    lines: VecDeque<(Source, String)>,
    partial_out: String,
    partial_err: String,
    dirty: bool,
    /// GWB guest runtime + id maps (None in legacy console mode).
    guest: Option<crate::abi::GuestRuntime>,
    maps: crate::abi::NodeMaps,
}

impl GwbDocument {
    pub fn new(guest_name: &str) -> Self {
        let html = SHELL_HTML.replace("{{GUEST}}", &escape_html(guest_name));
        let config = DocumentConfig {
            // set_inner_html needs a parser provider at mutation time.
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            ..Default::default()
        };
        let doc = HtmlDocument::from_html(&html, config);
        let log_node = doc
            .query_selector("#console-log")
            .expect("selector parses")
            .expect("#console-log exists in shell HTML");
        let mount_node = doc
            .query_selector("#mount")
            .expect("selector parses")
            .expect("#mount exists in shell HTML");
        let mut maps = crate::abi::NodeMaps::default();
        // Guest id 1 = the mount root (spec: the only host node a guest can address).
        maps.fwd.insert(1, mount_node);
        maps.rev.insert(mount_node, 1);
        Self {
            doc,
            log_node,
            lines: VecDeque::new(),
            partial_out: String::new(),
            partial_err: String::new(),
            dirty: false,
            guest: None,
            maps,
        }
    }

    /// Attach a started GWB guest: apply its initial batches immediately.
    pub fn attach_guest(&mut self, rt: crate::abi::GuestRuntime) {
        let shared = rt.shared.clone();
        let applied = crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared);
        crate::logger::log("abi", &format!("initial mount: {applied} ops applied"));
        self.guest = Some(rt);
        self.dirty = true;
    }

    pub fn push_chunk(&mut self, source: Source, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes).into_owned();
        match source {
            Source::Host => {
                for line in text.lines() {
                    self.push_line(Source::Host, line.to_string());
                }
            }
            Source::Stdout | Source::Stderr => {
                let mut buf = std::mem::take(match source {
                    Source::Stdout => &mut self.partial_out,
                    _ => &mut self.partial_err,
                });
                for ch in text.chars() {
                    if ch == '\n' {
                        let line = std::mem::take(&mut buf);
                        self.push_line(source, line);
                    } else {
                        buf.push(ch);
                    }
                }
                *(match source {
                    Source::Stdout => &mut self.partial_out,
                    _ => &mut self.partial_err,
                }) = buf;
            }
        }
        self.rebuild_log();
    }

    fn push_line(&mut self, source: Source, line: String) {
        self.lines.push_back((source, line));
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    fn rebuild_log(&mut self) {
        // Newest-first in DOM: #console-log is column-reverse, so the first
        // child paints at the visual bottom and the log stays pinned to the
        // newest line without any scroll scripting.
        let mut html = String::with_capacity(4096);
        let partials = [
            (Source::Stdout, self.partial_out.as_str()),
            (Source::Stderr, self.partial_err.as_str()),
        ];
        let entries = partials
            .into_iter()
            .filter(|(_, l)| !l.is_empty())
            .chain(self.lines.iter().rev().map(|(s, l)| (*s, l.as_str())));
        for (source, line) in entries {
            html.push_str("<div class=\"ln ");
            html.push_str(match source {
                Source::Stdout => "out",
                Source::Stderr => "err",
                Source::Host => "host",
            });
            html.push_str("\">");
            html.push_str(&escape_html(line));
            html.push_str("</div>");
        }
        let mut mutr = self.doc.mutate();
        mutr.set_inner_html(self.log_node, &html);
        drop(mutr);
        self.dirty = true;
    }
}

impl Document for GwbDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::Ref(&self.doc)
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::Ref(&mut self.doc)
    }

    fn poll(&mut self, _task_context: Option<std::task::Context<'_>>) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        if let Some(rt) = self.guest.as_mut() {
            let handler = GwbEventHandler { rt, maps: &mut self.maps, mutated: false };
            let mut driver = EventDriver::new(&mut self.doc, handler);
            driver.handle_ui_event(event);
            self.dirty = true;
        } else {
            let mut driver = EventDriver::new(&mut self.doc, NoopEventHandler);
            driver.handle_ui_event(event);
        }
    }
}

/// Routes hit-tested DOM events to the GWB guest and applies the batches the
/// guest submits in response — the full interactive loop, inside one dispatch.
struct GwbEventHandler<'a> {
    rt: &'a mut crate::abi::GuestRuntime,
    maps: &'a mut crate::abi::NodeMaps,
    mutated: bool,
}

impl EventHandler for GwbEventHandler<'_> {
    fn handle_event(
        &mut self,
        chain: &[usize],
        event: &mut DomEvent,
        doc: &mut dyn Document,
        _event_state: &mut EventState,
    ) {
        // v0: clicks only.
        let (kind, x, y) = match &event.data {
            DomEventData::Click(p) => (crate::abi::ev::CLICK, p.page_x(), p.page_y()),
            _ => return,
        };

        // Bubble host-side: nearest subscribed ancestor in the chain wins.
        let listener = chain.iter().find_map(|nid| {
            self.maps
                .rev
                .get(nid)
                .copied()
                .filter(|gid| self.maps.listeners.contains(&(*gid, kind)))
        });
        let Some(listener) = listener else { return };
        let target = self.maps.rev.get(&event.target).copied().unwrap_or(listener);

        crate::logger::log(
            "event",
            &format!("click -> guest (target={target} listener={listener} x={x:.0} y={y:.0})"),
        );
        match self.rt.deliver_click(target, listener, x, y) {
            Ok(_flags) => {
                let shared = self.rt.shared.clone();
                let mut guard = doc.inner_mut();
                let applied = crate::abi::apply_batches(&mut guard, self.maps, &shared);
                if applied > 0 {
                    self.mutated = true;
                }
            }
            Err(e) => crate::logger::log("crash", &format!("guest event handler failed: {e:#}")),
        }
    }
}

/// Application wrapper: delegates to BlitzApplication, intercepts
/// `Embedder(ConsoleMsg)` events, and buffers any that arrive before the
/// window surface exists.
pub struct GwbApplication {
    inner: BlitzApplication<VelloWindowRenderer>,
    pending: Vec<Arc<ConsoleMsg>>,
}

impl GwbApplication {
    pub fn new(inner: BlitzApplication<VelloWindowRenderer>) -> Self {
        Self {
            inner,
            pending: Vec::new(),
        }
    }

    pub fn add_window(&mut self, window: WindowConfig<VelloWindowRenderer>) {
        self.inner.add_window(window);
    }

    fn deliver(&mut self, msg: &ConsoleMsg) {
        for window in self.inner.windows.values_mut() {
            let doc = window.downcast_doc_mut::<GwbDocument>();
            doc.push_chunk(msg.source, &msg.bytes);
            window.poll();
            window.request_redraw();
        }
    }

    fn flush_pending(&mut self) {
        if self.inner.windows.is_empty() {
            return;
        }
        for msg in std::mem::take(&mut self.pending) {
            self.deliver(&msg);
        }
    }
}

impl ApplicationHandler for GwbApplication {
    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.can_create_surfaces(event_loop);
        crate::logger::log(
            "ui",
            &format!("window surface(s) created ({} window)", self.inner.windows.len()),
        );
        self.flush_pending();
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.destroy_surfaces(event_loop);
    }

    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            crate::logger::log("ui", &format!("window event: {event:?}"));
        }
        self.inner.window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.inner.event_queue.try_recv() {
            match event {
                BlitzShellEvent::Embedder(payload) => {
                    match payload.downcast::<ConsoleMsg>() {
                        Ok(msg) => {
                            if self.inner.windows.is_empty() {
                                self.pending.push(msg);
                            } else {
                                self.flush_pending();
                                self.deliver(&msg);
                            }
                        }
                        Err(_other) => { /* not ours; ignore */ }
                    }
                }
                event => self.inner.handle_blitz_shell_event(event_loop, event),
            }
        }
    }
}
