//! `--script` driver support: selector resolution, synthetic input, DOM dumps.

use super::*;

impl GwbDocument {
    // ---- scripted-driver support ------------------------------------

    pub(super) fn resolve(&self, selector: &str) -> Option<usize> {
        self.doc.query_selector(selector).ok().flatten()
    }

    /// Dispatch a synthetic click DomEvent at the selected element. Unlike a
    /// pointer sequence this is NOT hit-tested — it reaches the element even
    /// when scrolled out of view (deliberate: this is a test driver, and
    /// "scroll into view first" is the app's concern, not the assertion's).
    pub fn script_click(&mut self, selector: &str) -> bool {
        // synthetic_click_event reads layout positions; see resolve_if_stale.
        self.resolve_if_stale();
        let Some(nid) = self.resolve(selector) else { return false };
        let Some(node) = self.doc.get_node(nid) else { return false };
        let data = node.synthetic_click_event(Modifiers::empty());
        let is_input = node
            .element_data()
            .is_some_and(|el| matches!(&*el.name.local, "input" | "textarea"));
        self.script_dispatch(nid, data);
        // The pointer-down default (focus-on-click) didn't run; emulate it so
        // `click #input` + `type ...` composes naturally (caret at end). The
        // synthetic Focus event keeps guest onFocus handlers honest too.
        if is_input {
            if let Some(nid) = self.resolve(selector) {
                crate::abi::focus(&mut self.doc, nid);
                self.caret_to_end(nid);
                self.script_dispatch(
                    nid,
                    blitz_traits::events::DomEventData::Focus(blitz_traits::events::BlitzFocusEvent),
                );
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
        crate::abi::focus(&mut self.doc, nid);
        self.script_dispatch(
            nid,
            blitz_traits::events::DomEventData::Focus(blitz_traits::events::BlitzFocusEvent),
        );
        true
    }

    /// Build a `ShellEventHandler` scoped to the active tab. Computes the
    /// tab-chrome id list BEFORE taking the active tab's mutable borrow (both
    /// would otherwise borrow `self.tabs` at once).
    /// Upstream Blitz (f550417) hit-tests through cached `paint_children` /
    /// `layout_parent` ids that only the next layout pass refreshes. An input
    /// event arriving after a DOM removal but before the next redraw walks
    /// freed node ids and panics (`node.rs` `hit_inner` / `absolute_position`).
    /// The `gwb-perf` fork patched those sites; here we re-resolve before
    /// dispatching whenever the DOM changed since we last did. That's at most
    /// one extra (incremental) resolve per burst of mutations.
    pub(super) fn resolve_if_stale(&mut self) {
        if self.layout_stale || self.dirty {
            let t = self.started.elapsed().as_secs_f64();
            self.doc.resolve(t);
            self.layout_stale = false;
        }
    }

    pub(super) fn dispatch_with_shell_handler<F: FnOnce(&mut EventDriver<'_, ShellEventHandler<'_>>)>(&mut self, f: F) {
        self.resolve_if_stale();
        let tab_chrome: Vec<(u32, usize, usize)> =
            self.tabs.iter().map(|t| (t.id, t.chip_node, t.close_node)).collect();
        let bookmark_chrome: Vec<(String, usize, usize)> = self
            .bookmarks
            .iter()
            .map(|b| (b.target.clone(), b.row_node, b.remove_node))
            .collect();
        let active = self.active_idx;
        let guest = if active < self.tabs.len() {
            let tab = &mut self.tabs[active];
            tab.guest.as_mut().map(|rt| (rt, &mut tab.maps))
        } else {
            None
        };
        let handler = ShellEventHandler {
            guest,
            chrome: self.chrome,
            tab_chrome: &tab_chrome,
            bookmark_chrome: &bookmark_chrome,
            actions: &mut self.chrome_actions,
        };
        let mut driver = EventDriver::new(&mut self.doc, handler);
        f(&mut driver);
    }

    /// Dispatch one synthetic DomEvent at a node through the shell + guest
    /// event pipeline (shared by click/hover/unhover/rclick/wheel).
    pub(super) fn script_dispatch(&mut self, nid: usize, data: blitz_traits::events::DomEventData) {
        let event = DomEvent::new(nid, data);
        self.dispatch_with_shell_handler(|driver| driver.handle_dom_event(event));
        if self.tabs.get(self.active_idx).is_some_and(|t| t.guest.is_some()) {
            self.apply_active_guest_batches();
            self.dirty = true;
            self.check_observations();
        }
        self.process_chrome_actions();
    }

    /// Pointer-family synthesis: borrow the click event's coords/buttons and
    /// rewrap as the requested variant. Same non-hit-tested contract as
    /// script_click.
    pub(super) fn script_pointer(
        &mut self,
        selector: &str,
        wrap: fn(blitz_traits::events::BlitzPointerEvent) -> blitz_traits::events::DomEventData,
    ) -> bool {
        use blitz_traits::events::DomEventData as D;
        self.resolve_if_stale();
        let Some(nid) = self.resolve(selector) else { return false };
        let Some(node) = self.doc.get_node(nid) else { return false };
        let D::Click(pe) = node.synthetic_click_event(Modifiers::empty()) else { return false };
        self.script_dispatch(nid, wrap(pe));
        true
    }

    pub fn script_hover(&mut self, selector: &str) -> bool {
        self.script_pointer(selector, blitz_traits::events::DomEventData::PointerEnter)
    }

    pub fn script_unhover(&mut self, selector: &str) -> bool {
        self.script_pointer(selector, blitz_traits::events::DomEventData::PointerLeave)
    }

    pub fn script_rclick(&mut self, selector: &str) -> bool {
        self.script_pointer(selector, blitz_traits::events::DomEventData::ContextMenu)
    }

    pub fn script_wheel(&mut self, selector: &str, dy: f64) -> bool {
        use blitz_traits::events::{
            BlitzWheelDelta, BlitzWheelEvent, DomEventData as D, MouseEventButtons, PointerCoords,
        };
        self.resolve_if_stale();
        let Some(nid) = self.resolve(selector) else { return false };
        let ev = BlitzWheelEvent {
            delta: BlitzWheelDelta::Pixels(0.0, dy),
            coords: PointerCoords {
                page_x: 0.0,
                page_y: 0.0,
                screen_x: 0.0,
                screen_y: 0.0,
                client_x: 0.0,
                client_y: 0.0,
            },
            buttons: MouseEventButtons::empty(),
            mods: Modifiers::empty(),
        };
        self.script_dispatch(nid, D::Wheel(ev));
        true
    }

    /// Press a named key through the real keyboard pipeline (goes to the
    /// focused element, exactly like a physical key).
    pub fn script_key(&mut self, name: &str) -> bool {
        let (key, code) = match name {
            "Enter" => (Key::Enter, Code::Enter),
            "Escape" => (Key::Escape, Code::Escape),
            "Tab" => (Key::Tab, Code::Tab),
            "End" => (Key::End, Code::End),
            "Home" => (Key::Home, Code::Home),
            _ => return false,
        };
        let k = BlitzKeyEvent {
            key,
            code,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        };
        self.handle_ui_event(UiEvent::KeyDown(k));
        true
    }

    /// Serialize the active tab's guest mount subtree as indented pseudo-HTML.
    pub fn dump_dom(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::new();
        if let Some(tab) = self.tabs.get(self.active_idx) {
            if let Some(&root) = tab.maps.fwd.get(&1) {
                self.dump_node(root, 0, &mut out);
            }
        }
        std::fs::write(path, out)
    }

    /// DEBUG: dump the whole #app subtree (every tab's mount div, with its
    /// inline style attribute) — temporary, for diagnosing tab-visibility bugs.
    pub fn dump_app(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::new();
        self.dump_node(self.chrome.shell, 0, &mut out);
        std::fs::write(path, out)
    }

    pub(super) fn dump_node(&self, id: usize, depth: usize, out: &mut String) {
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
            out.push_str(&format!(
                " layout=(x={:.1},y={:.1},w={:.1},h={:.1})",
                node.final_layout.location.x,
                node.final_layout.location.y,
                node.final_layout.size.width,
                node.final_layout.size.height,
            ));
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
}
