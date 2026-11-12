//! Window shell + in-window console + tabs/address-bar/history chrome.
//!
//! The console is host-owned DOM inside the Blitz document — debugging UI
//! rides the same Stylo/Vello pipeline as everything else, no second toolkit.
//! Guest stdout/stderr arrive as [`ConsoleMsg`] payloads on
//! `BlitzShellEvent::Embedder` and get appended as console lines.
//!
//! Tabs are multiplexed INSIDE this one host `Document`: Blitz's
//! `BlitzApplication`/`WindowConfig` binds exactly one `Document` to one
//! winit window/GPU surface, so multiple concurrent guest sessions live as
//! multiple `#mount-N` divs (only the active one visible) rather than as
//! multiple real windows.

use std::collections::VecDeque;
use std::sync::Arc;

use anyrender_vello::VelloWindowRenderer;
use blitz_dom::{
    DocGuard, DocGuardMut, Document, DocumentConfig, EventDriver, EventHandler, LocalName,
    NoopEventHandler, QualName, ns,
};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_shell::{BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig};
use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, DomEvent, EventState, KeyState, UiEvent};
use keyboard_types::{Code, Key, Location, Modifiers};
use wasmtime::Engine;

use crate::abi::{LockExt, SafeMutate};

mod app;
mod bookmarks;
mod script_driver;

pub use app::GwbApplication;
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

const SHELL_HTML: &str = include_str!("shell.html");

