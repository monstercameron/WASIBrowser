//! `GwbApplication`: the winit application wrapper around BlitzApplication.

use super::*;

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
            C::Hover(sel) => {
                let ok = doc.script_hover(sel);
                crate::logger::log("script", &format!("hover {sel}: {}", if ok { "hit" } else { "NO MATCH" }));
            }
            C::Unhover(sel) => {
                let ok = doc.script_unhover(sel);
                crate::logger::log("script", &format!("unhover {sel}: {}", if ok { "hit" } else { "NO MATCH" }));
            }
            C::RClick(sel) => {
                let ok = doc.script_rclick(sel);
                crate::logger::log("script", &format!("rclick {sel}: {}", if ok { "hit" } else { "NO MATCH" }));
            }
            C::Key(name) => {
                let ok = doc.script_key(name);
                crate::logger::log("script", &format!("key {name}: {}", if ok { "sent" } else { "UNKNOWN KEY" }));
            }
            C::Wheel(sel, dy) => {
                let ok = doc.script_wheel(sel, *dy);
                crate::logger::log("script", &format!("wheel {sel} {dy}: {}", if ok { "hit" } else { "NO MATCH" }));
            }
            C::Dump(path) => match doc.dump_dom(path) {
                Ok(()) => crate::logger::log("script", &format!("dumped DOM to {path}")),
                Err(e) => crate::logger::log("script", &format!("dump {path} FAILED: {e}")),
            },
            C::DumpApp(path) => match doc.dump_app(path) {
                Ok(()) => crate::logger::log("script", &format!("dumped #app to {path}")),
                Err(e) => crate::logger::log("script", &format!("dumpapp {path} FAILED: {e}")),
            },
            C::Navigate(target) => {
                doc.navigate_active(target);
                crate::logger::log("script", &format!("navigate {target}"));
            }
            C::Back => {
                doc.back_script();
                crate::logger::log("script", "back");
            }
            C::Forward => {
                doc.forward_script();
                crate::logger::log("script", "forward");
            }
            C::Reload => {
                doc.reload_script();
                crate::logger::log("script", "reload");
            }
            C::NewTab(target) => {
                doc.new_tab_script(target);
                crate::logger::log("script", &format!("newtab {target}"));
            }
            C::CloseTab => {
                doc.close_active_tab_script();
                crate::logger::log("script", "closetab");
            }
            C::SwitchTab(idx) => {
                doc.switch_tab_script(*idx);
                crate::logger::log("script", &format!("switchtab {idx}"));
            }
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
        // Console divider drag: tracked at window level so fast pointer moves
        // can't escape the 6px divider (the DOM pointer-down on #divider
        // starts it; any button release ends it).
        if let Some(window) = self.inner.windows.get_mut(&window_id) {
            let doc = window.downcast_doc_mut::<GwbDocument>();
            if doc.console_drag_active() {
                match &event {
                    WindowEvent::PointerMoved { position, .. } => {
                        doc.drag_console_to(position.y);
                        window.poll();
                        window.request_redraw();
                        return; // consumed: no hover churn mid-drag
                    }
                    WindowEvent::PointerButton { state, .. } if !state.is_pressed() => {
                        doc.end_console_drag();
                        // fall through so blitz clears its pressed-button state
                    }
                    _ => {}
                }
            }
        }
        // Raw-input diagnostics: does the OS event reach us at all?
        if let WindowEvent::PointerButton { state, position, .. } = &event {
            if state.is_pressed() {
                crate::logger::log(
                    "hit",
                    &format!("raw pointer-down physical=({:.0},{:.0})", position.x, position.y),
                );
            }
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
                let doc = window.downcast_doc_mut::<GwbDocument>();
                if eo.kind == crate::abi::ev::WINDOW_RESIZE {
                    doc.clamp_console_to_window();
                }
                doc.notify_window_event(eo);
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
        let mut should_exit = false;
        for window in self.inner.windows.values_mut() {
            let doc = window.downcast_doc_mut::<GwbDocument>();
            let pending = doc.frame_pending();
            if doc.close_window_requested() {
                should_exit = true;
            }
            if pending {
                window.poll();
                window.request_redraw();
            }
        }
        if should_exit {
            crate::logger::log("ui", "last tab closed; exiting");
            event_loop.exit();
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
                        Err(other) => match other.downcast::<crate::script::ScriptCmd>() {
                            Ok(cmd) => self.handle_script_cmd(event_loop, &cmd),
                            Err(other) => match other.downcast::<crate::abi::NetResult>() {
                                Ok(net) => {
                                    for window in self.inner.windows.values_mut() {
                                        let doc = window.downcast_doc_mut::<GwbDocument>();
                                        doc.deliver_net_result(&net);
                                        window.poll();
                                        window.request_redraw();
                                    }
                                }
                                Err(other) => match other.downcast::<crate::abi::RpcResult>() {
                                    Ok(rpc) => {
                                        for window in self.inner.windows.values_mut() {
                                            let doc = window.downcast_doc_mut::<GwbDocument>();
                                            doc.deliver_rpc_result(&rpc);
                                            window.poll();
                                            window.request_redraw();
                                        }
                                    }
                                    Err(other) => {
                                        if let Ok(nav) = other.downcast::<crate::abi::NavRequest>() {
                                            for window in self.inner.windows.values_mut() {
                                                let doc = window.downcast_doc_mut::<GwbDocument>();
                                                doc.deliver_nav_request(&nav);
                                                window.poll();
                                                window.request_redraw();
                                            }
                                        }
                                    }
                                },
                            },
                        },
                    }
                }
                event => self.inner.handle_blitz_shell_event(event_loop, event),
            }
        }
    }
}
