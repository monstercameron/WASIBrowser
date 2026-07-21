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
  #shell { flex: 1 1 auto; min-height: 0; display: flex; background: #17181a; }
  #shell.layout-top { flex-direction: column; }
  #shell.layout-side { flex-direction: row; }
  /* Tab strip sits a shade darker than the toolbar below it — the active
     chip's top accent + matching body color is what visually welds it to
     the content, so the strip itself can stay quiet. */
  #tabstrip { flex: 0 0 auto; display: flex; gap: 4px; background: #101113;
              overflow: auto; align-items: center; }
  #shell.layout-top #tabstrip { flex-direction: row; padding: 8px 8px 0; }
  #shell.layout-side #tabstrip { flex-direction: column; align-items: stretch;
                                  width: 200px; padding: 8px 0 8px 8px; }
  .tab-chip { display: flex; align-items: center; gap: 6px; font-size: 12.5px;
              color: #9a9fa6; background: transparent; border: none;
              border-top: 2px solid transparent; border-radius: 6px 6px 0 0;
              padding: 7px 10px 6px; cursor: pointer; white-space: nowrap;
              max-width: 220px; overflow: hidden; transition: color .1s ease; }
  #shell.layout-side .tab-chip { border-top: none; border-left: 2px solid transparent;
                                  border-radius: 6px; }
  .tab-chip:hover { color: #e8e8e8; background: rgba(255,255,255,0.04); }
  .tab-chip.active { background: #26282c; border-top-color: #5b9dff; color: #f2f3f5; }
  #shell.layout-side .tab-chip.active { border-top-color: transparent; border-left-color: #5b9dff; }
  .tab-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tab-close { opacity: 0; width: 15px; height: 15px; display: flex; align-items: center;
               justify-content: center; border-radius: 4px; font-size: 12px; line-height: 1; }
  .tab-chip:hover .tab-close, .tab-chip.active .tab-close { opacity: 0.6; }
  .tab-close:hover { opacity: 1 !important; background: rgba(255,255,255,0.16); }
  #btn-newtab { flex: 0 0 auto; margin: 0 4px 6px; }
  #maincol { flex: 1 1 auto; min-width: 0; min-height: 0; display: flex; flex-direction: column;
             background: #26282c; }
  #toolbar { flex: 0 0 auto; display: flex; align-items: center;
             justify-content: space-between; padding: 7px 12px; gap: 14px;
             border-bottom: 1px solid #313438; }
  #addressbar { flex: 0 1 auto; width: 600px; min-width: 0; display: flex; gap: 4px;
                align-items: center; background: #1a1b1d; border: 1px solid #3a3d41;
                border-radius: 8px; padding: 3px; transition: border-color .12s ease,
                box-shadow .12s ease; }
  #addressbar:focus-within { border-color: #5b9dff; box-shadow: 0 0 0 3px rgba(91,157,255,0.18); }
  #ab-input { flex: 1 1 auto; min-width: 0; font-size: 12.5px; padding: 5px 8px;
              border: none; background: transparent;
              color: #e8e8e8; font-family: Consolas, 'Cascadia Mono', monospace; }
  #ab-input::placeholder { color: #6b7076; }
  .tb-btn { font-size: 12px; color: #c9cdd3; background: #313438;
            border: 1px solid transparent; border-radius: 6px; padding: 5px 11px;
            cursor: pointer; font-family: 'Segoe UI', sans-serif; }
  .tb-btn:hover { background: #3a3e44; color: #ffffff; }
  .tb-icon { min-width: 30px; display: flex; align-items: center; justify-content: center;
             padding: 5px 7px; font-size: 13px; background: transparent; border: none;
             border-radius: 6px; color: #9fa4ab; }
  .tb-icon:hover { background: rgba(255,255,255,0.08); color: #ffffff; }
  #toolbar-actions { display: flex; align-items: center; gap: 4px; flex: 0 0 auto; }
  #tb-menu, #tb-bookmarks { width: 30px; height: 28px; display: flex; flex-direction: column;
             align-items: center; justify-content: center; gap: 3px; background: transparent;
             border: none; border-radius: 6px; cursor: pointer; flex: 0 0 auto; }
  #tb-menu:hover, #tb-bookmarks:hover { background: rgba(255,255,255,0.08); }
  .kebab-dot { width: 3px; height: 3px; border-radius: 50%; background: #9fa4ab; }
  #tb-menu:hover .kebab-dot { background: #e8e8e8; }
  .hbar { width: 14px; height: 2px; border-radius: 1px; background: #9fa4ab; }
  #tb-bookmarks:hover .hbar { background: #e8e8e8; }
  #ab-bookmark { font-weight: 700; color: #9fa4ab; }
  /* Normal-flow expanding row (same reasoning as #toolbar-menu below): a
     floating overlay via position:absolute is unreliable inside nested flex
     containers in this engine's layout (Taffy). */
  #bookmarks-panel { display: none; flex: 0 0 auto; flex-direction: column;
                     background: #202225; border-bottom: 1px solid #3a3d41; }
  .bm-row { display: flex; align-items: center; gap: 10px; padding: 8px 14px;
            border-bottom: 1px solid #2a2c30; cursor: pointer; }
  .bm-row:hover { background: #26282c; }
  .bm-title { font-size: 12.5px; color: #e8e8e8; overflow: hidden; text-overflow: ellipsis;
              white-space: nowrap; flex: 1 1 auto; min-width: 0; }
  .bm-url { font-size: 11px; color: #6b7076; font-family: Consolas, 'Cascadia Mono', monospace;
            flex: 0 0 auto; }
  .bm-remove { flex: 0 0 auto; font-size: 11px; color: #9fa4ab; padding: 3px 8px;
               border-radius: 5px; background: transparent; border: none; cursor: pointer;
               font-family: 'Segoe UI', sans-serif; }
  .bm-remove:hover { background: rgba(255,138,128,0.15); color: #ff8a80; }
  .bm-empty { padding: 14px; font-size: 12.5px; color: #6b7076; text-align: center; }
  /* Normal-flow expanding row (like #console's own show/hide), not a
     floating overlay — position:absolute inside nested flex containers is
     unreliable in this engine's layout (Taffy), so the menu pushes #app
     down instead of floating above it. */
  #toolbar-menu { display: none; flex: 0 0 auto; flex-direction: row; flex-wrap: wrap;
                  gap: 6px; padding: 8px 12px; background: #202225;
                  border-bottom: 1px solid #3a3d41; }
  .menu-item { background: #313438; border: 1px solid transparent; color: #c9cdd3;
               font-size: 12px; padding: 5px 11px; border-radius: 6px; cursor: pointer;
               font-family: 'Segoe UI', sans-serif; }
  .menu-item:hover { background: #3a3e44; color: #ffffff; }
  #app { flex: 1 1 auto; min-height: 0; overflow-y: auto; position: relative; }
  /* display:flex (not plain block) so the guest's top-level div is a flex
     ITEM here and gets Taffy's flex cross-axis stretch sizing for free —
     plain block width:100% on a guest's own root div was observed to be
     silently ignored (computed via shrink-to-fit instead) whenever a
     display:grid descendant existed anywhere in that guest's subtree. */
  .mount { display: flex; flex-direction: column; min-height: 100%; }
  /* No element styling inside .mount: guest apps own their look entirely. */
  #divider { flex: 0 0 auto; height: 6px; background: #26282c;
             border-top: 1px solid #3a3d41; cursor: row-resize; }
  #divider:hover { background: #5b9dff; }
  /* Fixed height (host-adjusted by dragging #divider): the console NEVER
     grows with message count — #console-log overflow-scrolls instead. */
  #console { flex: 0 0 auto; height: 240px; min-height: 0; display: flex;
             flex-direction: column; background: #161719; }
  #console-title { flex: 0 0 auto; padding: 6px 12px; font-size: 12px;
                   color: #9a9fa6; background: #222427;
                   border-bottom: 1px solid #3a3d41;
                   font-family: Consolas, monospace; }
  #console-log { flex: 1 1 auto; min-height: 0; overflow-y: auto; display: flex;
                 flex-direction: column-reverse; padding: 8px 12px;
                 font-family: Consolas, 'Cascadia Mono', monospace;
                 font-size: 13px; line-height: 1.5; }
  .ln { white-space: pre-wrap; }
  .out { color: #d7d9dd; }
  .err { color: #ff8a80; }
  .host { color: #82aaff; }
</style></head>
<body>
  <div id="shell" class="layout-top">
    <div id="tabstrip">
      <button id="btn-newtab" class="tb-icon" title="New tab">+</button>
    </div>
    <div id="maincol">
      <header id="toolbar">
        <div id="addressbar">
          <button id="ab-back" class="tb-icon" title="Back">&larr;</button>
          <button id="ab-fwd" class="tb-icon" title="Forward">&rarr;</button>
          <button id="ab-reload" class="tb-btn" title="Reload">reload</button>
          <input id="ab-input" type="text" placeholder="web://name or path.wasm" value="" />
          <button id="ab-go" class="tb-btn">go</button>
          <button id="ab-bookmark" class="tb-icon" title="Bookmark this page">*</button>
        </div>
        <div id="toolbar-actions">
          <button id="tb-bookmarks" class="tb-icon" title="Bookmarks">
            <span class="hbar"></span><span class="hbar"></span><span class="hbar"></span>
          </button>
          <button id="tb-menu" title="More">
            <span class="kebab-dot"></span><span class="kebab-dot"></span><span class="kebab-dot"></span>
          </button>
        </div>
      </header>
      <div id="toolbar-menu">
        <button id="tb-tabpos" class="menu-item">tabs: top</button>
        <button id="tb-theme" class="menu-item">dark mode</button>
        <button id="tb-clear" class="menu-item">clear console</button>
        <button id="tb-console" class="menu-item">hide console</button>
      </div>
      <div id="bookmarks-panel">
        <div id="bookmarks-empty" class="bm-empty">No bookmarks yet — click the * next to the address bar to save this page.</div>
      </div>
      <main id="app"></main>
      <div id="divider"></div>
      <footer id="console">
        <div id="console-title">console — drag the bar above to resize</div>
        <div id="console-log"></div>
      </footer>
    </div>
  </div>
</body></html>
"#;

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
}

impl GwbDocument {
    pub fn new(engine: Engine, proxy: BlitzShellProxy, manifest_root: String) -> Self {
        let config = DocumentConfig {
            // set_inner_html needs a parser provider at mutation time.
            html_parser_provider: Some(Arc::new(HtmlProvider)),
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
        let mount_node = {
            let mut m = self.doc.mutate();
            let n = m.create_element(qn("div"), Vec::new());
            m.set_attribute(n, attr_qn("class"), "mount");
            m.set_style_property(n, "display", "none");
            m.append_children(self.chrome.app_root, &[n]);
            n
        };
        // Guest id 1 = the mount root (spec: the only host node a guest can
        // address), re-seeded fresh for every tab/navigation.
        let mut maps = crate::abi::NodeMaps::default();
        maps.fwd.insert(1, mount_node);
        maps.rev.insert(mount_node, 1);
        self.tabs.push(TabState {
            id,
            guest: None,
            maps,
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

    fn create_tab_chip(&mut self, title: &str) -> (usize, usize, usize) {
        let mut m = self.doc.mutate();
        let chip = m.create_element(qn("div"), Vec::new());
        m.set_attribute(chip, attr_qn("class"), "tab-chip");
        let label = m.create_element(qn("span"), Vec::new());
        m.set_attribute(label, attr_qn("class"), "tab-label");
        m.set_inner_html(label, &escape_html(&truncate_title(title)));
        let close = m.create_element(qn("span"), Vec::new());
        m.set_attribute(close, attr_qn("class"), "tab-close");
        m.set_inner_html(close, "&times;");
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
            m.set_style_property(old.mount_node, "display", "none");
            m.set_attribute(old.chip_node, attr_qn("class"), "tab-chip");
        }
        let new = &self.tabs[idx];
        m.set_style_property(new.mount_node, "display", "flex");
        m.set_attribute(new.chip_node, attr_qn("class"), "tab-chip active");
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
        m.set_attribute(self.chrome.ab_input, attr_qn("value"), "");
        drop(m);
        self.doc.set_focus_to(self.chrome.ab_input);
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
        let (mount, chip) = (self.tabs[idx].mount_node, self.tabs[idx].chip_node);
        {
            let mut m = self.doc.mutate();
            m.remove_and_drop_node(mount);
            m.remove_and_drop_node(chip);
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
        m.set_attribute(self.chrome.ab_input, attr_qn("value"), &text);
        drop(m);
        self.dirty = true;
        self.refresh_bookmark_star();
    }

    fn update_tab_label(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get(idx) else { return };
        let label_node = tab.label_node;
        let title = tab.title.clone();
        let mut m = self.doc.mutate();
        m.set_inner_html(label_node, &escape_html(&truncate_title(&title)));
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
        {
            let mut m = self.doc.mutate();
            m.remove_and_drop_all_children(mount_node);
        }
        let mut maps = crate::abi::NodeMaps::default();
        maps.fwd.insert(1, mount_node);
        maps.rev.insert(mount_node, 1);
        self.tabs[tab_idx].maps = maps;

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
        match crate::abi::try_load(&self.engine, &resolved.bundle_path, tab_id, self.proxy.clone(), resolved.services, resolved.links) {
            Ok(Some(mut rt)) => {
                if let Some(id) = self.window_id {
                    rt.shared.lock().unwrap().window_id = Some(id);
                }
                if let Err(e) = rt.start(w, h, scale, self.theme_dark as u32) {
                    crate::logger::log("crash", &format!("gwb_start failed: {e:#}"));
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
                    m.set_inner_html(
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
                    m.set_attribute(
                        self.chrome.shell,
                        attr_qn("class"),
                        if side { "layout-side" } else { "layout-top" },
                    );
                    m.set_inner_html(self.chrome.tb_tabpos, if side { "tabs: side" } else { "tabs: top" });
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

    // ---- bookmarks: toggle / list / persist ------------------------------

    fn bookmarks_path() -> &'static str {
        "bookmarks.json"
    }

    fn load_bookmarks(&mut self) {
        let text = match std::fs::read_to_string(Self::bookmarks_path()) {
            Ok(t) => t,
            Err(_) => return, // no file yet: fine, starts empty
        };
        let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&text) else {
            crate::logger::log("ui", "bookmarks.json is malformed; ignoring");
            return;
        };
        for item in items {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if target.is_empty() || self.is_bookmarked(&target) {
                continue;
            }
            let (row_node, remove_node) = self.create_bookmark_row(&title, &target);
            self.bookmarks.push(Bookmark { title, target, row_node, remove_node });
        }
        self.refresh_bookmarks_empty_state();
        crate::logger::log("ui", &format!("loaded {} bookmark(s)", self.bookmarks.len()));
    }

    fn save_bookmarks(&self) {
        let items: Vec<serde_json::Value> = self
            .bookmarks
            .iter()
            .map(|b| serde_json::json!({ "title": b.title, "target": b.target }))
            .collect();
        let text = match serde_json::to_string_pretty(&serde_json::Value::Array(items)) {
            Ok(t) => t,
            Err(e) => {
                crate::logger::log("ui", &format!("bookmarks serialize failed: {e}"));
                return;
            }
        };
        if let Err(e) = std::fs::write(Self::bookmarks_path(), text) {
            crate::logger::log("ui", &format!("bookmarks.json write failed: {e}"));
        }
    }

    fn is_bookmarked(&self, target: &str) -> bool {
        self.bookmarks.iter().any(|b| b.target == target)
    }

    /// Toggle the bookmark for the active tab's current target (mirrors a
    /// browser's address-bar star: click again to remove).
    fn toggle_bookmark(&mut self) {
        let Some(tab) = self.tabs.get(self.active_idx) else { return };
        if tab.target.is_empty() {
            return;
        }
        let target = tab.target.clone();
        let title = tab.title.clone();
        if let Some(pos) = self.bookmarks.iter().position(|b| b.target == target) {
            let bm = self.bookmarks.remove(pos);
            let mut m = self.doc.mutate();
            m.remove_and_drop_node(bm.row_node);
            drop(m);
            crate::logger::log("ui", &format!("bookmark removed: {target}"));
        } else {
            let (row_node, remove_node) = self.create_bookmark_row(&title, &target);
            self.bookmarks.push(Bookmark { title, target: target.clone(), row_node, remove_node });
            crate::logger::log("ui", &format!("bookmark added: {target}"));
        }
        self.refresh_bookmarks_empty_state();
        self.refresh_bookmark_star();
        self.save_bookmarks();
        self.dirty = true;
    }

    fn remove_bookmark(&mut self, target: &str) {
        let Some(pos) = self.bookmarks.iter().position(|b| b.target == target) else { return };
        let bm = self.bookmarks.remove(pos);
        let mut m = self.doc.mutate();
        m.remove_and_drop_node(bm.row_node);
        drop(m);
        crate::logger::log("ui", &format!("bookmark removed: {target}"));
        self.refresh_bookmarks_empty_state();
        self.refresh_bookmark_star();
        self.save_bookmarks();
        self.dirty = true;
    }

    fn open_bookmark(&mut self, target: String) {
        self.navigate_active(&target);
        self.set_bookmarks_panel_open(false);
    }

    /// Build one row (title, url, remove button) and insert it just before
    /// the panel's permanent "no bookmarks yet" node — same pattern as tab
    /// chips inserting before `#btn-newtab`.
    fn create_bookmark_row(&mut self, title: &str, target: &str) -> (usize, usize) {
        let mut m = self.doc.mutate();
        let row = m.create_element(qn("div"), Vec::new());
        m.set_attribute(row, attr_qn("class"), "bm-row");
        let title_el = m.create_element(qn("span"), Vec::new());
        m.set_attribute(title_el, attr_qn("class"), "bm-title");
        m.set_inner_html(title_el, &escape_html(&truncate_title(title)));
        let url_el = m.create_element(qn("span"), Vec::new());
        m.set_attribute(url_el, attr_qn("class"), "bm-url");
        m.set_inner_html(url_el, &escape_html(target));
        let remove = m.create_element(qn("button"), Vec::new());
        m.set_attribute(remove, attr_qn("class"), "bm-remove");
        m.set_inner_html(remove, "remove");
        m.append_children(row, &[title_el, url_el, remove]);
        m.insert_nodes_before(self.chrome.bookmarks_empty, &[row]);
        drop(m);
        (row, remove)
    }

    fn refresh_bookmarks_empty_state(&mut self) {
        let show_empty = self.bookmarks.is_empty();
        let mut m = self.doc.mutate();
        m.set_style_property(
            self.chrome.bookmarks_empty,
            "display",
            if show_empty { "block" } else { "none" },
        );
        drop(m);
        self.dirty = true;
    }

    /// Sync the address-bar star to whether the active tab's target is
    /// bookmarked — called on navigate/activate so it never goes stale.
    fn refresh_bookmark_star(&mut self) {
        let target = self.tabs.get(self.active_idx).map(|t| t.target.clone()).unwrap_or_default();
        let active = !target.is_empty() && self.is_bookmarked(&target);
        let mut m = self.doc.mutate();
        // Two layered engine quirks ruled out simpler approaches: (1) toggling
        // a CLASS here (like tab chips use) hits a real bug where a class
        // attribute already present from the static SHELL_HTML parse doesn't
        // get replaced by a runtime set_attribute of the same name — it
        // appends a second "class" attr instead, and style matching only
        // sees the first (stale) one (see attr_qn's doc comment — fixed for
        // class/value elsewhere, but simplest to just avoid class-matching
        // here). (2) The specialized set_style_property mutator marks the
        // node itself dirty but never propagates that up through ancestors
        // (unlike set_attribute, which explicitly calls mark_ancestors_dirty)
        // — Stylo's restyle traversal starts at the root and only descends
        // into subtrees an ancestor flagged dirty, so a color-only change
        // through that API is silently never repainted. Setting `style` as a
        // plain HTML attribute goes through set_attribute's code path
        // instead, which does propagate correctly.
        m.set_attribute(
            self.chrome.ab_bookmark,
            attr_qn("style"),
            &format!("color: {}", if active { "#ffb400" } else { "#9fa4ab" }),
        );
        drop(m);
        self.dirty = true;
    }

    fn set_bookmarks_panel_open(&mut self, open: bool) {
        if self.bookmarks_panel_open == open {
            return;
        }
        self.bookmarks_panel_open = open;
        let mut m = self.doc.mutate();
        m.set_style_property(
            self.chrome.bookmarks_panel,
            "display",
            if open { "flex" } else { "none" },
        );
        drop(m);
        self.dirty = true;
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
        m.set_inner_html(
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
                rt.shared.lock().unwrap().window_id = Some(id);
            }
        }
    }

    /// Whether the active tab's guest has an undelivered request_frame.
    pub fn frame_pending(&self) -> bool {
        self.tabs
            .get(self.active_idx)
            .and_then(|t| t.guest.as_ref())
            .is_some_and(|rt| rt.shared.lock().unwrap().frame_requested)
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
        self.script_dispatch(nid, data);
        // The pointer-down default (focus-on-click) didn't run; emulate it so
        // `click #input` + `type ...` composes naturally (caret at end). The
        // synthetic Focus event keeps guest onFocus handlers honest too.
        if is_input {
            if let Some(nid) = self.resolve(selector) {
                self.doc.set_focus_to(nid);
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
        self.doc.set_focus_to(nid);
        self.script_dispatch(
            nid,
            blitz_traits::events::DomEventData::Focus(blitz_traits::events::BlitzFocusEvent),
        );
        true
    }

    /// Build a `ShellEventHandler` scoped to the active tab. Computes the
    /// tab-chrome id list BEFORE taking the active tab's mutable borrow (both
    /// would otherwise borrow `self.tabs` at once).
    fn dispatch_with_shell_handler<F: FnOnce(&mut EventDriver<'_, ShellEventHandler<'_>>)>(&mut self, f: F) {
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
    fn script_dispatch(&mut self, nid: usize, data: blitz_traits::events::DomEventData) {
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
    fn script_pointer(
        &mut self,
        selector: &str,
        wrap: fn(blitz_traits::events::BlitzPointerEvent) -> blitz_traits::events::DomEventData,
    ) -> bool {
        use blitz_traits::events::DomEventData as D;
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
        let cap = rt.shared.lock().unwrap().event_region.1 as usize;
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
            Err(e) => crate::logger::log("crash", &format!("net delivery failed: {e:#}")),
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
        let cap = rt.shared.lock().unwrap().event_region.1 as usize;
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
            Err(e) => crate::logger::log("crash", &format!("rpc delivery failed: {e:#}")),
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
            let Some(rt) = self.tabs[idx].guest.as_mut() else { continue };
            if let Err(e) = rt.deliver_event(&eo, 0, *gid, *gid) {
                crate::logger::log("crash", &format!("observation delivery failed: {e:#}"));
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

        // Animation frames: deliver gwb_frame to the ACTIVE tab only if it
        // requested one — background tabs don't animate (legacy Go spine's
        // Background/Frozen intent), and catch up immediately once switched to.
        let active = self.active_idx;
        if let Some(tab) = self.tabs.get_mut(active) {
            if let Some(rt) = tab.guest.as_mut() {
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
                    if self.apply_active_guest_batches() > 0 {
                        dirty = true;
                    }
                }
            }
        }

        if self.check_observations() {
            dirty = true;
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
            rt.shared.lock().unwrap().gesture_active = true;
        }
        let dispatch_result = rt.deliver_event(&eo, record_flags, target, listener);
        if is_gesture {
            rt.shared.lock().unwrap().gesture_active = false;
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
                if !rt.shared.lock().unwrap().batches.is_empty() {
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