/// Truncate a tab title to a fixed char budget with an ASCII "..." marker
/// (never a Unicode ellipsis glyph — same reasoning as the reload button:
/// don't gamble on a codepoint Blitz's font fallback may not cover). Done
/// host-side rather than via CSS `text-overflow: ellipsis`, which Blitz
/// doesn't render (long titles hard-clip with no marker otherwise).
fn truncate_title(s: &str) -> String {
    const MAX_CHARS: usize = 26;
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX_CHARS).collect();
    out.push_str("...");
    out
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A plain HTML element/attribute QualName (host chrome nodes only — guest
/// tags/attrs go through the atom table in abi.rs; this is the same
/// construction, used directly since host chrome has no atom indirection).
fn qn(tag: &str) -> QualName {
    QualName::new(None, ns!(html), LocalName::from(tag))
}

/// An attribute-name QualName (null namespace — matches how html5ever tags
/// plain HTML attributes when parsing SHELL_HTML). `set_attribute` replaces
/// an existing attribute only when the QualName matches exactly, including
/// namespace: using `qn()`'s `ns!(html)` for an attribute name that already
/// exists from static parsing doesn't match the parser's null-namespace
/// entry, so it silently APPENDS A DUPLICATE instead of updating it — style
/// matching then sees only the first (stale) value. This bit both the
/// address bar's live value and the tabs-side layout toggle before this
/// helper existed (both target nodes with a static-parsed attribute).
fn attr_qn(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

/// Host-chrome node ids, resolved once from the static shell HTML.
#[derive(Clone, Copy)]
struct ChromeNodes {
    shell: usize,
    btn_newtab: usize,
    app_root: usize,
    console: usize,
    divider: usize,
    tb_console: usize,
    tb_clear: usize,
    tb_theme: usize,
    tb_tabpos: usize,
    ab_back: usize,
    ab_fwd: usize,
    ab_reload: usize,
    ab_input: usize,
    ab_go: usize,
    ab_bookmark: usize,
    tb_menu: usize,
    toolbar_menu: usize,
    tb_bookmarks: usize,
    bookmarks_panel: usize,
    bookmarks_empty: usize,
}

/// A chrome interaction noticed during event dispatch; processed after the
/// driver returns (mutating the doc mid-dispatch invalidates node chains).
enum ChromeAction {
    ToggleConsole,
    ClearConsole,
    ToggleTheme,
    ToggleTabsSide,
    DividerDown,
    NavBack,
    NavForward,
    NavReload,
    AddressInput(String),
    AddressSubmit,
    NewTab,
    CloseTab(u32),
    SwitchTab(u32),
    ToggleMenu,
    ToggleBookmark,
    ToggleBookmarksPanel,
    OpenBookmark(String),
    RemoveBookmark(String),
    /// The active tab's guest trapped while handling a UI event.
    GuestFault(String),
}

/// One saved page: title + `web://` target (or bare `.wasm` path), plus the
/// host chrome nodes for its row in the bookmarks panel (rebuilt whenever the
/// row is created; torn down with the row on removal).
struct Bookmark {
    title: String,
    target: String,
    row_node: usize,
    remove_node: usize,
}

/// One tab's guest session: its own runtime, node maps, mount point, chrome
/// (tab-chip) nodes, and navigation history. Only `GwbDocument::active_idx`'s
/// tab receives UI-event routing and the animation-frame pump; every tab
/// still receives its own async fetch/RPC completions regardless of whether
/// it's active (routed by `id`, not by focus — see `deliver_net_result`).
struct TabState {
    id: u32,
    guest: Option<crate::abi::GuestRuntime>,
    maps: crate::abi::NodeMaps,
    /// Host-owned wrapper around the mount: the tab's show/hide toggle, its
    /// CSS scope (`.gwb-scope-t{id}`), and a positioned, clipped box so guest
    /// content (incl. `position: fixed`, which Blitz lays out as absolute)
    /// can never paint over the browser chrome. Guests can't address it.
    wrap_node: usize,
    /// Host-owned hidden container for this tab's scoped guest stylesheets.
    styles_host: usize,
    mount_node: usize,
    chip_node: usize,
    label_node: usize,
    close_node: usize,
    title: String,
    target: String,
    history: Vec<String>,
    history_idx: usize,
}

/// The renderer's document: host-owned shell chrome (tabs/address-bar/
/// console) with N tab guest sessions, one active at a time.
pub struct GwbDocument {
    doc: HtmlDocument,
    log_node: usize,
    lines: VecDeque<(Source, String)>,
    partial_out: String,
    partial_err: String,
    dirty: bool,
    /// View-option state: dark mode requested via the toolbar (delivered to
    /// the guest as a THEME_CHANGE event; the guest owns its own look).
    theme_dark: bool,
    /// Shared across every tab's guest load (wasmtime `Engine`s are meant to
    /// be cheaply shared across many `Store`s).
    engine: Engine,
    proxy: BlitzShellProxy,
    manifest_root: String,
    window_id: Option<WindowId>,
    tabs: Vec<TabState>,
    active_idx: usize,
    next_tab_id: u32,
    tabs_side: bool,
    /// Live address-bar text, captured from the input's own Input events
    /// (Blitz reports the full current value on each Input, not a delta).
    addr_draft: String,
    close_window_requested: bool,
    last_frame: Option<std::time::Instant>,
    chrome: ChromeNodes,
    chrome_actions: Vec<ChromeAction>,
    console_visible: bool,
    /// Console height in CSS px (the divider drag adjusts it).
    console_height: f32,
    console_drag: bool,
    /// Whether the kebab overflow menu (tabs position/theme/console actions)
    /// is currently open.
    menu_open: bool,
    bookmarks: Vec<Bookmark>,
    bookmarks_panel_open: bool,
    /// The DOM changed since the host last resolved style + layout itself.
    /// See `resolve_if_stale`.
    layout_stale: bool,
    /// Animation clock for host-triggered resolves (seconds since startup,
    /// the same base Blitz's shell uses).
    started: std::time::Instant,
}

impl GwbDocument {
    pub fn new(engine: Engine, proxy: BlitzShellProxy, manifest_root: String) -> Self {
        let config = DocumentConfig {
            // set_inner_html needs a parser provider at mutation time.
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            // A base Blitz can resolve against (its default `data:` base
            // panics on any relative src/href). No net provider is set, so
            // nothing is ever fetched from it.
            base_url: Some(crate::abi::DOC_BASE_URL.to_string()),
            ..Default::default()
        };
        let doc = HtmlDocument::from_html(SHELL_HTML, config);
        let log_node = doc
            .query_selector("#console-log")
            .expect("selector parses")
            .expect("#console-log exists in shell HTML");
        let sel = |s: &str| {
            doc.query_selector(s)
                .expect("selector parses")
                .unwrap_or_else(|| panic!("{s} exists in shell HTML"))
        };
        let chrome = ChromeNodes {
            shell: sel("#shell"),
            btn_newtab: sel("#btn-newtab"),
            app_root: sel("#app"),
            console: sel("#console"),
            divider: sel("#divider"),
            tb_console: sel("#tb-console"),
            tb_clear: sel("#tb-clear"),
            tb_theme: sel("#tb-theme"),
            tb_tabpos: sel("#tb-tabpos"),
            ab_back: sel("#ab-back"),
            ab_fwd: sel("#ab-fwd"),
            ab_reload: sel("#ab-reload"),
            ab_input: sel("#ab-input"),
            ab_go: sel("#ab-go"),
            ab_bookmark: sel("#ab-bookmark"),
            tb_menu: sel("#tb-menu"),
            toolbar_menu: sel("#toolbar-menu"),
            tb_bookmarks: sel("#tb-bookmarks"),
            bookmarks_panel: sel("#bookmarks-panel"),
            bookmarks_empty: sel("#bookmarks-empty"),
        };
        let mut me = Self {
            doc,
            log_node,
            lines: VecDeque::new(),
            partial_out: String::new(),
            partial_err: String::new(),
            dirty: false,
            theme_dark: false,
            engine,
            proxy,
            manifest_root,
            window_id: None,
            tabs: Vec::new(),
            active_idx: usize::MAX, // sentinel: no tab yet, see open_new_tab/activate_tab
            next_tab_id: 0,
            tabs_side: false,
            addr_draft: String::new(),
            close_window_requested: false,
            last_frame: None,
            chrome,
            chrome_actions: Vec::new(),
            console_visible: true,
            console_height: 240.0,
            console_drag: false,
            menu_open: false,
            bookmarks: Vec::new(),
            bookmarks_panel_open: false,
            layout_stale: true,
            started: std::time::Instant::now(),
        };
        let idx = me.open_new_tab();
        me.activate_tab(idx);
        me.set_console_visible(false);
        me.load_bookmarks();
        me
    }

    // ---- tabs: create / activate / close --------------------------------

    /// Create a fresh, guest-less tab (mount div + tab chip), not yet active.
    /// Returns its index. Caller decides whether/when to activate it.
    fn open_new_tab(&mut self) -> usize {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let (chip_node, label_node, close_node) = self.create_tab_chip("New Tab");
        let (styles_host, wrap_node, mount_node) = {
            let mut m = self.doc.mutate();
            let styles = m.create_element(qn("div"), Vec::new());
            m.set_attr(styles, attr_qn("class"), "guest-styles");
            let wrap = m.create_element(qn("div"), Vec::new());
            m.set_attr(wrap, attr_qn("class"), &format!("mount-wrap gwb-scope-t{id}"));
            m.set_style_property(wrap, "display", "none");
            let n = m.create_element(qn("div"), Vec::new());
            m.set_attr(n, attr_qn("class"), "mount");
            // The wrapper's ONLY child, ever: the scope selector relies on it.
            m.append_children(wrap, &[n]);
            m.append_children(self.chrome.app_root, &[styles, wrap]);
            (styles, wrap, n)
        };
        // Guest id 1 = the mount root (spec: the only host node a guest can
        // address), re-seeded fresh for every tab/navigation.
        let maps = Self::fresh_maps(id, mount_node, styles_host);
        self.tabs.push(TabState {
            id,
            guest: None,
            maps,
            wrap_node,
            styles_host,
            mount_node,
            chip_node,
            label_node,
            close_node,
            title: "New Tab".to_string(),
            target: String::new(),
            history: Vec::new(),
            history_idx: 0,
        });
        self.tabs.len() - 1
    }

    /// Node maps for a (re)loaded tab: guest id 1 = the mount, and guest CSS
    /// scoped to `.gwb-scope-t{id}>*` (the wrapper's only child, the mount).
    fn fresh_maps(tab_id: u32, mount_node: usize, styles_host: usize) -> crate::abi::NodeMaps {
        crate::abi::NodeMaps::for_mount(
            mount_node,
            crate::abi::StyleScope {
                mount: mount_node,
                styles_host,
                prefix: format!(".gwb-scope-t{tab_id}>*"),
            },
        )
    }

    /// A guest call returned an error. Log it; if the guest itself faulted
    /// (a wasm trap: CPU budget, out-of-bounds, unreachable, stack overflow)
    /// unload it. Host-side errors (e.g. an event that didn't fit the event
    /// region) leave the guest running.
    fn guest_call_failed(&mut self, tab_idx: usize, what: &str, e: anyhow::Error) {
        crate::logger::log("crash", &format!("{what} failed: {e:#}"));
        if let Some(why) = crate::abi::guest_fault(&e) {
            self.kill_guest(tab_idx, &why);
        }
    }

    /// Unload a tab's guest after it trapped (CPU budget, memory limit, or
    /// a wasm trap). A trapped instance can be left mid-update, and a guest
    /// stuck in a loop would otherwise burn its full budget on every event.
    fn kill_guest(&mut self, tab_idx: usize, why: &str) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else { return };
        if tab.guest.take().is_none() {
            return;
        }
        let (mount, styles, tab_id) = (tab.mount_node, tab.styles_host, tab.id);
        tab.maps = Self::fresh_maps(tab_id, mount, styles);
        {
            let mut m = self.doc.mutate();
            m.drop_children(mount);
            m.drop_children(styles);
            let msg = m.create_element(qn("div"), Vec::new());
            m.set_style_property(msg, "padding", "24px");
            m.set_style_property(msg, "font-family", "'Segoe UI', sans-serif");
            m.set_style_property(msg, "color", "#b3261e");
            let text = m.create_text_node(&format!("This app stopped: {why}"));
            m.append_children(msg, &[text]);
            m.append_children(mount, &[msg]);
        }
        crate::logger::log("crash", &format!("tab {tab_id}: guest unloaded: {why}"));
        host_console_line(&self.proxy, &format!("[host] tab {tab_id}: app stopped: {why}"));
        self.dirty = true;
    }

    fn create_tab_chip(&mut self, title: &str) -> (usize, usize, usize) {
        let mut m = self.doc.mutate();
        let chip = m.create_element(qn("div"), Vec::new());
        m.set_attr(chip, attr_qn("class"), "tab-chip");
        let label = m.create_element(qn("span"), Vec::new());
        m.set_attr(label, attr_qn("class"), "tab-label");
        m.set_html(label, &escape_html(&truncate_title(title)));
        let close = m.create_element(qn("span"), Vec::new());
        m.set_attr(close, attr_qn("class"), "tab-close");
        m.set_html(close, "&times;");
        m.append_children(chip, &[label, close]);
        m.insert_nodes_before(self.chrome.btn_newtab, &[chip]);
        drop(m);
        (chip, label, close)
    }

    fn activate_tab(&mut self, idx: usize) {
        if idx == self.active_idx || idx >= self.tabs.len() {
            return;
        }
        let mut m = self.doc.mutate();
        if let Some(old) = self.tabs.get(self.active_idx) {
            m.set_style_property(old.wrap_node, "display", "none");
            m.set_attr(old.chip_node, attr_qn("class"), "tab-chip");
        }
        let new = &self.tabs[idx];
        m.set_style_property(new.wrap_node, "display", "flex");
        m.set_attr(new.chip_node, attr_qn("class"), "tab-chip active");
        drop(m);
        self.active_idx = idx;
        self.dirty = true;
        self.refresh_address_bar();
    }

    fn new_tab(&mut self) {
        let idx = self.open_new_tab();
        self.activate_tab(idx);
        self.addr_draft.clear();
        let mut m = self.doc.mutate();
        m.set_attr(self.chrome.ab_input, attr_qn("value"), "");
        drop(m);
        crate::abi::focus(&mut self.doc, self.chrome.ab_input);
    }

    fn close_tab(&mut self, tab_id: u32) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            return;
        };
        if self.tabs.len() == 1 {
            // Last tab: the host has no direct window handle here — signal
            // GwbApplication::about_to_wait to exit the event loop instead.
            self.close_window_requested = true;
            return;
        }
        let t = &self.tabs[idx];
        let (wrap, styles, chip) = (t.wrap_node, t.styles_host, t.chip_node);
        {
            let mut m = self.doc.mutate();
            m.drop_node(wrap); // takes the mount with it
            m.drop_node(styles);
            m.drop_node(chip);
        }
        self.tabs.remove(idx); // drops GuestRuntime — safe/synchronous teardown
        if self.active_idx == idx {
            // Activate the previous tab, or the next one (now at the same
            // index post-removal) if we closed index 0 — same policy as the
            // legacy Go window.go's close/reactivate-adjacent.
            let target = if idx > 0 { idx - 1 } else { 0 };
            self.active_idx = usize::MAX; // sentinel so activate_tab doesn't no-op
            self.activate_tab(target);
        } else if self.active_idx > idx {
            self.active_idx -= 1;
        }
    }

    pub fn close_window_requested(&mut self) -> bool {
        std::mem::take(&mut self.close_window_requested)
    }

    fn refresh_address_bar(&mut self) {
        let text = self.tabs.get(self.active_idx).map(|t| t.target.clone()).unwrap_or_default();
        self.addr_draft = text.clone();
        let mut m = self.doc.mutate();
        m.set_attr(self.chrome.ab_input, attr_qn("value"), &text);
        drop(m);
        self.dirty = true;
        self.refresh_bookmark_star();
    }

    fn update_tab_label(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get(idx) else { return };
        let label_node = tab.label_node;
        let title = tab.title.clone();
        let mut m = self.doc.mutate();
        m.set_html(label_node, &escape_html(&truncate_title(&title)));
        drop(m);
        self.dirty = true;
    }

    // ---- navigation core --------------------------------------------------

    /// Resolve `target` (a `web://name` address or a `.wasm` path), load it
    /// via the shared Engine into `tab_idx`'s mount point, and (optionally)
    /// push it onto that tab's back/forward history. Drives address-bar
    /// submit, back/forward/reload, initial tab load, and new-tab loads —
    /// one code path for all of them. Returns true iff a GWB guest was
    /// loaded and started (false for a resolve/load failure or a target that
    /// doesn't export the GWB ABI — the caller may fall back to legacy
    /// console-mode for the very first boot target; see main.rs).
    fn navigate(&mut self, tab_idx: usize, target: &str, push_history: bool) -> bool {
        let target = target.trim();
        if target.is_empty() {
            return false;
        }
        let resolved = match crate::manifest::resolve(target, &self.manifest_root) {
            Ok(r) => r,
            Err(e) => {
                crate::logger::log("sys", &format!("navigate '{target}' FAILED: {e:#}"));
                return false;
            }
        };
        if resolved.dev_unverified {
            crate::logger::log(
                "rpc",
                &format!("dev: loading unverified bundle '{}' (no b3: check)", resolved.bundle_path),
            );
        }

        // Drop any previous guest for this tab and clear its mount subtree
        // before loading the new one (Store/Memory teardown is synchronous —
        // no custom Drop needed, per the abi.rs guest-lifecycle review).
        self.tabs[tab_idx].guest = None;
        let mount_node = self.tabs[tab_idx].mount_node;
        let styles_host = self.tabs[tab_idx].styles_host;
        {
            let mut m = self.doc.mutate();
            m.drop_children(mount_node);
            m.drop_children(styles_host);
            // A guest can set inline style/attrs on its mount (id 1); reset
            // them so the next app starts from the host's defaults.
            m.set_attr(mount_node, attr_qn("class"), "mount");
            m.clear_attr(mount_node, attr_qn("style"));
        }
        self.tabs[tab_idx].maps = Self::fresh_maps(self.tabs[tab_idx].id, mount_node, styles_host);

        let (w, h, scale) = {
            let inner = self.doc.inner();
            let vp = inner.viewport();
            if vp.window_size.0 > 0 && vp.window_size.1 > 0 {
                let s = vp.scale_f64() as f32;
                (vp.window_size.0 as f32 / s.max(0.0001), vp.window_size.1 as f32 / s.max(0.0001), s)
            } else {
                (1100.0, 800.0, 1.0)
            }
        };

        let tab_id = self.tabs[tab_idx].id;
        crate::logger::log(
            "sys",
            &format!("navigating tab {tab_id} to '{target}' -> bundle={} title=\"{}\"", resolved.bundle_path, resolved.title),
        );
        let mut loaded = false;
        match crate::abi::try_load(
            &self.engine,
            &resolved.bundle_path,
            tab_id,
            self.proxy.clone(),
            resolved.services,
            resolved.links,
            resolved.fetch,
        ) {
            Ok(Some(mut rt)) => {
                if let Some(id) = self.window_id {
                    rt.shared.lock_ok().window_id = Some(id);
                }
                if let Err(e) = rt.start(w, h, scale, self.theme_dark as u32) {
                    crate::logger::log("crash", &format!("gwb_start failed: {e:#}"));
                    host_console_line(&self.proxy, &format!("[host] app failed to start: {e:#}"));
                } else {
                    self.tabs[tab_idx].guest = Some(rt);
                    let applied = self.apply_guest_batches_for(tab_idx);
                    crate::logger::log("abi", &format!("initial mount: {applied} ops applied"));
                    self.dirty = true;
                    if tab_idx == self.active_idx {
                        self.notify_window_event(crate::abi::EventOut::page_load(w, h, scale));
                    }
                    loaded = true;
                }
            }
            Ok(None) => {
                crate::logger::log(
                    "sys",
                    &format!("'{target}' does not export the GWB ABI; legacy console-mode guests are not tab-navigable in this build"),
                );
            }
            Err(e) => {
                crate::logger::log("crash", &format!("guest load failed: {e:#}"));
                host_console_line(&self.proxy, &format!("[host] guest load FAILED: {e:#}"));
            }
        }

        self.tabs[tab_idx].title = resolved.title.clone();
        self.tabs[tab_idx].target = target.to_string();
        self.update_tab_label(tab_idx);
        if push_history {
            let t = &mut self.tabs[tab_idx];
            t.history.truncate(t.history_idx + 1);
            t.history.push(target.to_string());
            t.history_idx = t.history.len() - 1;
        }
        if tab_idx == self.active_idx {
            self.refresh_address_bar();
        }
        loaded
    }

    fn nav_back(&mut self) {
        let idx = self.active_idx;
        let Some(tab) = self.tabs.get_mut(idx) else { return };
        if tab.history_idx == 0 {
            return;
        }
        tab.history_idx -= 1;
        let target = tab.history[tab.history_idx].clone();
        self.navigate(idx, &target, false);
    }

    fn nav_forward(&mut self) {
        let idx = self.active_idx;
        let Some(tab) = self.tabs.get_mut(idx) else { return };
        if tab.history_idx + 1 >= tab.history.len() {
            return;
        }
        tab.history_idx += 1;
        let target = tab.history[tab.history_idx].clone();
        self.navigate(idx, &target, false);
    }

    fn nav_reload(&mut self) {
        let idx = self.active_idx;
        let Some(tab) = self.tabs.get(idx) else { return };
        if tab.history.is_empty() {
            return;
        }
        let target = tab.history[tab.history_idx].clone();
        self.navigate(idx, &target, false);
    }

    /// Navigate the active tab (address bar / initial load / script driver).
    /// Returns true iff a GWB guest was loaded and started.
    pub fn navigate_active(&mut self, target: &str) -> bool {
        let idx = self.active_idx;
        self.navigate(idx, target, true)
    }

    pub fn new_tab_script(&mut self, target: &str) {
        self.new_tab();
        if !target.is_empty() {
            self.navigate_active(target);
        }
    }

    pub fn close_active_tab_script(&mut self) {
        let id = self.tabs.get(self.active_idx).map(|t| t.id);
        if let Some(id) = id {
            self.close_tab(id);
        }
    }

    pub fn switch_tab_script(&mut self, idx: usize) {
        self.activate_tab(idx);
    }

    pub fn back_script(&mut self) {
        self.nav_back();
    }

    pub fn forward_script(&mut self) {
        self.nav_forward();
    }

    pub fn reload_script(&mut self) {
        self.nav_reload();
    }

    // ---- window chrome: console visibility, clear, drag-resize ----------

    fn process_chrome_actions(&mut self) {
        for action in std::mem::take(&mut self.chrome_actions) {
            match action {
                ChromeAction::ToggleConsole => {
                    self.set_console_visible(!self.console_visible);
                    self.set_menu_open(false);
                }
                ChromeAction::ClearConsole => {
                    self.lines.clear();
                    self.partial_out.clear();
                    self.partial_err.clear();
                    self.rebuild_log();
                    crate::logger::log("ui", "console cleared");
                    self.set_menu_open(false);
                }
                ChromeAction::ToggleTheme => {
                    self.theme_dark = !self.theme_dark;
                    let dark = self.theme_dark;
                    let mut m = self.doc.mutate();
                    m.set_html(
                        self.chrome.tb_theme,
                        if dark { "light mode" } else { "dark mode" },
                    );
                    drop(m);
                    self.dirty = true;
                    crate::logger::log("ui", if dark { "theme -> dark" } else { "theme -> light" });
                    // The guest owns its look: deliver the standard event.
                    self.notify_window_event(crate::abi::EventOut::theme_change(dark));
                    self.set_menu_open(false);
                }
                ChromeAction::ToggleTabsSide => {
                    self.tabs_side = !self.tabs_side;
                    let side = self.tabs_side;
                    let mut m = self.doc.mutate();
                    m.set_attr(
                        self.chrome.shell,
                        attr_qn("class"),
                        if side { "layout-side" } else { "layout-top" },
                    );
                    m.set_html(self.chrome.tb_tabpos, if side { "tabs: side" } else { "tabs: top" });
                    drop(m);
                    self.dirty = true;
                    crate::logger::log("ui", if side { "tabs -> side" } else { "tabs -> top" });
                    self.set_menu_open(false);
                }
                ChromeAction::DividerDown => {
                    self.console_drag = true;
                    crate::logger::log("ui", "console resize drag started");
                }
                ChromeAction::NavBack => self.nav_back(),
                ChromeAction::NavForward => self.nav_forward(),
                ChromeAction::NavReload => self.nav_reload(),
                ChromeAction::GuestFault(why) => self.kill_guest(self.active_idx, &why),
                ChromeAction::AddressInput(text) => self.addr_draft = text,
                ChromeAction::AddressSubmit => {
                    let target = self.addr_draft.clone();
                    self.navigate_active(&target);
                }
                ChromeAction::NewTab => self.new_tab(),
                ChromeAction::CloseTab(id) => self.close_tab(id),
                ChromeAction::SwitchTab(id) => {
                    if let Some(idx) = self.tabs.iter().position(|t| t.id == id) {
                        self.activate_tab(idx);
                    }
                }
                ChromeAction::ToggleMenu => {
                    let open = !self.menu_open;
                    self.set_menu_open(open);
                    self.set_bookmarks_panel_open(false);
                }
                ChromeAction::ToggleBookmark => self.toggle_bookmark(),
                ChromeAction::ToggleBookmarksPanel => {
                    let open = !self.bookmarks_panel_open;
                    self.set_bookmarks_panel_open(open);
                    self.set_menu_open(false);
                }
                ChromeAction::OpenBookmark(target) => self.open_bookmark(target),
                ChromeAction::RemoveBookmark(target) => self.remove_bookmark(&target),
            }
        }
    }


    fn set_menu_open(&mut self, open: bool) {
        if self.menu_open == open {
            return;
        }
        self.menu_open = open;
        let mut m = self.doc.mutate();
        m.set_style_property(self.chrome.toolbar_menu, "display", if open { "flex" } else { "none" });
        drop(m);
        self.dirty = true;
    }

    fn set_console_visible(&mut self, show: bool) {
        self.console_visible = show;
        let mut m = self.doc.mutate();
        m.set_style_property(self.chrome.console, "display", if show { "flex" } else { "none" });
        m.set_style_property(self.chrome.divider, "display", if show { "block" } else { "none" });
        m.set_html(
            self.chrome.tb_console,
            if show { "hide console" } else { "show console" },
        );
        drop(m);
        self.dirty = true;
        crate::logger::log("ui", if show { "console shown" } else { "console hidden" });
    }

    fn apply_console_height(&mut self) {
        let css = format!("{}px", self.console_height.round());
        let mut m = self.doc.mutate();
        m.set_style_property(self.chrome.console, "height", &css);
        drop(m);
        self.dirty = true;
    }

    pub fn console_drag_active(&self) -> bool {
        self.console_drag
    }

    /// Drag tick: pointer at physical `y`; console takes the space below it.
    pub fn drag_console_to(&mut self, phys_y: f64) {
        let (scale, win_h) = {
            let inner = self.doc.inner();
            let vp = inner.viewport();
            (vp.scale_f64(), vp.window_size.1 as f64)
        };
        if scale <= 0.0 || win_h <= 0.0 {
            return;
        }
        let total_css = (win_h / scale) as f32;
        let new_h = (total_css - (phys_y / scale) as f32).clamp(48.0, (total_css - 120.0).max(48.0));
        if (new_h - self.console_height).abs() >= 1.0 {
            self.console_height = new_h;
            self.apply_console_height();
        }
    }

    pub fn end_console_drag(&mut self) {
        self.console_drag = false;
        crate::logger::log(
            "ui",
            &format!("console resized to {}px", self.console_height.round()),
        );
    }

    /// Keep the console from swallowing a shrunken window: clamp its height
    /// to leave at least 120 CSS px of app area.
    pub fn clamp_console_to_window(&mut self) {
        let (scale, win_h) = {
            let inner = self.doc.inner();
            let vp = inner.viewport();
            (vp.scale_f64(), vp.window_size.1 as f64)
        };
        if scale <= 0.0 || win_h <= 0.0 {
            return;
        }
        let max_h = ((win_h / scale) as f32 - 120.0).max(48.0);
        if self.console_height > max_h {
            self.console_height = max_h;
            self.apply_console_height();
        }
    }

    /// Apply everything the active tab's guest submitted; if a Focus op
    /// landed on a text input, move the caret to the end via a synthetic End
    /// keypress (the replace-render recreates inputs with the caret at 0,
    /// which made typing insert backwards).
    fn apply_guest_batches_for(&mut self, idx: usize) -> usize {
        let Some(tab) = self.tabs.get(idx) else { return 0 };
        let Some(rt) = tab.guest.as_ref() else { return 0 };
        let shared = rt.shared.clone();
        let (applied, focused) =
            crate::abi::apply_batches(&mut self.doc, &mut self.tabs[idx].maps, &shared);
        if let Some(nid) = focused {
            self.caret_to_end(nid);
        }
        applied
    }

    fn apply_active_guest_batches(&mut self) -> usize {
        self.apply_guest_batches_for(self.active_idx)
    }

    fn caret_to_end(&mut self, nid: usize) {
        let is_text_input = self
            .doc
            .get_node(nid)
            .and_then(|n| n.element_data())
            .is_some_and(|el| el.text_input_data().is_some());
        if !is_text_input {
            return;
        }
        let key = BlitzKeyEvent {
            key: Key::End,
            code: Code::End,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        };
        let mut driver = EventDriver::new(&mut self.doc, NoopEventHandler);
        driver.handle_ui_event(UiEvent::KeyDown(key));
    }

    /// Record the winit window id so request_frame can wake the event loop —
    /// propagated to every tab's already-loaded guest (a tab opened before
    /// the window surface existed) and to future ones via `navigate`.
    pub fn set_window_id(&mut self, id: WindowId) {
        self.window_id = Some(id);
        for tab in &self.tabs {
            if let Some(rt) = &tab.guest {
                rt.shared.lock_ok().window_id = Some(id);
            }
        }
    }

    /// Whether the active tab's guest has an undelivered request_frame.
    pub fn frame_pending(&self) -> bool {
        self.tabs
            .get(self.active_idx)
            .and_then(|t| t.guest.as_ref())
            .is_some_and(|rt| rt.shared.lock_ok().frame_requested)
    }

    /// Deliver a window-level event (resize/theme) to the active tab's guest
    /// if it subscribed on the mount root (guest id 1). Background tabs pick
    /// this up implicitly once they become active (their own PAGE_LOAD/
    /// resize state is refreshed on next interaction).
    pub fn notify_window_event(&mut self, eo: crate::abi::EventOut) {
        let idx = self.active_idx;
        let Some(tab) = self.tabs.get_mut(idx) else { return };
        let Some(rt) = tab.guest.as_mut() else { return };
        if !tab.maps.listeners.contains(&(1, eo.kind)) {
            return;
        }
        match rt.deliver_event(&eo, 0, 1, 1) {
            Ok(_) => {
                if self.apply_active_guest_batches() > 0 {
                    self.dirty = true;
                }
            }
            Err(e) => self.guest_call_failed(idx, "window event delivery", e),
        }
    }


    /// Deliver a completed fetch to whichever tab owns it (`r.tab_id`),
    /// regardless of whether that tab is currently active — an in-flight
    /// request started just before switching tabs must still land correctly.
    /// Body is truncated to fit the guest's registered event region.
    pub fn deliver_net_result(&mut self, r: &crate::abi::NetResult) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == r.tab_id) else {
            crate::logger::log("net", &format!("fetch #{}: owning tab closed, dropping result", r.id));
            return;
        };
        let Some(rt) = self.tabs[idx].guest.as_mut() else { return };
        let cap = rt.shared.lock_ok().event_region.1 as usize;
        let max_body = cap.saturating_sub(64);
        let mut body = r.body.clone();
        if body.len() > max_body {
            let mut cut = max_body;
            while cut > 0 && !body.is_char_boundary(cut) {
                cut -= 1;
            }
            body.truncate(cut);
            crate::logger::log(
                "net",
                &format!("fetch #{}: body truncated to {} bytes (event region)", r.id, cut),
            );
        }
        let eo = crate::abi::EventOut::net_result(r.status, r.ok, body);
        match rt.deliver_event(&eo, 0, r.id, 0) {
            Ok(_) => {
                if self.apply_guest_batches_for(idx) > 0 {
                    self.dirty = true;
                }
            }
            Err(e) => self.guest_call_failed(idx, "net delivery", e),
        }
    }

    /// Deliver a completed RPC to whichever tab owns it (`r.tab_id`), mirroring
    /// deliver_net_result (docs/04-WEB-RPC.md).
    pub fn deliver_rpc_result(&mut self, r: &crate::abi::RpcResult) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == r.tab_id) else {
            crate::logger::log("rpc", &format!("rpc #{}: owning tab closed, dropping result", r.req_id));
            return;
        };
        let Some(rt) = self.tabs[idx].guest.as_mut() else { return };
        let cap = rt.shared.lock_ok().event_region.1 as usize;
        let max_body = cap.saturating_sub(64);
        let mut body = r.body.clone();
        if body.len() > max_body {
            let mut cut = max_body;
            while cut > 0 && !body.is_char_boundary(cut) {
                cut -= 1;
            }
            body.truncate(cut);
            crate::logger::log(
                "rpc",
                &format!("rpc #{}: body truncated to {} bytes (event region)", r.req_id, cut),
            );
        }
        let eo = crate::abi::EventOut::rpc_result(r.req_id, r.status, r.ok, r.err_class, body);
        match rt.deliver_event(&eo, 0, 0, 0) {
            Ok(_) => {
                if self.apply_guest_batches_for(idx) > 0 {
                    self.dirty = true;
                }
            }
            Err(e) => self.guest_call_failed(idx, "rpc delivery", e),
        }
    }

    /// Act on a guest-requested cross-site navigation, once abi.rs has
    /// already passed the capability + gesture checks (see `navigate`'s host
    /// import). Same-tab: navigate the requesting tab in place, pushing
    /// history. New-tab: open a fresh tab next to the requester and navigate
    /// that instead — the requesting tab is untouched.
    pub fn deliver_nav_request(&mut self, req: &crate::abi::NavRequest) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == req.tab_id) else {
            crate::logger::log("nav", &format!("nav request for closed tab {}, dropping", req.tab_id));
            return;
        };
        if req.new_tab {
            let new_idx = self.open_new_tab();
            self.activate_tab(new_idx);
            self.navigate(new_idx, &req.target, true);
        } else {
            self.navigate(idx, &req.target, true);
        }
    }

    /// Emit observed_layout records for the active tab's Observe'd nodes
    /// whose geometry changed. Returns true if the guest mutated the DOM.
    fn check_observations(&mut self) -> bool {
        let idx = self.active_idx;
        let Some(tab) = self.tabs.get(idx) else { return false };
        if tab.maps.observers.is_empty() || tab.guest.is_none() {
            return false;
        }
        // Rects must come from current layout (and stale layout_parent ids
        // would panic in absolute_position; see resolve_if_stale).
        self.resolve_if_stale();
        let Some(tab) = self.tabs.get(idx) else { return false };
        // Collect changed rects first (read-only pass over the layout tree).
        let mut changes: Vec<(u32, [f32; 4])> = Vec::new();
        for (&gid, &what) in &tab.maps.observers {
            if what & crate::abi::obs::LAYOUT == 0 {
                continue;
            }
            let Some(&nid) = tab.maps.fwd.get(&gid) else { continue };
            let Some(node) = self.doc.get_node(nid) else { continue };
            let pos = node.absolute_position(0.0, 0.0);
            let size = node.final_layout.size;
            let rect = [pos.x, pos.y, size.width, size.height];
            if tab.maps.last_rects.get(&gid) != Some(&rect) {
                changes.push((gid, rect));
            }
        }
        if changes.is_empty() {
            return false;
        }
        for (gid, rect) in &changes {
            self.tabs[idx].maps.last_rects.insert(*gid, *rect);
            let eo = crate::abi::EventOut::layout_rect(*rect);
            let Some(rt) = self.tabs[idx].guest.as_mut() else { break };
            if let Err(e) = rt.deliver_event(&eo, 0, *gid, *gid) {
                self.guest_call_failed(idx, "observation delivery", e);
            }
        }
        let applied = self.apply_guest_batches_for(idx);
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
        mutr.set_html(self.log_node, &html);
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

        // Animation frames: deliver gwb_frame to the ACTIVE tab only if it
        // requested one — background tabs don't animate (legacy Go spine's
        // Background/Frozen intent), and catch up immediately once switched to.
        let active = self.active_idx;
        if let Some(tab) = self.tabs.get_mut(active) {
            if let Some(rt) = tab.guest.as_mut() {
                let wanted = std::mem::take(&mut rt.shared.lock_ok().frame_requested);
                if wanted {
                    let now = std::time::Instant::now();
                    let dt = self
                        .last_frame
                        .map(|t| now.duration_since(t).as_secs_f32() * 1000.0)
                        .unwrap_or(16.6);
                    self.last_frame = Some(now);
                    if let Err(e) = rt.deliver_frame(dt) {
                        self.guest_call_failed(active, "gwb_frame", e);
                        dirty = true;
                    }
                    if self.apply_active_guest_batches() > 0 {
                        dirty = true;
                    }
                }
            }
        }

        if self.check_observations() {
            dirty = true;
        }
        if dirty {
            // A redraw (and Blitz's resolve) is coming, but input can arrive
            // first; see `resolve_if_stale`.
            self.layout_stale = true;
        }
        dirty
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        self.dispatch_with_shell_handler(|driver| driver.handle_ui_event(event));
        // Post-dispatch: apply everything the guest submitted during the
        // dispatch, now that the driver no longer holds node chains; then
        // run any chrome actions the dispatch queued.
        if self.tabs.get(self.active_idx).is_some_and(|t| t.guest.is_some()) {
            self.apply_active_guest_batches();
            self.dirty = true;
            self.check_observations();
        }
        self.process_chrome_actions();
    }
}

