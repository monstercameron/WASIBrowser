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
use blitz_traits::events::{BlitzImeEvent, DomEvent, EventState, UiEvent};
use keyboard_types::Modifiers;
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
    last_frame: Option<std::time::Instant>,
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
            last_frame: None,
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

    /// Record the winit window id so request_frame can wake the event loop.
    pub fn set_window_id(&mut self, id: WindowId) {
        if let Some(rt) = &self.guest {
            rt.shared.lock().unwrap().window_id = Some(id);
        }
    }

    /// Whether the guest has an undelivered request_frame.
    pub fn frame_pending(&self) -> bool {
        self.guest
            .as_ref()
            .is_some_and(|rt| rt.shared.lock().unwrap().frame_requested)
    }

    /// Deliver a window-level event (resize/theme) if the guest subscribed on
    /// the mount root (guest id 1).
    pub fn notify_window_event(&mut self, eo: crate::abi::EventOut) {
        let Some(rt) = self.guest.as_mut() else { return };
        if !self.maps.listeners.contains(&(1, eo.kind)) {
            return;
        }
        match rt.deliver_event(&eo, 0, 1, 1) {
            Ok(_) => {
                let shared = rt.shared.clone();
                if crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared) > 0 {
                    self.dirty = true;
                }
            }
            Err(e) => crate::logger::log("crash", &format!("window event delivery failed: {e:#}")),
        }
    }

    // ---- scripted-driver support ------------------------------------

    fn resolve(&self, selector: &str) -> Option<usize> {
        self.doc.query_selector(selector).ok().flatten()
    }

    /// Dispatch a synthetic click DomEvent at the selected element. Unlike a
    /// pointer sequence this is NOT hit-tested — it reaches the element even
    /// when scrolled out of view (deliberate: this is a test driver, and
    /// "scroll into view first" is the app's concern, not the assertion's).
    pub fn script_click(&mut self, selector: &str) -> bool {
        let Some(nid) = self.resolve(selector) else { return false };
        let Some(node) = self.doc.get_node(nid) else { return false };
        let data = node.synthetic_click_event(Modifiers::empty());
        let is_input = node
            .element_data()
            .is_some_and(|el| matches!(&*el.name.local, "input" | "textarea"));
        let event = DomEvent::new(nid, data);
        if self.guest.is_some() {
            {
                let rt = self.guest.as_mut().unwrap();
                let handler = GwbEventHandler { rt, maps: &mut self.maps, mutated: false };
                let mut driver = EventDriver::new(&mut self.doc, handler);
                driver.handle_dom_event(event);
            }
            let shared = self.guest.as_ref().unwrap().shared.clone();
            crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared);
            self.dirty = true;
            self.check_observations();
        } else {
            let mut driver = EventDriver::new(&mut self.doc, NoopEventHandler);
            driver.handle_dom_event(event);
        }
        // The pointer-down default (focus-on-click) didn't run; emulate it so
        // `click #input` + `type ...` composes naturally.
        if is_input {
            if let Some(nid) = self.resolve(selector) {
                self.doc.set_focus_to(nid);
            }
        }
        true
    }

    /// Commit text into the focused element (IME path — the real input pipeline).
    pub fn script_type(&mut self, text: &str) {
        self.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Commit(text.to_string())));
    }

    pub fn script_focus(&mut self, selector: &str) -> bool {
        let Some(nid) = self.resolve(selector) else { return false };
        self.doc.set_focus_to(nid);
        true
    }

    /// Serialize the guest mount subtree as indented pseudo-HTML.
    pub fn dump_dom(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::new();
        if let Some(&root) = self.maps.fwd.get(&1) {
            self.dump_node(root, 0, &mut out);
        }
        std::fs::write(path, out)
    }

    fn dump_node(&self, id: usize, depth: usize, out: &mut String) {
        let Some(node) = self.doc.get_node(id) else { return };
        let indent = "  ".repeat(depth);
        if let Some(el) = node.element_data() {
            out.push_str(&indent);
            out.push('<');
            out.push_str(&el.name.local);
            for attr in el.attrs.iter() {
                out.push(' ');
                out.push_str(&attr.name.local);
                out.push_str("=\"");
                out.push_str(&attr.value);
                out.push('"');
            }
            out.push_str(">\n");
            for &child in node.children.iter() {
                self.dump_node(child, depth + 1, out);
            }
        } else if let Some(text) = node.text_data() {
            let t = text.content.trim();
            if !t.is_empty() {
                out.push_str(&indent);
                out.push('"');
                out.push_str(t);
                out.push_str("\"\n");
            }
        } else {
            for &child in node.children.iter() {
                self.dump_node(child, depth + 1, out);
            }
        }
    }

    /// Emit observed_layout records for Observe'd nodes whose geometry changed.
    /// Returns true if the guest mutated the DOM in response.
    fn check_observations(&mut self) -> bool {
        let Some(rt) = self.guest.as_mut() else { return false };
        if self.maps.observers.is_empty() {
            return false;
        }
        // Collect changed rects first (read-only pass over the layout tree).
        let mut changes: Vec<(u32, [f32; 4])> = Vec::new();
        for (&gid, &what) in &self.maps.observers {
            if what & crate::abi::obs::LAYOUT == 0 {
                continue;
            }
            let Some(&nid) = self.maps.fwd.get(&gid) else { continue };
            let Some(node) = self.doc.get_node(nid) else { continue };
            let pos = node.absolute_position(0.0, 0.0);
            let size = node.final_layout.size;
            let rect = [pos.x, pos.y, size.width, size.height];
            if self.maps.last_rects.get(&gid) != Some(&rect) {
                changes.push((gid, rect));
            }
        }
        if changes.is_empty() {
            return false;
        }
        for (gid, rect) in &changes {
            self.maps.last_rects.insert(*gid, *rect);
            let eo = crate::abi::EventOut::layout_rect(*rect);
            if let Err(e) = rt.deliver_event(&eo, 0, *gid, *gid) {
                crate::logger::log("crash", &format!("observation delivery failed: {e:#}"));
            }
        }
        let shared = rt.shared.clone();
        let applied = crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared);
        if applied > 0 {
            self.dirty = true;
        }
        applied > 0
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
        let mut dirty = std::mem::take(&mut self.dirty);

        // Animation frames: deliver gwb_frame if the guest requested one.
        if let Some(rt) = self.guest.as_mut() {
            let wanted = std::mem::take(&mut rt.shared.lock().unwrap().frame_requested);
            if wanted {
                let now = std::time::Instant::now();
                let dt = self
                    .last_frame
                    .map(|t| now.duration_since(t).as_secs_f32() * 1000.0)
                    .unwrap_or(16.6);
                self.last_frame = Some(now);
                if let Err(e) = rt.deliver_frame(dt) {
                    crate::logger::log("crash", &format!("gwb_frame failed: {e:#}"));
                }
                let shared = rt.shared.clone();
                if crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared) > 0 {
                    dirty = true;
                }
            }
        }

        if self.check_observations() {
            dirty = true;
        }
        dirty
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        if self.guest.is_some() {
            {
                let rt = self.guest.as_mut().unwrap();
                let handler = GwbEventHandler { rt, maps: &mut self.maps, mutated: false };
                let mut driver = EventDriver::new(&mut self.doc, handler);
                driver.handle_ui_event(event);
            }
            // Post-dispatch: apply everything the guest submitted during the
            // dispatch, now that the driver no longer holds node chains.
            let shared = self.guest.as_ref().unwrap().shared.clone();
            crate::abi::apply_batches(&mut self.doc, &mut self.maps, &shared);
            self.dirty = true;
            self.check_observations();
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
        event_state: &mut EventState,
    ) {
        let Some(eo) = crate::abi::map_dom_event(&event.data) else { return };

        // Bubble host-side: nearest subscribed ancestor in the chain wins.
        let listener = chain.iter().find_map(|nid| {
            self.maps
                .rev
                .get(nid)
                .copied()
                .filter(|gid| self.maps.listeners.contains(&(*gid, eo.kind)))
        });
        let Some(listener) = listener else { return };
        let target = self.maps.rev.get(&event.target).copied().unwrap_or(listener);

        let preventable = event.cancelable;
        let record_flags = if preventable { 1u16 } else { 0 };
        if eo.kind != crate::abi::ev::POINTER_MOVE {
            crate::logger::log(
                "event",
                &format!("{} -> guest (target={target} listener={listener})", event.name()),
            );
        }
        // ABI law: batches submitted during dispatch are applied AFTER the
        // event driver finishes (see handle_ui_event) — mutating the tree
        // mid-dispatch invalidates the driver's node chain (learned the hard
        // way: Remove mid-chain panicked blitz-dom node.rs:1119).
        let _ = doc;
        match self.rt.deliver_event(&eo, record_flags, target, listener) {
            Ok(flags) => {
                if preventable && flags & 1 != 0 {
                    event_state.prevent_default();
                }
                if flags & 2 != 0 {
                    event_state.stop_propagation();
                }
                if !self.rt.shared.lock().unwrap().batches.is_empty() {
                    self.mutated = true;
                    event.request_redraw = true;
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

    fn handle_script_cmd(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        cmd: &crate::script::ScriptCmd,
    ) {
        use crate::script::ScriptCmd as C;
        if let C::Quit = cmd {
            crate::logger::log("script", "quit");
            event_loop.exit();
            return;
        }
        let Some(window) = self.inner.windows.values_mut().next() else {
            crate::logger::log("script", "command before window exists; dropped");
            return;
        };
        let doc = window.downcast_doc_mut::<GwbDocument>();
        match cmd {
            C::Click(sel) => {
                let ok = doc.script_click(sel);
                crate::logger::log("script", &format!("click {sel}: {}", if ok { "hit" } else { "NO MATCH" }));
            }
            C::Type(text) => {
                doc.script_type(text);
                crate::logger::log("script", &format!("type \"{text}\""));
            }
            C::Focus(sel) => {
                let ok = doc.script_focus(sel);
                crate::logger::log("script", &format!("focus {sel}: {}", if ok { "ok" } else { "NO MATCH" }));
            }
            C::Dump(path) => match doc.dump_dom(path) {
                Ok(()) => crate::logger::log("script", &format!("dumped DOM to {path}")),
                Err(e) => crate::logger::log("script", &format!("dump {path} FAILED: {e}")),
            },
            C::Quit => unreachable!(),
        }
        window.poll();
        window.request_redraw();
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
        // Give each GWB document its window id (request_frame wake-ups).
        let ids: Vec<WindowId> = self.inner.windows.keys().copied().collect();
        for id in ids {
            if let Some(window) = self.inner.windows.get_mut(&id) {
                window.downcast_doc_mut::<GwbDocument>().set_window_id(id);
            }
        }
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
        // Capture window-level facts the guest may subscribe to before the
        // event is consumed by the inner application.
        let notify: Option<crate::abi::EventOut> = match &event {
            WindowEvent::SurfaceResized(size) => {
                Some(crate::abi::EventOut::window_resize(size.width as f32, size.height as f32, 1.0))
            }
            WindowEvent::ThemeChanged(theme) => Some(crate::abi::EventOut::theme_change(matches!(
                theme,
                winit::window::Theme::Dark
            ))),
            _ => None,
        };
        self.inner.window_event(event_loop, window_id, event);
        if let Some(eo) = notify {
            if let Some(window) = self.inner.windows.get_mut(&window_id) {
                window.downcast_doc_mut::<GwbDocument>().notify_window_event(eo);
                window.poll();
                window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
        // Frame pump: deliver pending gwb_frame ticks, paced by the redraw
        // stream (poll -> dirty -> request_redraw -> vsync present -> next
        // about_to_wait). Idle guests cost nothing here.
        for window in self.inner.windows.values_mut() {
            let doc = window.downcast_doc_mut::<GwbDocument>();
            if doc.frame_pending() {
                window.poll();
                window.request_redraw();
            }
        }
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
                        Err(other) => {
                            if let Ok(cmd) = other.downcast::<crate::script::ScriptCmd>() {
                                self.handle_script_cmd(event_loop, &cmd);
                            }
                        }
                    }
                }
                event => self.inner.handle_blitz_shell_event(event_loop, event),
            }
        }
    }
}