/// Routes hit-tested DOM events: host chrome first (toolbar buttons, address
/// bar, tab strip, console divider), then the active tab's GWB guest — the
/// full interactive loop, inside one dispatch. Chrome actions are queued and
/// processed post-dispatch.
struct ShellEventHandler<'a> {
    guest: Option<(&'a mut crate::abi::GuestRuntime, &'a mut crate::abi::NodeMaps)>,
    chrome: ChromeNodes,
    /// (tab_id, chip_node, close_node) for every open tab — used to route
    /// tab-strip clicks without giving the handler mutable access to `tabs`.
    tab_chrome: &'a [(u32, usize, usize)],
    /// (target, row_node, remove_node) for every saved bookmark — same
    /// by-node-id routing pattern as `tab_chrome`, keyed by target instead
    /// of a stable id since a bookmark has no separate identity from its URL.
    bookmark_chrome: &'a [(String, usize, usize)],
    actions: &'a mut Vec<ChromeAction>,
}

impl EventHandler for ShellEventHandler<'_> {
    fn handle_event(
        &mut self,
        chain: &[usize],
        event: &mut DomEvent,
        doc: &mut dyn Document,
        event_state: &mut EventState,
    ) {
        use blitz_traits::events::DomEventData as D;
        // Hit-test diagnostics: where did this click actually land?
        if let D::Click(pe) = &event.data {
            crate::logger::log(
                "hit",
                &format!(
                    "click page=({:.0},{:.0}) client=({:.0},{:.0}) target={} chain_len={}",
                    pe.page_x(),
                    pe.page_y(),
                    pe.client_x(),
                    pe.client_y(),
                    event.target,
                    chain.len()
                ),
            );
        }
        match &event.data {
            D::Click(_) => {
                if chain.contains(&self.chrome.tb_console) {
                    self.actions.push(ChromeAction::ToggleConsole);
                    return;
                }
                if chain.contains(&self.chrome.tb_clear) {
                    self.actions.push(ChromeAction::ClearConsole);
                    return;
                }
                if chain.contains(&self.chrome.tb_theme) {
                    self.actions.push(ChromeAction::ToggleTheme);
                    return;
                }
                if chain.contains(&self.chrome.tb_tabpos) {
                    self.actions.push(ChromeAction::ToggleTabsSide);
                    return;
                }
                if chain.contains(&self.chrome.tb_menu) {
                    self.actions.push(ChromeAction::ToggleMenu);
                    return;
                }
                if chain.contains(&self.chrome.ab_bookmark) {
                    self.actions.push(ChromeAction::ToggleBookmark);
                    return;
                }
                if chain.contains(&self.chrome.tb_bookmarks) {
                    self.actions.push(ChromeAction::ToggleBookmarksPanel);
                    return;
                }
                // Remove is checked before the row itself since it's a child
                // of the row and both appear in the same bubble chain.
                for (target, row, remove) in self.bookmark_chrome {
                    if chain.contains(remove) {
                        self.actions.push(ChromeAction::RemoveBookmark(target.clone()));
                        return;
                    }
                    if chain.contains(row) {
                        self.actions.push(ChromeAction::OpenBookmark(target.clone()));
                        return;
                    }
                }
                if chain.contains(&self.chrome.btn_newtab) {
                    self.actions.push(ChromeAction::NewTab);
                    return;
                }
                if chain.contains(&self.chrome.ab_go) {
                    self.actions.push(ChromeAction::AddressSubmit);
                    return;
                }
                if chain.contains(&self.chrome.ab_back) {
                    self.actions.push(ChromeAction::NavBack);
                    return;
                }
                if chain.contains(&self.chrome.ab_fwd) {
                    self.actions.push(ChromeAction::NavForward);
                    return;
                }
                if chain.contains(&self.chrome.ab_reload) {
                    self.actions.push(ChromeAction::NavReload);
                    return;
                }
                // Close (X) is checked before the chip itself since it's a
                // child of the chip and both appear in the same bubble chain.
                for &(tab_id, chip, close) in self.tab_chrome {
                    if chain.contains(&close) {
                        self.actions.push(ChromeAction::CloseTab(tab_id));
                        return;
                    }
                    if chain.contains(&chip) {
                        self.actions.push(ChromeAction::SwitchTab(tab_id));
                        return;
                    }
                }
            }
            D::PointerDown(_) => {
                if chain.contains(&self.chrome.divider) {
                    self.actions.push(ChromeAction::DividerDown);
                    return;
                }
            }
            D::Input(i) => {
                if chain.contains(&self.chrome.ab_input) {
                    self.actions.push(ChromeAction::AddressInput(i.value.clone()));
                    return;
                }
            }
            D::KeyDown(k) => {
                if chain.contains(&self.chrome.ab_input) {
                    if matches!(k.key, Key::Enter) {
                        self.actions.push(ChromeAction::AddressSubmit);
                    }
                    return;
                }
            }
            _ => {}
        }

        let Some((rt, maps)) = self.guest.as_mut() else { return };
        let Some(eo) = crate::abi::map_dom_event(&event.data) else { return };

        // Bubble host-side: nearest subscribed ancestor in the chain wins.
        let listener = chain.iter().find_map(|nid| {
            maps.rev
                .get(nid)
                .copied()
                .filter(|gid| maps.listeners.contains(&(*gid, eo.kind)))
        });
        let Some(listener) = listener else { return };
        let target = maps.rev.get(&event.target).copied().unwrap_or(listener);

        let preventable = event.cancelable;
        let record_flags = if preventable { 1u16 } else { 0 };
        let call_start = std::time::Instant::now();
        // ABI law: batches submitted during dispatch are applied AFTER the
        // event driver finishes (see handle_ui_event) — mutating the tree
        // mid-dispatch invalidates the driver's node chain (learned the hard
        // way: Remove mid-chain panicked blitz-dom node.rs:1119).
        let _ = doc;
        // `navigate`'s gesture gate (abi.rs): true only for the duration of a
        // genuine click/key dispatch, so a guest can request cross-site
        // navigation only in direct response to user input — never from a
        // frame tick or an async RPC/fetch completion, which don't go through
        // this call at all.
        let is_gesture = matches!(eo.kind, crate::abi::ev::CLICK | crate::abi::ev::KEY_DOWN);
        if is_gesture {
            rt.shared.lock_ok().gesture_active = true;
        }
        let dispatch_result = rt.deliver_event(&eo, record_flags, target, listener);
        if is_gesture {
            rt.shared.lock_ok().gesture_active = false;
        }
        match dispatch_result {
            Ok(flags) => {
                if eo.kind != crate::abi::ev::POINTER_MOVE {
                    crate::logger::log(
                        "event",
                        &format!(
                            "{} -> guest (target={target} listener={listener}) guest_call_us={}",
                            event.name(),
                            call_start.elapsed().as_micros()
                        ),
                    );
                }
                if preventable && flags & 1 != 0 {
                    event_state.prevent_default();
                }
                if flags & 2 != 0 {
                    event_state.stop_propagation();
                }
                if !rt.shared.lock_ok().batches.is_empty() {
                    event.request_redraw = true;
                }
            }
            Err(e) => {
                crate::logger::log("crash", &format!("guest event handler failed: {e:#}"));
                if let Some(why) = crate::abi::guest_fault(&e) {
                    self.actions.push(ChromeAction::GuestFault(why));
                }
            }
        }
    }
}
