//! GWB ABI v0 — host implementation (see docs/ABI.md).
//!
//! v0 scope: batch decode (fixed 16-byte records + string heap), well-known +
//! dynamic atoms, Listen subscriptions, click delivery into the guest event
//! region. Hacky corners are marked; the contract shape matches the spec.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use blitz_dom::{BaseDocument, DocumentMutator, LocalName, QualName, ns};
use blitz_shell::BlitzShellProxy;
use wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::net::{self, Inflight, Origin};
use crate::ui::{ConsoleMsg, Source};

pub const ABI_MAJOR: u32 = 1;

// ---------------------------------------------------------------- resource limits

/// Epoch tick length; `main` runs a thread that bumps the engine epoch this often.
pub const EPOCH_TICK: std::time::Duration = std::time::Duration::from_millis(10);
/// CPU budget for module init + `gwb_start` (Go's runtime init is not tiny).
const START_BUDGET_TICKS: u64 = 1000; // 10 s
/// CPU budget for any single event / frame / completion delivery. A guest that
/// overruns it traps; the host then unloads it (see ui.rs `kill_guest`).
const CALL_BUDGET_TICKS: u64 = 200; // 2 s
/// Hard caps per guest store.
const MAX_GUEST_MEMORY: usize = 512 * 1024 * 1024;
const MAX_TABLE_ELEMENTS: usize = 1_000_000;

/// Batch decode caps (checked before allocating anything sized by the guest).
const MAX_OPS_PER_BATCH: usize = 1 << 20;
/// Ops queued for the UI thread across all not-yet-applied batches.
const MAX_PENDING_OPS: usize = 4 << 20;
/// First guest-definable atom (0..1024 are the spec's well-known table).
pub const FIRST_DYNAMIC_ATOM: u32 = 1024;
/// Atom table ceiling. Atoms index a flat Vec, so an unbounded id would let a
/// single DefineAtom make the host allocate gigabytes.
pub const MAX_ATOMS: u32 = 1 << 16;
/// Largest session token the host will hold for a guest.
const MAX_SESSION_TOKEN: usize = 16 * 1024;

/// The process-wide engine: epoch interruption on, so runaway guests trap
/// instead of freezing the UI thread.
pub fn new_engine() -> Result<Engine> {
    let mut cfg = Config::new();
    cfg.epoch_interruption(true);
    Ok(Engine::new(&cfg)?)
}

/// If a guest call failed because the guest itself faulted (CPU budget,
/// out-of-bounds, unreachable, stack overflow...), say why in user terms.
/// Host-side errors return None.
pub fn guest_fault(e: &anyhow::Error) -> Option<String> {
    let trap = e
        .downcast_ref::<wasmtime::Trap>()
        .or_else(|| e.chain().find_map(|c| c.downcast_ref::<wasmtime::Trap>()))?;
    Some(match trap {
        wasmtime::Trap::Interrupt => "it ran too long without yielding (CPU limit)".to_string(),
        other => format!("it crashed ({other})"),
    })
}

/// Bump `engine`'s epoch every [`EPOCH_TICK`], forever (daemon thread).
pub fn spawn_epoch_ticker(engine: Engine) {
    std::thread::Builder::new()
        .name("wasm-epoch".into())
        .spawn(move || loop {
            std::thread::sleep(EPOCH_TICK);
            engine.increment_epoch();
        })
        .expect("spawn epoch ticker");
}

/// Memory/table/instance caps applied to every guest store.
pub fn guest_store_limits() -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(MAX_GUEST_MEMORY)
        .table_elements(MAX_TABLE_ELEMENTS)
        .instances(4)
        .memories(2)
        .tables(8)
        .trap_on_grow_failure(false)
        .build()
}

// ---------------------------------------------------------------- Blitz workaround

/// Upstream Blitz (f550417) gives every new element Stylo data inside
/// `create_element`. An attribute change or focus on such a node *before its
/// first style pass* records a Stylo snapshot, and the next traversal's
/// invalidator then unwraps the node's (missing) primary styles and panics
/// (`stylo data.rs: ElementStyles::primary`). The `gwb-perf` fork avoided it by
/// initializing that data lazily. We get the same state host-side: right
/// before any snapshotting mutation, drop Stylo data from a node that has
/// never been styled. The traversal re-creates it, exactly as the lazy patch
/// does. Styled nodes are untouched.
pub fn forget_unstyled_data(doc: &mut BaseDocument, id: usize) {
    let Some(node) = doc.get_node_mut(id) else { return };
    let unstyled = node.stylo_element_data.get().is_some_and(|d| !d.has_styles());
    if unstyled {
        node.stylo_element_data.take();
    }
}

/// Upstream Blitz keeps `focus_node_id` / `hover_node_id` / `active_node_id`
/// pointing at a node after it's dropped; the next focus/hover/press change
/// then snapshots the freed slot and panics (`document.rs snapshot_node:
/// invalid key`). Before dropping `root`'s subtree, release those states
/// while the nodes still exist.
fn release_states_within(doc: &mut BaseDocument, root: usize, include_root: bool) {
    let inside = |doc: &BaseDocument, id: usize| (include_root || id != root) && is_within(doc, id, root);
    if doc.get_focussed_node_id().is_some_and(|f| inside(doc, f)) {
        doc.clear_focus();
    }
    if doc.get_hover_node_id().is_some_and(|h| inside(doc, h)) {
        // :active follows the hovered node; release it with the hover.
        doc.unactive_node();
        doc.clear_hover();
    }
}

/// `set_attribute` / `clear_attribute` / node dropping with the guards above.
/// Use these everywhere instead of the raw mutator methods.
pub trait SafeMutate {
    fn set_attr(&mut self, id: usize, name: QualName, value: &str);
    fn clear_attr(&mut self, id: usize, name: QualName);
    /// `remove_and_drop_node`, releasing focus/hover/active inside it first.
    fn drop_node(&mut self, id: usize);
    /// `remove_and_drop_all_children`, releasing focus/hover/active first.
    fn drop_children(&mut self, id: usize);
    /// `set_inner_html` (which drops the old children) with the same guard.
    fn set_html(&mut self, id: usize, html: &str);
}

impl SafeMutate for DocumentMutator<'_> {
    fn set_attr(&mut self, id: usize, name: QualName, value: &str) {
        forget_unstyled_data(self.doc, id);
        DocumentMutator::set_attribute(self, id, name, value);
    }
    fn clear_attr(&mut self, id: usize, name: QualName) {
        forget_unstyled_data(self.doc, id);
        DocumentMutator::clear_attribute(self, id, name);
    }
    fn drop_node(&mut self, id: usize) {
        if self.doc.get_node(id).is_none() {
            return;
        }
        release_states_within(self.doc, id, true);
        self.remove_and_drop_node(id);
    }
    fn drop_children(&mut self, id: usize) {
        if self.doc.get_node(id).is_none() {
            return;
        }
        release_states_within(self.doc, id, false);
        self.remove_and_drop_all_children(id);
    }
    fn set_html(&mut self, id: usize, html: &str) {
        self.drop_children(id);
        self.set_inner_html(id, html);
    }
}

/// `set_focus_to` with the same guards (focus snapshots the node, and blurs
/// the previous one).
pub fn focus(doc: &mut BaseDocument, id: usize) {
    if doc.get_node(id).is_none() {
        return;
    }
    forget_unstyled_data(doc, id);
    doc.set_focus_to(id);
}

/// Locking that survives a poisoned mutex. Host threads (fetch, rpc, UI) share
/// guest state; a panic in one must not turn every later `lock()` into a
/// second panic on the UI thread. The data here stays consistent across a
/// panic (plain fields, no multi-step invariants), so recovering is safe.
pub trait LockExt<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// RPC method names become one URL path segment: `[A-Za-z0-9_-]{1,64}`.
fn valid_method(m: &str) -> bool {
    (1..=64).contains(&m.len()) && m.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Bounds-checked view of guest memory (no `ptr + len` overflow).
fn guest_bytes(data: &[u8], ptr: u32, len: u32) -> Option<&[u8]> {
    let start = ptr as usize;
    let end = start.checked_add(len as usize)?;
    data.get(start..end)
}

/// Load the persisted app-scoped ed25519 key (32-byte seed) or create one.
/// Local-dev stand-in for the §1 per-app derived identity — a stable key so the
/// server sees a consistent caller. Returns (signing key, base64 pubkey).
fn load_or_create_app_key() -> (Arc<ed25519_dalek::SigningKey>, String) {
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;
    let path = std::path::Path::new(".gwb-app-key.bin");
    let seed: [u8; 32] = match std::fs::read(path) {
        Ok(b) if b.len() == 32 => b.try_into().unwrap(),
        _ => {
            use rand::RngCore;
            let mut s = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut s);
            let _ = std::fs::write(path, s);
            // Restrict to owner-only where the platform supports it. NOTE: this
            // is a DEV seed (gitignored); a real deployment keeps the app key in
            // the OS keychain / TPM, never a plaintext file (spec §10.5).
            restrict_key_file(path);
            crate::logger::log("rpc", "generated app-scoped key (.gwb-app-key.bin, owner-only)");
            s
        }
    };
    let key = SigningKey::from_bytes(&seed);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    (Arc::new(key), pub_b64)
}

/// Best-effort owner-only permissions on the key seed. Unix sets 0600; on
/// Windows the file inherits the user-profile ACL (already user-scoped) — the
/// durable answer there is the OS keychain, out of scope for this dev seed.
#[cfg(unix)]
fn restrict_key_file(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict_key_file(_path: &std::path::Path) {}

pub mod ev {
    pub const NET_RESULT: u16 = 40;
    /// Host-mediated RPC completion, correlated by req_id. See docs/04-WEB-RPC.md.
    pub const RPC_RESULT: u16 = 41;
    pub const POINTER_DOWN: u16 = 1;
    pub const POINTER_UP: u16 = 2;
    pub const POINTER_MOVE: u16 = 3;
    pub const CLICK: u16 = 4;
    pub const DBLCLICK: u16 = 5;
    pub const POINTER_ENTER: u16 = 6;
    pub const POINTER_LEAVE: u16 = 7;
    pub const WHEEL: u16 = 8;
    pub const KEY_DOWN: u16 = 9;
    pub const KEY_UP: u16 = 10;
    pub const TEXT_INPUT: u16 = 11;
    pub const INPUT: u16 = 12;
    // 13 change, 14 submit: reserved (Blitz does not emit them yet)
    pub const FOCUS: u16 = 15;
    pub const BLUR: u16 = 16;
    pub const SCROLL: u16 = 17;
    pub const WINDOW_RESIZE: u16 = 18;
    pub const THEME_CHANGE: u16 = 19;
    pub const CONTEXT_MENU: u16 = 20;
    pub const POINTER_CANCEL: u16 = 21;
    /// Delivered once to the mount root after the initial batches are applied
    /// (the "document loaded" moment). Payload = {w f32, h f32, scale f32}.
    pub const PAGE_LOAD: u16 = 22;
    pub const OBSERVED_LAYOUT: u16 = 32;
    pub const OBSERVED_VISIBILITY: u16 = 33;
}

/// Observe() `what` bits.
pub mod obs {
    pub const LAYOUT: u32 = 1;
    pub const VISIBILITY: u32 = 2;
}

// ---------------------------------------------------------------- ops

#[derive(Debug, Clone)]
pub enum Op {
    CreateElement { id: u32, tag: u32 },
    CreateText { id: u32, text: String },
    SetAttr { id: u32, name: u32, value: String },
    RemoveAttr { id: u32, name: u32 },
    SetText { id: u32, text: String },
    SetStyle { id: u32, prop: u32, value: String },
    RemoveStyle { id: u32, prop: u32 },
    AppendChild { parent: u32, child: u32 },
    InsertBefore { child: u32, before: u32 },
    Remove { id: u32 },
    ReplaceWith { old: u32, new: u32 },
    Clear { id: u32 },
    SetInnerHtml { id: u32, html: String },
    DefineAtom { atom: u32, name: String },
    Listen { id: u32, kind: u16 },
    Unlisten { id: u32, kind: u16 },
    Observe { id: u32, what: u32 },
    Unobserve { id: u32, what: u32 },
    Focus { id: u32 },
}

// ---------------------------------------------------------------- shared state

/// An interned name: the raw string plus a lazily-built QualName (QualName
/// clones are cheap Arc-ish atom copies; building one hashes — do it once).
pub struct AtomEntry {
    pub name: String,
    qual: Option<QualName>,
}

impl AtomEntry {
    fn new(name: String) -> Self {
        Self { name, qual: None }
    }
    fn qual(&mut self) -> QualName {
        if self.qual.is_none() {
            self.qual = Some(QualName::new(None, ns!(html), LocalName::from(&*self.name)));
        }
        self.qual.clone().unwrap()
    }
}

/// A completed host-side HTTP request, shipped to the UI thread.
pub struct NetResult {
    pub id: u32,
    /// Which tab's guest this belongs to (routing key — see ui.rs delivery).
    pub tab_id: u32,
    pub status: u16,
    pub ok: bool,
    pub body: String,
}

/// A manifest-declared RPC service the app may call (capability). See
/// docs/04-WEB-RPC.md §4. The `service` name in a `rpc_call` indexes this.
#[derive(Clone)]
pub struct ServiceEntry {
    pub endpoint: String,
    pub iface: String,
    /// ed:<server pubkey> from the manifest (verified in the handshake; the
    /// local-dev HTTP ramp does not yet pin it — Phase 2/native transport).
    pub server_key: String,
}

/// A completed host-mediated RPC call, shipped to the UI thread. Mirrors
/// NetResult but carries req_id correlation + an error class (§1 of 04-WEB-RPC).
pub struct RpcResult {
    pub req_id: u32,
    /// Which tab's guest this belongs to (routing key — see ui.rs delivery).
    pub tab_id: u32,
    pub status: u16,
    pub ok: bool,
    pub err_class: u8,
    pub body: String,
}

/// A guest-requested cross-site navigation, shipped to the UI thread once the
/// capability + gesture checks pass (see the `navigate` host import below).
/// The guest never touches tab/window state directly — same law as
/// fetch/rpc_call: it expresses intent, the host acts.
pub struct NavRequest {
    /// The tab that asked (the *requesting* tab — for a new-tab open this is
    /// where the new tab gets inserted next to; for same-tab it's the one
    /// that navigates).
    pub tab_id: u32,
    pub target: String,
    pub new_tab: bool,
}

pub struct Shared {
    pub atoms: Vec<Option<AtomEntry>>,
    pub batches: Vec<Vec<Op>>,
    pub event_region: (u32, u32),
    pub next_fetch_id: u32,
    /// Guest called request_frame; cleared when gwb_frame is delivered.
    pub frame_requested: bool,
    /// Set once the window exists; lets request_frame wake the event loop.
    pub window_id: Option<winit::window::WindowId>,
    /// service name -> capability (from the app manifest, §4). rpc_call resolves
    /// its `service` field here; an unknown name is a capability violation.
    pub services: HashMap<String, ServiceEntry>,
    pub next_rpc_id: u32,
    /// Current user session token (set by the guest via `session_set` after an
    /// auth.login; attached to calls whose flags request it). §3 of 04-WEB-RPC.
    pub session_token: Option<String>,
    /// App-scoped ed25519 signing key (§1/§3): signs every RPC request.
    pub app_key: Arc<ed25519_dalek::SigningKey>,
    /// base64 of the app verifying (public) key — the GWB-App-Key header.
    pub app_key_b64: String,
    /// Manifest-declared cross-site link targets (bare `web://` names, e.g.
    /// "retailer.local") this guest may navigate to/open. Mirrors `services`'
    /// capability model: an undeclared target is rejected pre-navigation,
    /// same as an undeclared RPC service is rejected pre-network.
    pub links: HashSet<String>,
    /// True only while the host is dispatching a genuine pointer/key event to
    /// this guest — set around that one `deliver_event` call in ui.rs. Gates
    /// `navigate`: a guest can request navigation only in direct response to
    /// user input, never from a frame tick or an async RPC/fetch completion
    /// (no drive-by redirects with no user action behind them).
    pub gesture_active: bool,
    /// This tab's id — stamped into every NavRequest so the host routes the
    /// request back to the correct tab (mirrors NetResult/RpcResult's tab_id).
    pub tab_id: u32,
}

/// Well-known atoms 0..1024 (spec appendix; mirrored in sdk/gwb).
fn well_known_atoms() -> Vec<Option<AtomEntry>> {
    let mut v: Vec<Option<AtomEntry>> = Vec::new();
    v.resize_with(1024, || None);
    let entries: &[(usize, &str)] = &[
        // elements: 1..
        (1, "div"), (2, "span"), (3, "p"), (4, "h1"), (5, "h2"), (6, "h3"),
        (7, "button"), (8, "input"), (9, "a"), (10, "img"), (11, "ul"),
        (12, "ol"), (13, "li"), (14, "table"), (15, "tr"), (16, "td"),
        (17, "th"), (18, "form"), (19, "label"), (20, "section"),
        (21, "header"), (22, "footer"), (23, "main"), (24, "nav"),
        (25, "article"), (26, "pre"), (27, "code"), (28, "strong"), (29, "em"),
        (30, "textarea"), (31, "select"), (32, "option"),
        // elements, extension set (2026-07-06): the rest of renderable HTML.
        // Document-level / scripting tags (html/head/meta/script/iframe/...)
        // are deliberately absent — guests own #mount only, and no JS exists.
        (33, "h4"), (34, "h5"), (35, "h6"), (36, "aside"), (37, "address"),
        (38, "blockquote"), (39, "hr"), (40, "br"), (41, "figure"),
        (42, "figcaption"), (43, "small"), (44, "mark"), (45, "kbd"),
        (46, "samp"), (47, "var"), (48, "cite"), (49, "abbr"), (50, "q"),
        (51, "sub"), (52, "sup"), (53, "time"), (54, "data"), (55, "b"),
        (56, "i"), (57, "u"), (58, "s"), (59, "wbr"), (60, "dfn"),
        (61, "del"), (62, "ins"), (63, "dl"), (64, "dt"), (65, "dd"),
        (66, "menu"), (67, "caption"), (68, "thead"), (69, "tbody"),
        (70, "tfoot"), (71, "colgroup"), (72, "col"), (73, "optgroup"),
        (74, "fieldset"), (75, "legend"), (76, "datalist"), (77, "output"),
        (78, "progress"), (79, "meter"), (80, "details"), (81, "summary"),
        (82, "dialog"), (83, "video"), (84, "audio"), (85, "source"),
        (86, "canvas"), (87, "picture"), (88, "track"),
        // attributes: 100..
        (100, "class"), (101, "id"), (102, "style"), (103, "href"),
        (104, "value"), (105, "type"), (106, "placeholder"), (107, "src"),
        (108, "alt"), (109, "title"), (110, "disabled"), (111, "checked"),
        (112, "name"),
        // style properties: 200..
        (200, "display"), (201, "color"), (202, "background"), (203, "width"),
        (204, "height"), (205, "margin"), (206, "padding"), (207, "border"),
        (208, "font-size"), (209, "font-weight"), (210, "gap"),
        (211, "flex-direction"), (212, "align-items"), (213, "justify-content"),
        (214, "border-radius"), (215, "cursor"), (216, "text-align"),
    ];
    for &(i, s) in entries {
        v[i] = Some(AtomEntry::new(s.to_string()));
    }
    v
}

// ---------------------------------------------------------------- batch decode

const MAGIC: u32 = u32::from_le_bytes(*b"GWB1");
const NO_STR: u32 = 0xFFFF_FFFF;

pub(crate) fn decode_batch(bytes: &[u8]) -> Result<Vec<Op>> {
    if bytes.len() < 16 {
        bail!("batch too short");
    }
    let u32_at = |off: usize| u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
    if u32_at(0) != MAGIC {
        bail!("bad magic");
    }
    let op_count = u32_at(4) as usize;
    let heap_off = u32_at(8) as usize;
    let heap_len = u32_at(12) as usize;
    if op_count > MAX_OPS_PER_BATCH {
        bail!("batch has {op_count} ops (limit {MAX_OPS_PER_BATCH})");
    }
    let ops_fit = op_count
        .checked_mul(16)
        .and_then(|n| n.checked_add(16))
        .is_some_and(|end| end <= bytes.len());
    if !ops_fit {
        bail!("batch bounds");
    }
    let heap = heap_off
        .checked_add(heap_len)
        .and_then(|end| bytes.get(heap_off..end))
        .ok_or_else(|| anyhow!("batch bounds"))?;

    let get_str = |d: u32| -> Result<String> {
        if d == NO_STR {
            return Ok(String::new());
        }
        let off = d as usize;
        let len_bytes = off
            .checked_add(4)
            .and_then(|end| heap.get(off..end))
            .ok_or_else(|| anyhow!("str offset out of range"))?;
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        let s = (off + 4)
            .checked_add(len)
            .and_then(|end| heap.get(off + 4..end))
            .ok_or_else(|| anyhow!("str out of range"))?;
        Ok(String::from_utf8_lossy(s).into_owned())
    };

    let mut ops = Vec::with_capacity(op_count);
    for i in 0..op_count {
        let r = 16 + i * 16;
        let code = bytes[r];
        let a = u16::from_le_bytes(bytes[r + 2..r + 4].try_into().unwrap());
        let b = u32_at(r + 4);
        let c = u32_at(r + 8);
        let d = u32_at(r + 12);
        let op = match code {
            1 => Op::CreateElement { id: b, tag: c },
            2 => Op::CreateText { id: b, text: get_str(d)? },
            3 => Op::SetAttr { id: b, name: c, value: get_str(d)? },
            4 => Op::RemoveAttr { id: b, name: c },
            5 => Op::SetText { id: b, text: get_str(d)? },
            6 => Op::SetStyle { id: b, prop: c, value: get_str(d)? },
            7 => Op::RemoveStyle { id: b, prop: c },
            8 => Op::AppendChild { parent: b, child: c },
            9 => Op::InsertBefore { child: c, before: d },
            10 => Op::Remove { id: b },
            11 => Op::ReplaceWith { old: b, new: c },
            12 => Op::Clear { id: b },
            13 => Op::SetInnerHtml { id: b, html: get_str(d)? },
            14 => {
                if !(FIRST_DYNAMIC_ATOM..MAX_ATOMS).contains(&b) {
                    bail!(
                        "DefineAtom {b} outside the dynamic range {FIRST_DYNAMIC_ATOM}..{MAX_ATOMS} at index {i}"
                    );
                }
                Op::DefineAtom { atom: b, name: get_str(d)? }
            }
            15 => Op::Listen { id: b, kind: a },
            16 => Op::Unlisten { id: b, kind: a },
            17 => Op::Observe { id: b, what: c },
            18 => Op::Unobserve { id: b, what: c },
            19 => Op::Focus { id: b },
            other => bail!("unsupported op code {other} at index {i}"),
        };
        ops.push(op);
    }
    Ok(ops)
}

// ---------------------------------------------------------------- node maps + apply

/// Tags a guest may never create, directly or via innerHTML. They are
/// document-level or resource-loading elements that would act outside the
/// guest's mount (window title, base URL, linked sheets, frames) or only make
/// sense with a script engine. `style` is handled separately (scoped).
const DENIED_TAGS: &[&str] = &[
    "script", "noscript", "iframe", "frame", "frameset", "object", "embed", "applet", "portal",
    "link", "base", "meta", "title", "html", "head", "body", "template", "slot",
];
/// The inert element a guest `<style>` becomes. It never turns into a live
/// stylesheet; its text is scoped into a host-owned sheet (css_scope.rs).
const GUEST_STYLE_TAG: &str = "gwb-style";
/// innerHTML import limits (node count, nesting depth).
const MAX_INNER_HTML_NODES: usize = 100_000;
const MAX_INNER_HTML_DEPTH: usize = 256;

fn tag_denied(name: &str) -> bool {
    DENIED_TAGS.iter().any(|t| name.eq_ignore_ascii_case(t))
}

/// Base URL for the host document. Blitz's default (`data:`) can't be a base,
/// and Blitz *panics* when an `<img src>`, `<a href>` or form `action` fails to
/// resolve — so a relative `src="a.png"` used to crash the browser. Nothing is
/// fetched either way: the document has no network provider.
pub const DOC_BASE_URL: &str = "gwb://app/";

/// Attributes Blitz resolves as URLs (and panics on if resolution fails).
fn url_attr_ok(name: &str, value: &str) -> bool {
    if !["src", "href", "action", "formaction"].iter().any(|a| name.eq_ignore_ascii_case(a)) {
        return true;
    }
    static BASE: std::sync::OnceLock<url::Url> = std::sync::OnceLock::new();
    BASE.get_or_init(|| url::Url::parse(DOC_BASE_URL).expect("valid base"))
        .join(value)
        .is_ok()
}

/// Where a tab's guest CSS may land (see css_scope.rs).
pub struct StyleScope {
    /// The guest's mount node (guest id 1).
    pub mount: usize,
    /// Host-owned container for this tab's scoped sheets. Guests can't
    /// address it (it's not in `fwd`), so they can't remove or edit them.
    pub styles_host: usize,
    /// Selector matching only `mount`, e.g. `.gwb-scope-t3>*`.
    pub prefix: String,
}

struct GuestStyle {
    /// The inert `gwb-style` node the guest sees as its `<style>`.
    node: usize,
    /// Host-owned live `<style>` element and its text node.
    shadow: Option<(usize, usize)>,
    /// Raw guest CSS currently loaded into `shadow` (None = nothing applied).
    applied: Option<String>,
}

#[derive(Default)]
pub struct NodeMaps {
    /// guest id -> blitz node id
    pub fwd: HashMap<u32, usize>,
    /// blitz node id -> guest id
    pub rev: HashMap<usize, u32>,
    /// (guest id, event kind) subscriptions
    pub listeners: HashSet<(u32, u16)>,
    /// guest id -> Observe() what-bits
    pub observers: HashMap<u32, u32>,
    /// last delivered layout rect per observed guest id
    pub last_rects: HashMap<u32, [f32; 4]>,
    /// Tab style scope. None = guest `<style>`s are inert and never applied.
    pub scope: Option<StyleScope>,
    /// guest id -> its `<style>` stand-in
    guest_styles: HashMap<u32, GuestStyle>,
    /// guest text-node id -> the guest style it was appended into
    style_text: HashMap<u32, u32>,
    /// Some guest style's text may have changed since the last sync.
    styles_dirty: bool,
    /// Emptied shadow sheets kept for reuse. Shadows are never dropped inside
    /// an apply: Blitz processes `<style>` nodes at mutator flush and indexes
    /// them directly, so dropping one in the same session would panic.
    spare_shadows: Vec<(usize, usize)>,
}

impl NodeMaps {
    /// Fresh maps for a guest mounted at `mount` (guest id 1).
    pub fn for_mount(mount: usize, scope: StyleScope) -> Self {
        let mut m = NodeMaps { scope: Some(scope), ..Default::default() };
        m.fwd.insert(1, mount);
        m.rev.insert(mount, 1);
        m
    }

    fn retire_style(&mut self, m: &mut DocumentMutator<'_>, gs: GuestStyle) {
        if let Some((el, text)) = gs.shadow {
            m.set_node_text(text, "");
            self.spare_shadows.push((el, text));
        }
    }

    /// Bring every host shadow sheet in line with its guest `<style>`: scoped
    /// text when the guest element is connected under the mount, empty
    /// otherwise.
    fn sync_guest_styles(&mut self, m: &mut DocumentMutator<'_>) {
        let Some(scope) = self.scope.as_ref() else { return };
        let (mount, host, prefix) = (scope.mount, scope.styles_host, scope.prefix.clone());
        let dirty = std::mem::take(&mut self.styles_dirty);

        let mut dead = Vec::new();
        let mut updates: Vec<(u32, Option<String>)> = Vec::new();
        for (&gid, gs) in &self.guest_styles {
            let Some(node) = m.doc.get_node(gs.node) else {
                dead.push(gid);
                continue;
            };
            if node.element_data().is_none_or(|e| e.name.local.as_ref() != GUEST_STYLE_TAG) {
                dead.push(gid);
                continue;
            }
            if is_within(m.doc, gs.node, mount) {
                if dirty || gs.applied.is_none() {
                    let src = node.text_content();
                    if gs.applied.as_deref() != Some(src.as_str()) {
                        updates.push((gid, Some(src)));
                    }
                }
            } else if gs.applied.is_some() {
                updates.push((gid, None));
            }
        }

        for gid in dead {
            if let Some(gs) = self.guest_styles.remove(&gid) {
                self.retire_style(m, gs);
            }
        }
        for (gid, src) in updates {
            let shadow = match self.guest_styles.get(&gid).and_then(|g| g.shadow) {
                Some(s) => s,
                None => {
                    let s = self.spare_shadows.pop().unwrap_or_else(|| {
                        let el = m.create_element(qual_local("style"), Vec::new());
                        let text = m.create_text_node("");
                        m.append_children(el, &[text]);
                        m.append_children(host, &[el]);
                        (el, text)
                    });
                    if let Some(g) = self.guest_styles.get_mut(&gid) {
                        g.shadow = Some(s);
                    }
                    s
                }
            };
            let text = match &src {
                Some(raw) => crate::css_scope::scoped_style_text(raw, &prefix),
                None => String::new(),
            };
            m.set_node_text(shadow.1, &text);
            if let Some(g) = self.guest_styles.get_mut(&gid) {
                g.applied = src;
            }
        }
    }
}

/// Drop guest-id mappings for every node in `root`'s subtree (optionally
/// excluding `root`) before Blitz frees them. Otherwise a stale guest id would
/// index a freed slab slot (panic) or, once Blitz reuses that slot for a host
/// chrome node or another tab's node, let this guest mutate it.
fn forget_subtree(maps: &mut NodeMaps, doc: &BaseDocument, root: usize, include_root: bool) {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(n) = doc.get_node(id) else { continue };
        stack.extend_from_slice(&n.children);
        if id == root && !include_root {
            continue;
        }
        if let Some(g) = maps.rev.remove(&id) {
            maps.fwd.remove(&g);
            maps.observers.remove(&g);
            maps.last_rects.remove(&g);
            maps.style_text.remove(&g);
        }
    }
}

fn qual_local(name: &str) -> QualName {
    QualName::new(None, ns!(html), LocalName::from(name))
}

/// Is `node` `ancestor` or inside it?
fn is_within(doc: &BaseDocument, node: usize, ancestor: usize) -> bool {
    let mut cur = Some(node);
    for _ in 0..100_000 {
        match cur {
            Some(id) if id == ancestor => return true,
            Some(id) => cur = doc.get_node(id).and_then(|n| n.parent),
            None => return false,
        }
    }
    false
}

thread_local! {
    /// Throwaway document guest innerHTML is parsed in (see `import_inner_html`).
    static SCRATCH: std::cell::RefCell<Option<blitz_html::HtmlDocument>> =
        const { std::cell::RefCell::new(None) };
}

/// Guest `SetInnerHtml` on a normal element. html5ever parses fragments under
/// the document root, so parsing straight into the live document would
/// register `<style>`/`<link>`/`<title>` with the engine before we could
/// filter them (and removing them afterwards panics at flush). Instead we parse
/// in a scratch document and copy across only text and allowed elements with
/// their attributes.
fn import_inner_html(m: &mut DocumentMutator<'_>, target: usize, html: &str) {
    let ctx_name = m
        .doc
        .get_node(target)
        .and_then(|n| n.element_data())
        .map(|e| e.name.clone())
        .unwrap_or_else(|| qual_local("div"));
    m.drop_children(target);

    SCRATCH.with(|cell| {
        let mut slot = cell.borrow_mut();
        let scratch = slot.get_or_insert_with(|| {
            blitz_html::HtmlDocument::from_html(
                "<!DOCTYPE html><html><body></body></html>",
                blitz_dom::DocumentConfig {
                    html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                    base_url: Some(DOC_BASE_URL.to_string()),
                    ..Default::default()
                },
            )
        });
        let Some(body) = scratch.query_selector("body").ok().flatten() else { return };
        let ctx = {
            let mut sm = scratch.mutate();
            let ctx = sm.create_element(ctx_name, Vec::new());
            sm.append_children(body, &[ctx]);
            sm.set_inner_html(ctx, html);
            ctx
        };
        let mut budget = MAX_INNER_HTML_NODES;
        copy_children(scratch, ctx, m, target, 0, &mut budget);
        scratch.mutate().remove_and_drop_node(ctx);
    });
}

fn copy_children(
    src: &BaseDocument,
    from: usize,
    m: &mut DocumentMutator<'_>,
    to: usize,
    depth: usize,
    budget: &mut usize,
) {
    if depth > MAX_INNER_HTML_DEPTH {
        return;
    }
    let Some(node) = src.get_node(from) else { return };
    let mut created = Vec::new();
    for &kid in &node.children {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Some(k) = src.get_node(kid) else { continue };
        match &k.data {
            blitz_dom::NodeData::Text(t) => created.push(m.create_text_node(&t.content)),
            blitz_dom::NodeData::Element(e) => {
                let local = e.name.local.as_ref();
                if local.eq_ignore_ascii_case("style") || tag_denied(local) {
                    crate::logger::log("abi", &format!("SetInnerHtml: dropped <{local}> (not allowed in guest markup)"));
                    continue;
                }
                let attrs = e
                    .attrs()
                    .iter()
                    .filter(|a| url_attr_ok(a.name.local.as_ref(), &a.value))
                    .cloned()
                    .collect();
                let id = m.create_element(e.name.clone(), attrs);
                copy_children(src, kid, m, id, depth + 1, budget);
                created.push(id);
            }
            _ => {}
        }
    }
    if !created.is_empty() {
        m.append_children(to, &created);
    }
}

/// Drain all pending batches into the document. Returns (ops applied,
/// blitz node focused by a Focus op — the caller should fix the caret).
pub fn apply_batches(
    doc: &mut BaseDocument,
    maps: &mut NodeMaps,
    shared: &Arc<Mutex<Shared>>,
) -> (usize, Option<usize>) {
    let mut guard = shared.lock_ok();
    if guard.batches.is_empty() {
        return (0, None);
    }
    let batches = std::mem::take(&mut guard.batches);

    // NOTE: an "auto-fragment" optimization (detach hot parents, mutate on
    // the cheap path, reattach once) was tried here and MEASURED SLOWER
    // (create5k apply 14ms -> 18.7ms): Blitz's per-op invalidation walks
    // short-circuit on already-dirty ancestors, so the per-op path is cheap
    // after the first op, while reattach re-traverses the whole subtree.
    // Bulk-create cost is intrinsic per-node creation (stylo data init,
    // slab insert) — an engine-side optimization target, not a batching one.
    let apply_start = Instant::now();
    let mut applied = 0;
    let mut focus_target: Option<usize> = None;
    let mut m = doc.mutate();
    // Blitz defers form-owner / <style> / <title> processing of newly attached
    // nodes to the mutator's flush and indexes them directly, so a node
    // attached and dropped in the same session panics at flush ("form.rs:
    // invalid key"). Flush before any dropping op if something was attached
    // since the last flush. Reconciliation batches are mostly attach-only, so
    // this rarely costs more than one extra flush per batch.
    let mut attached_since_flush = false;
    for ops in &batches {
        for op in ops {
            applied += 1;
            let drops = matches!(op, Op::Remove { .. } | Op::Clear { .. } | Op::SetInnerHtml { .. });
            if drops && attached_since_flush {
                drop(m);
                m = doc.mutate();
                attached_since_flush = false;
            }
            attached_since_flush |= matches!(
                op,
                Op::AppendChild { .. } | Op::InsertBefore { .. } | Op::ReplaceWith { .. } | Op::SetInnerHtml { .. }
            );
            let node = |maps: &NodeMaps, g: u32| maps.fwd.get(&g).copied();
            match op {
                Op::DefineAtom { atom, name } => {
                    let idx = *atom as usize;
                    if guard.atoms.len() <= idx {
                        guard.atoms.resize_with(idx + 1, || None);
                    }
                    guard.atoms[idx] = Some(AtomEntry::new(name.clone()));
                }
                Op::CreateElement { id, tag } => {
                    // id 1 is the mount; re-creating it would orphan the mount.
                    if *id == 1 {
                        continue;
                    }
                    let Some(entry) = guard.atoms.get_mut(*tag as usize).and_then(|e| e.as_mut())
                    else {
                        continue;
                    };
                    if let Some(prev) = maps.fwd.get(id) {
                        maps.rev.remove(prev);
                    }
                    if tag_denied(&entry.name) {
                        crate::logger::log(
                            "abi",
                            &format!("CreateElement <{}> denied (document-level/resource tag)", entry.name),
                        );
                        continue;
                    }
                    if let Some(old) = maps.guest_styles.remove(id) {
                        maps.retire_style(&mut m, old);
                    }
                    maps.style_text.remove(id);
                    let nid = if entry.name.eq_ignore_ascii_case("style") {
                        // Inert stand-in; its CSS is scoped into a host sheet.
                        let nid = m.create_element(qual_local(GUEST_STYLE_TAG), Vec::new());
                        m.set_style_property(nid, "display", "none");
                        maps.guest_styles.retain(|_, g| g.node != nid);
                        maps.guest_styles.insert(*id, GuestStyle { node: nid, shadow: None, applied: None });
                        maps.styles_dirty = true;
                        nid
                    } else {
                        m.create_element(entry.qual(), Vec::new())
                    };
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::CreateText { id, text } => {
                    if *id == 1 {
                        continue;
                    }
                    if let Some(prev) = maps.fwd.get(id) {
                        maps.rev.remove(prev);
                    }
                    if let Some(old) = maps.guest_styles.remove(id) {
                        maps.retire_style(&mut m, old);
                    }
                    maps.style_text.remove(id);
                    let nid = m.create_text_node(text);
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::SetAttr { id, name, value } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get_mut(*name as usize).and_then(|e| e.as_mut()),
                    ) {
                        if !url_attr_ok(&entry.name, value) {
                            crate::logger::log("abi", &format!("SetAttr {}: unresolvable URL dropped", entry.name));
                            continue;
                        }
                        m.set_attr(n, entry.qual(), value);
                    }
                }
                Op::RemoveAttr { id, name } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get_mut(*name as usize).and_then(|e| e.as_mut()),
                    ) {
                        m.clear_attr(n, entry.qual());
                    }
                }
                Op::SetText { id, text } => {
                    if let Some(n) = node(maps, *id) {
                        m.set_node_text(n, text);
                        if maps.style_text.contains_key(id) {
                            maps.styles_dirty = true;
                        }
                    }
                }
                Op::SetStyle { id, prop, value } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get(*prop as usize).and_then(|e| e.as_ref()),
                    ) {
                        m.set_style_property(n, &entry.name, value);
                    }
                }
                Op::RemoveStyle { id, prop } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get(*prop as usize).and_then(|e| e.as_ref()),
                    ) {
                        m.remove_style_property(n, &entry.name);
                    }
                }
                // Tree-shape guards (all were host crashes or hangs before):
                // the mount (guest id 1) may never be moved, replaced or
                // removed (the host holds its node id); a node may not be
                // moved into its own subtree (cycle); and Blitz's
                // insert/replace unwrap the anchor's parent, so the anchor
                // must be attached.
                Op::AppendChild { parent, child } => {
                    if let (Some(p), Some(c)) = (node(maps, *parent), node(maps, *child)) {
                        if *child != 1 && !is_within(m.doc, p, c) {
                            m.append_children(p, &[c]);
                            maps.style_text.remove(child);
                            if maps.guest_styles.contains_key(parent) {
                                maps.style_text.insert(*child, *parent);
                            }
                            maps.styles_dirty = true;
                        }
                    }
                }
                Op::InsertBefore { child, before } => {
                    if let (Some(c), Some(anchor)) = (node(maps, *child), node(maps, *before)) {
                        if *child != 1 && *before != 1 && m.node_has_parent(anchor) && !is_within(m.doc, anchor, c) {
                            m.insert_nodes_before(anchor, &[c]);
                            maps.style_text.remove(child);
                            if let Some(&style) = maps.style_text.get(before) {
                                maps.style_text.insert(*child, style);
                            }
                            maps.styles_dirty = true;
                        }
                    }
                }
                Op::ReplaceWith { old, new } => {
                    if let (Some(o), Some(n)) = (node(maps, *old), node(maps, *new)) {
                        if *old != 1 && *new != 1 && m.node_has_parent(o) && !is_within(m.doc, o, n) {
                            m.replace_node_with(o, &[n]);
                            if let Some(g) = maps.rev.remove(&o) {
                                maps.fwd.remove(&g);
                            }
                            if let Some(style) = maps.style_text.remove(old) {
                                maps.style_text.insert(*new, style);
                            }
                            maps.styles_dirty = true;
                        }
                    }
                }
                Op::Remove { id } => {
                    if *id == 1 {
                        continue;
                    }
                    if let Some(n) = node(maps, *id) {
                        forget_subtree(maps, m.doc, n, true);
                        m.drop_node(n);
                        maps.styles_dirty = true;
                    }
                }
                Op::Clear { id } => {
                    if let Some(n) = node(maps, *id) {
                        forget_subtree(maps, m.doc, n, false);
                        m.drop_children(n);
                        maps.styles_dirty = true;
                    }
                }
                Op::SetInnerHtml { id, html } => {
                    if let Some(n) = node(maps, *id) {
                        forget_subtree(maps, m.doc, n, false);
                        if maps.guest_styles.contains_key(id) {
                            // A real <style>'s innerHTML is raw text; keep that.
                            m.drop_children(n);
                            let t = m.create_text_node(html);
                            m.append_children(n, &[t]);
                        } else {
                            import_inner_html(&mut m, n, html);
                        }
                        maps.styles_dirty = true;
                    }
                }
                Op::Listen { id, kind } => {
                    maps.listeners.insert((*id, *kind));
                }
                Op::Unlisten { id, kind } => {
                    maps.listeners.remove(&(*id, *kind));
                }
                Op::Observe { id, what } => {
                    *maps.observers.entry(*id).or_insert(0) |= *what;
                }
                Op::Unobserve { id, what } => {
                    if let Some(bits) = maps.observers.get_mut(id) {
                        *bits &= !*what;
                        if *bits == 0 {
                            maps.observers.remove(id);
                            maps.last_rects.remove(id);
                        }
                    }
                }
                Op::Focus { id } => {
                    focus_target = node(maps, *id);
                }
            }
        }
    }
    if maps.styles_dirty || !maps.guest_styles.is_empty() {
        maps.sync_guest_styles(&mut m);
    }
    drop(m);
    if let Some(n) = focus_target {
        focus(doc, n);
    }
    crate::logger::log(
        "abi",
        &format!(
            "applied {applied} ops from {} batch(es), apply_us={}",
            batches.len(),
            apply_start.elapsed().as_micros()
        ),
    );
    (applied, focus_target)
}

// ---------------------------------------------------------------- guest runtime

pub struct HostState {
    wasi: WasiP1Ctx,
    shared: Arc<Mutex<Shared>>,
    proxy: BlitzShellProxy,
    limits: StoreLimits,
    /// Outstanding fetch + rpc_call requests (bounded, see net.rs).
    inflight: Inflight,
    /// Manifest-declared origins `fetch` may reach.
    fetch_origins: Arc<Vec<Origin>>,
}

pub struct GuestRuntime {
    store: Store<HostState>,
    memory: Memory,
    fn_events: TypedFunc<u32, u32>,
    fn_start: TypedFunc<(f32, f32, f32, u32), ()>,
    fn_frame: Option<TypedFunc<f32, ()>>,
    started_at: Instant,
    pub shared: Arc<Mutex<Shared>>,
}

fn console(proxy: &BlitzShellProxy, source: Source, text: &str) {
    proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(ConsoleMsg {
        source,
        bytes: format!("{text}\n").into_bytes(),
    })));
}

/// Load a wasm module. Returns Ok(None) if it doesn't export the GWB ABI
/// (caller should fall back to legacy console mode).
///
/// `engine` is one process-wide `wasmtime::Engine` shared across every tab's
/// guest (wasmtime `Engine`s are designed to be cheaply shared across many
/// `Store`s — creating one per guest wastes JIT/codegen setup for no benefit).
/// `tab_id` tags this guest's async fetch/rpc completions so multi-tab event
/// delivery can route each result back to its owning tab instead of
/// broadcasting to whichever guest happens to be active (see ui.rs).
pub fn try_load(
    engine: &Engine,
    path: &str,
    tab_id: u32,
    proxy: BlitzShellProxy,
    services: HashMap<String, ServiceEntry>,
    links: HashSet<String>,
    fetch_origins: Vec<Origin>,
) -> Result<Option<GuestRuntime>> {
    let module = Module::from_file(engine, path)
        .map_err(|e| anyhow!("loading {path}: {e}"))?;

    let is_gwb = module.exports().any(|e| e.name() == "gwb_abi_version");
    if !is_gwb {
        return Ok(None);
    }

    // Service registry (capabilities) comes from the resolved app manifest
    // (manifest.rs / docs/04-WEB-RPC.md §4). rpc_call's `service` indexes it;
    // an undeclared name is a capability violation, rejected pre-network.
    {
        let mut names: Vec<&str> = services.keys().map(|s| s.as_str()).collect();
        names.sort();
        crate::logger::log("rpc", &format!("services declared: [{}]", names.join(", ")));
    }

    let (app_key, app_key_b64) = load_or_create_app_key();
    let shared = Arc::new(Mutex::new(Shared {
        atoms: well_known_atoms(),
        batches: Vec::new(),
        event_region: (0, 0),
        next_fetch_id: 0,
        frame_requested: false,
        window_id: None,
        services,
        next_rpc_id: 0,
        session_token: None,
        app_key,
        app_key_b64,
        links,
        gesture_active: false,
        tab_id,
    }));

    let mut linker: Linker<HostState> = Linker::new(engine);
    wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |s: &mut HostState| &mut s.wasi)?;

    linker.func_wrap(
        "gwb",
        "submit",
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
            let shared = caller.data().shared.clone();
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return u32::MAX;
            };
            let data = mem.data(&caller);
            let Some(bytes) = guest_bytes(data, ptr, len) else {
                return u32::MAX;
            };
            let t0 = std::time::Instant::now();
            match decode_batch(bytes) {
                Ok(ops) => {
                    crate::logger::log(
                        "abi",
                        &format!(
                            "submit: {} ops, {} bytes, decode_us={}",
                            ops.len(),
                            len,
                            t0.elapsed().as_micros()
                        ),
                    );
                    let mut g = shared.lock_ok();
                    // Batches queue until the UI thread applies them; cap the
                    // backlog so a submit loop can't grow host memory unbounded.
                    let pending: usize = g.batches.iter().map(Vec::len).sum();
                    if pending + ops.len() > MAX_PENDING_OPS {
                        crate::logger::log("abi", &format!("submit: REJECTED, {pending} ops already pending"));
                        return 2;
                    }
                    g.batches.push(ops);
                    0
                }
                Err(e) => {
                    crate::logger::log("abi", &format!("submit: DECODE ERROR: {e}"));
                    1
                }
            }
        },
    )?;

    linker.func_wrap(
        "gwb",
        "event_region",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            crate::logger::log("abi", &format!("event_region: ptr={ptr} len={len}"));
            caller.data().shared.lock_ok().event_region = (ptr, len);
        },
    )?;

    linker.func_wrap(
        "gwb",
        "log",
        |mut caller: Caller<'_, HostState>, level: u32, ptr: u32, len: u32| {
            const MAX_LOG: u32 = 64 * 1024;
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else { return };
            let data = mem.data(&caller);
            let Some(bytes) = guest_bytes(data, ptr, len.min(MAX_LOG)) else { return };
            let text = String::from_utf8_lossy(bytes).into_owned();
            let source = if level >= 2 { Source::Stderr } else { Source::Stdout };
            crate::logger::log("guest:log", &text);
            console(&caller.data().proxy, source, &text);
        },
    )?;

    // Async HTTP: the host owns the event loop and the sockets (the guest is
    // freestanding wasm — no libuv/libcurl possible or needed). Each fetch
    // runs on a short-lived host thread; completion returns to the UI thread
    // as a NetResult and reaches the guest as a NET_RESULT event record.
    //
    // Policy (net.rs): only manifest-declared origins, no redirects, public
    // addresses only unless the manifest names a local IP, capped body, and at
    // most net::MAX_INFLIGHT requests per guest. A refused request still gets
    // an id and a NET_RESULT (ok=0, status=0, body=reason), like a browser
    // network error, so guests need no new error path.
    linker.func_wrap(
        "gwb",
        "fetch",
        move |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return 0;
            };
            let data = mem.data(&caller);
            let Some(bytes) = guest_bytes(data, ptr, len.min(64 * 1024)) else {
                return 0;
            };
            let url = String::from_utf8_lossy(bytes).into_owned();
            let id = {
                let mut g = caller.data().shared.lock_ok();
                g.next_fetch_id = g.next_fetch_id.wrapping_add(1).max(1);
                g.next_fetch_id
            };
            let proxy = caller.data().proxy.clone();
            let refuse = |why: String| {
                crate::logger::log("net", &format!("fetch #{id}: REFUSED {url}: {why}"));
                proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(NetResult {
                    id,
                    tab_id,
                    status: 0,
                    ok: false,
                    body: why,
                })));
                id
            };
            let (parsed, allow_private) = match net::check_fetch(&url, &caller.data().fetch_origins) {
                Ok(v) => v,
                Err(why) => return refuse(why),
            };
            let Some(slot) = caller.data().inflight.try_acquire() else {
                return refuse(format!("too many requests in flight (limit {})", net::MAX_INFLIGHT));
            };
            crate::logger::log("net", &format!("fetch #{id}: GET {url}"));
            let proxy = proxy.clone();
            std::thread::Builder::new()
                .name(format!("fetch-{id}"))
                .spawn(move || {
                    let _slot = slot;
                    let started = Instant::now();
                    let result = net::agent(allow_private).request_url("GET", &parsed).call();
                    let (ok, status, body) = match result {
                        Ok(resp) => {
                            let status = resp.status();
                            match net::read_body(resp) {
                                Ok(b) => (true, status, b),
                                Err(e) => (false, status, e),
                            }
                        }
                        Err(ureq::Error::Status(code, resp)) => {
                            (false, code, net::read_body(resp).unwrap_or_else(|e| e))
                        }
                        Err(e) => (false, 0, e.to_string()),
                    };
                    crate::logger::log(
                        "net",
                        &format!(
                            "fetch #{id}: {} {} ({} bytes, {:.0?})",
                            if ok { "OK" } else { "ERR" },
                            status,
                            body.len(),
                            started.elapsed()
                        ),
                    );
                    proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(
                        NetResult { id, tab_id, status, ok, body },
                    )));
                })
                .ok();
            id
        },
    )?;

    // Host-mediated RPC (docs/04-WEB-RPC.md). Mirrors `fetch`: parse the request
    // buffer, resolve the manifest-declared service (capability check), POST to
    // its endpoint on a host thread, and return the result as an RPC_RESULT
    // event correlated by req_id. The guest never sees a socket.
    linker.func_wrap(
        "gwb",
        "rpc_call",
        move |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return 0;
            };
            let data = mem.data(&caller);
            let Some(buf) = guest_bytes(data, ptr, len) else {
                return 0;
            };
            let l = buf.len();
            if l < 8 {
                return 0;
            }
            let u16_at = |o: usize| u16::from_le_bytes([buf[o], buf[o + 1]]) as usize;
            let service_len = u16_at(0);
            let iface_len = u16_at(2);
            let method_len = u16_at(4);
            let flags = u16_at(6);
            let head = 8;
            if head + service_len + iface_len + method_len > l {
                return 0;
            }
            let service = String::from_utf8_lossy(&buf[head..head + service_len]).into_owned();
            let iface =
                String::from_utf8_lossy(&buf[head + service_len..head + service_len + iface_len])
                    .into_owned();
            let mstart = head + service_len + iface_len;
            let method =
                String::from_utf8_lossy(&buf[mstart..mstart + method_len]).into_owned();
            let payload = &buf[mstart + method_len..];
            if payload.len() > net::MAX_REQUEST_BYTES {
                crate::logger::log("rpc", &format!("rpc_call REJECTED: payload {} bytes over limit", payload.len()));
                return 0;
            }
            let payload = payload.to_vec();
            let want_session = flags & 0b10 != 0;

            let (req_id, entry, session, app_key, app_key_b64) = {
                let mut g = caller.data().shared.lock_ok();
                // Capability check: the app may call only manifest-declared
                // services. An unknown name never reaches the network.
                let Some(entry) = g.services.get(&service).cloned() else {
                    crate::logger::log(
                        "rpc",
                        &format!("rpc_call REJECTED: undeclared service '{service}' (capability)"),
                    );
                    return 0;
                };
                // The manifest binds each service to one interface; a guest
                // can't reuse a declared service's endpoint for another iface.
                if iface != entry.iface {
                    crate::logger::log(
                        "rpc",
                        &format!(
                            "rpc_call REJECTED: service '{service}' is bound to '{}', not '{iface}' (capability)",
                            entry.iface
                        ),
                    );
                    return 0;
                }
                // The method becomes a URL path segment: keep it to a plain
                // identifier so it can't add path segments, a query or a fragment.
                if !valid_method(&method) {
                    crate::logger::log("rpc", &format!("rpc_call REJECTED: invalid method name '{method}'"));
                    return 0;
                }
                g.next_rpc_id = g.next_rpc_id.wrapping_add(1).max(1);
                let id = g.next_rpc_id;
                let session = if want_session { g.session_token.clone() } else { None };
                (id, entry, session, g.app_key.clone(), g.app_key_b64.clone())
            };

            let url = format!("{}/rpc/{}/{}", entry.endpoint.trim_end_matches('/'), iface, method);
            crate::logger::log(
                "rpc",
                &format!("rpc #{req_id}: {service}.{method} -> POST {url} ({} bytes)", payload.len()),
            );
            let proxy = caller.data().proxy.clone();
            let Some(slot) = caller.data().inflight.try_acquire() else {
                crate::logger::log("rpc", &format!("rpc #{req_id}: REFUSED, too many requests in flight"));
                proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(RpcResult {
                    req_id,
                    tab_id,
                    status: 0,
                    ok: false,
                    err_class: 1,
                    body: format!("too many requests in flight (limit {})", net::MAX_INFLIGHT),
                })));
                return req_id;
            };
            let allow_private = net::endpoint_allows_private(&entry.endpoint);
            std::thread::Builder::new()
                .name(format!("rpc-{req_id}"))
                .spawn(move || {
                    let _slot = slot;
                    use base64::Engine as _;
                    use ed25519_dalek::Signer as _;
                    use sha2::{Digest as _, Sha256};
                    let started = Instant::now();
                    // Channel authn (§2): sign canonical bytes with the app key.
                    // canonical = iface\nmethod\nreq_id\nts\nhex(sha256(body)).
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                        .to_string();
                    let body_hash = hex::encode(Sha256::digest(&payload));
                    let canonical = format!("{iface}\n{method}\n{req_id}\n{ts}\n{body_hash}");
                    let sig = app_key.sign(canonical.as_bytes());
                    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
                    let mut req = net::agent(allow_private)
                        .post(&url)
                        .set("Content-Type", "application/json")
                        .set("GWB-Req-Id", &req_id.to_string())
                        .set("GWB-Iface", &iface)
                        .set("GWB-App-Key", &app_key_b64)
                        .set("GWB-Sig", &sig_b64)
                        .set("GWB-Ts", &ts);
                    if let Some(tok) = &session {
                        req = req.set("GWB-Session", tok);
                    }
                    let result = req.send_bytes(&payload);
                    let (ok, status, err_class, body) = match result {
                        Ok(resp) => {
                            let status = resp.status();
                            match net::read_body(resp) {
                                Ok(b) => (true, status, 0u8, b),
                                Err(e) => (false, status, 1u8, e),
                            }
                        }
                        Err(ureq::Error::Status(code, resp)) => {
                            // HTTP error → map to an err_class (§1 of 04-WEB-RPC).
                            let ec = match code {
                                401 => 2, // authn
                                403 => 3, // authz
                                404 => 4, // not found
                                400 => 5, // bad request
                                _ => 6,   // server error
                            };
                            (false, code, ec, net::read_body(resp).unwrap_or_else(|e| e))
                        }
                        // Transport failure: the "backend is a zombie" signal (§3).
                        Err(e) => (false, 0, 1u8, e.to_string()),
                    };
                    crate::logger::log(
                        "rpc",
                        &format!(
                            "rpc #{req_id}: {} status={status} err_class={err_class} ({} bytes, {:.0?})",
                            if ok { "OK" } else { "ERR" },
                            body.len(),
                            started.elapsed()
                        ),
                    );
                    proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(RpcResult {
                        req_id,
                        tab_id,
                        status,
                        ok,
                        err_class,
                        body,
                    })));
                })
                .ok();
            req_id
        },
    )?;

    // Session slot (§3): the guest, after an auth.login RPC, hands the host the
    // token string; the host attaches it (GWB-Session) to calls that request it
    // via the GWB_RPC_F_SESSION flag. Keeps the host out of body-parsing.
    linker.func_wrap(
        "gwb",
        "session_set",
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else { return };
            let data = mem.data(&caller);
            if len as usize > MAX_SESSION_TOKEN {
                crate::logger::log("rpc", &format!("session_set REJECTED: {len} bytes (limit {MAX_SESSION_TOKEN})"));
                return;
            }
            let Some(bytes) = guest_bytes(data, ptr, len) else { return };
            // The token travels as an HTTP header value: printable ASCII only.
            if !bytes.iter().all(|b| (0x21..0x7f).contains(b)) {
                crate::logger::log("rpc", "session_set REJECTED: token is not printable ASCII");
                return;
            }
            let tok = String::from_utf8_lossy(bytes).into_owned();
            crate::logger::log("rpc", &format!("session_set ({} bytes)", tok.len()));
            caller.data().shared.lock_ok().session_token = Some(tok);
        },
    )?;
    linker.func_wrap("gwb", "session_clear", |caller: Caller<'_, HostState>| {
        caller.data().shared.lock_ok().session_token = None;
    })?;

    linker.func_wrap("gwb", "request_frame", |caller: Caller<'_, HostState>| {
        // Just mark the request. GwbApplication::about_to_wait pumps pending
        // frames via window.poll() + request_redraw(), so animation is paced
        // by the redraw/vsync stream instead of spinning the event loop.
        caller.data().shared.lock_ok().frame_requested = true;
    })?;

    // Guest-requested cross-site navigation. Same law as fetch/rpc_call: the
    // guest expresses intent, the host acts — it never touches tab/window
    // state directly. Two gates before anything is queued:
    //   1. Capability: `target` (bare name, e.g. "retailer.local") must be in
    //      this app's manifest-declared `links` — an undeclared target is
    //      rejected here, before any navigation happens (mirrors rpc_call's
    //      undeclared-service rejection).
    //   2. Gesture: only honored while `gesture_active` is true, i.e. while
    //      the host is dispatching a genuine click/key event to this guest —
    //      never from a frame tick or an async RPC/fetch completion. No
    //      drive-by redirects with no user action behind them.
    // Returns 0 (queued), 1 (undeclared target), or 2 (no active gesture).
    // The actual navigation happens later on the UI thread (ui.rs's
    // deliver_nav_request) — this call can't do it synchronously: a wasmtime
    // host-import closure only closes over HostState, not the GwbDocument
    // that owns the tab/mount tree.
    linker.func_wrap(
        "gwb",
        "navigate",
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32, flags: u32| -> u32 {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return 1;
            };
            let data = mem.data(&caller);
            let Some(bytes) = guest_bytes(data, ptr, len.min(1024)) else {
                return 1;
            };
            let target = String::from_utf8_lossy(bytes).into_owned();
            let bare = target.trim_start_matches("web://").to_string();

            let (tab_id, proxy) = {
                let g = caller.data().shared.lock_ok();
                if !g.gesture_active {
                    crate::logger::log(
                        "nav",
                        &format!("navigate REJECTED: no active user gesture (target='{target}')"),
                    );
                    return 2;
                }
                if !g.links.contains(&bare) {
                    crate::logger::log(
                        "nav",
                        &format!("navigate REJECTED: undeclared link '{bare}' (capability)"),
                    );
                    return 1;
                }
                (g.tab_id, caller.data().proxy.clone())
            };
            crate::logger::log("nav", &format!("navigate: tab {tab_id} -> '{target}' new_tab={}", flags & 1 != 0));
            proxy.send_event(blitz_shell::BlitzShellEvent::Embedder(Arc::new(NavRequest {
                tab_id,
                target,
                new_tab: flags & 1 != 0,
            })));
            0
        },
    )?;

    // stdout/stderr only: no stdin (a blocking read would hang the UI thread
    // inside a host call, where the epoch deadline can't interrupt it), no
    // filesystem, no env, no sockets.
    let wasi = WasiCtxBuilder::new().inherit_stdout().inherit_stderr().build_p1();
    let mut store = Store::new(
        engine,
        HostState {
            wasi,
            shared: shared.clone(),
            proxy,
            limits: guest_store_limits(),
            inflight: Inflight::default(),
            fetch_origins: Arc::new(fetch_origins),
        },
    );
    store.limiter(|s| &mut s.limits);
    store.set_epoch_deadline(START_BUDGET_TICKS);

    let instance = linker.instantiate(&mut store, &module)?;

    // wasip1 reactor init
    if let Some(init) = instance.get_func(&mut store, "_initialize") {
        init.typed::<(), ()>(&store)?.call(&mut store, ())?;
    }

    let version = instance
        .get_typed_func::<(), u32>(&mut store, "gwb_abi_version")?
        .call(&mut store, ())?;
    if version >> 16 != ABI_MAJOR {
        bail!("guest ABI major {} != host {}", version >> 16, ABI_MAJOR);
    }

    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| anyhow!("guest has no exported memory"))?;
    let fn_events = instance.get_typed_func::<u32, u32>(&mut store, "gwb_events")?;
    let fn_start = instance.get_typed_func::<(f32, f32, f32, u32), ()>(&mut store, "gwb_start")?;
    let fn_frame = instance
        .get_typed_func::<f32, ()>(&mut store, "gwb_frame")
        .ok();

    crate::logger::log("abi", &format!("gwb guest loaded: {path} abi=v{}.{}", version >> 16, version & 0xFFFF));
    Ok(Some(GuestRuntime {
        store,
        memory,
        fn_events,
        fn_start,
        fn_frame,
        started_at: Instant::now(),
        shared,
    }))
}

/// An encoded outgoing event: (kind, 16-byte payload, optional trailing string).
pub struct EventOut {
    pub kind: u16,
    pub payload: [u8; 16],
    pub text: Option<String>,
}

impl EventOut {
    fn new(kind: u16) -> Self {
        Self { kind, payload: [0; 16], text: None }
    }
    fn f32_at(mut self, off: usize, v: f32) -> Self {
        self.payload[off..off + 4].copy_from_slice(&v.to_le_bytes());
        self
    }
    fn u16_at(mut self, off: usize, v: u16) -> Self {
        self.payload[off..off + 2].copy_from_slice(&v.to_le_bytes());
        self
    }
    fn u8_at(mut self, off: usize, v: u8) -> Self {
        self.payload[off] = v;
        self
    }
    fn u32_at(mut self, off: usize, v: u32) -> Self {
        self.payload[off..off + 4].copy_from_slice(&v.to_le_bytes());
        self
    }
    fn text(mut self, s: String) -> Self {
        self.text = Some(s);
        self
    }

    pub fn layout_rect(rect: [f32; 4]) -> Self {
        EventOut::new(ev::OBSERVED_LAYOUT)
            .f32_at(0, rect[0])
            .f32_at(4, rect[1])
            .f32_at(8, rect[2])
            .f32_at(12, rect[3])
    }

    pub fn window_resize(w: f32, h: f32, scale: f32) -> Self {
        EventOut::new(ev::WINDOW_RESIZE)
            .f32_at(0, w)
            .f32_at(4, h)
            .f32_at(8, scale)
    }

    pub fn theme_change(dark: bool) -> Self {
        EventOut::new(ev::THEME_CHANGE).u32_at(0, dark as u32)
    }

    pub fn page_load(w: f32, h: f32, scale: f32) -> Self {
        EventOut::new(ev::PAGE_LOAD)
            .f32_at(0, w)
            .f32_at(4, h)
            .f32_at(8, scale)
    }

    pub fn net_result(status: u16, ok: bool, body: String) -> Self {
        EventOut::new(ev::NET_RESULT)
            .u16_at(0, status)
            .u8_at(2, ok as u8)
            .text(body)
    }

    /// RPC_RESULT (kind 41): {status u16, ok u8, err_class u8, req_id u32} + body.
    pub fn rpc_result(req_id: u32, status: u16, ok: bool, err_class: u8, body: String) -> Self {
        EventOut::new(ev::RPC_RESULT)
            .u16_at(0, status)
            .u8_at(2, ok as u8)
            .u8_at(3, err_class)
            .u32_at(4, req_id)
            .text(body)
    }
}

/// Map a Blitz DOM event to a GWB event record. Returns None for kinds the
/// ABI doesn't forward (legacy Mouse*/Touch* duplicates, IME composition).
pub fn map_dom_event(data: &blitz_traits::events::DomEventData) -> Option<EventOut> {
    use blitz_traits::events::{BlitzPointerEvent, DomEventData as D};

    fn pointer(kind: u16, p: &BlitzPointerEvent) -> EventOut {
        EventOut::new(kind)
            .f32_at(0, p.page_x())
            .f32_at(4, p.page_y())
            .u16_at(8, p.buttons.bits() as u16)
            .u16_at(10, (p.mods.bits() & 0xFFFF) as u16)
    }

    Some(match data {
        D::PointerDown(p) => pointer(ev::POINTER_DOWN, p),
        D::PointerUp(p) => pointer(ev::POINTER_UP, p),
        D::PointerMove(p) => pointer(ev::POINTER_MOVE, p),
        D::PointerEnter(p) => pointer(ev::POINTER_ENTER, p),
        D::PointerLeave(p) => pointer(ev::POINTER_LEAVE, p),
        D::Click(p) => pointer(ev::CLICK, p),
        D::DoubleClick(p) => pointer(ev::DBLCLICK, p),
        D::ContextMenu(p) => pointer(ev::CONTEXT_MENU, p),
        D::PointerCancel(p) => pointer(ev::POINTER_CANCEL, p),
        D::Wheel(w) => {
            use blitz_traits::events::BlitzWheelDelta;
            let (dx, dy) = match w.delta {
                BlitzWheelDelta::Pixels(x, y) => (x as f32, y as f32),
                // Lines are normalized to pixels with a conventional factor.
                BlitzWheelDelta::Lines(x, y) => (x as f32 * 40.0, y as f32 * 40.0),
            };
            EventOut::new(ev::WHEEL)
                .f32_at(0, dx)
                .f32_at(4, dy)
                .u16_at(8, (w.mods.bits() & 0xFFFF) as u16)
        }
        D::KeyDown(k) | D::KeyUp(k) => {
            let kind = if matches!(data, D::KeyDown(_)) { ev::KEY_DOWN } else { ev::KEY_UP };
            let text = k
                .text
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| k.key.to_string());
            EventOut::new(kind)
                .u16_at(2, (k.modifiers.bits() & 0xFFFF) as u16)
                .u8_at(4, k.state.is_pressed() as u8)
                .text(text)
        }
        D::KeyPress(k) => {
            let text = k
                .text
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| k.key.to_string());
            EventOut::new(ev::TEXT_INPUT)
                .u16_at(2, (k.modifiers.bits() & 0xFFFF) as u16)
                .text(text)
        }
        D::Input(i) => EventOut::new(ev::INPUT).text(i.value.clone()),
        D::Focus(_) => EventOut::new(ev::FOCUS),
        D::Blur(_) => EventOut::new(ev::BLUR),
        D::Scroll(s) => EventOut::new(ev::SCROLL)
            .f32_at(0, s.scroll_left as f32)
            .f32_at(4, s.scroll_top as f32),
        // Mouse*/Touch* are legacy duplicates of the Pointer* stream; Ime,
        // FocusIn/Out and Apple keybindings are not forwarded in v1.
        _ => return None,
    })
}

impl GuestRuntime {
    pub fn start(&mut self, w: f32, h: f32, scale: f32, flags: u32) -> Result<()> {
        self.store.set_epoch_deadline(START_BUDGET_TICKS);
        self.fn_start.call(&mut self.store, (w, h, scale, flags))?;
        let region = self.shared.lock_ok().event_region;
        if region.1 < 64 {
            bail!("guest did not register a usable event region during gwb_start");
        }
        Ok(())
    }

    /// Write one event record into the event region and deliver it.
    /// Returns the guest's response flags (bit0 prevent_default, bit1 stop_propagation).
    pub fn deliver_event(
        &mut self,
        eo: &EventOut,
        flags: u16,
        target: u32,
        listener: u32,
    ) -> Result<u32> {
        let (ptr, cap) = self.shared.lock_ok().event_region;
        let text = eo.text.as_deref().unwrap_or("").as_bytes();
        let padded = (text.len() + 3) & !3;
        let total = 40 + padded;
        if (cap as usize) < total {
            bail!("event region too small ({cap} < {total})");
        }
        let mut rec = vec![0u8; total];
        rec[0..2].copy_from_slice(&eo.kind.to_le_bytes());
        rec[2..4].copy_from_slice(&flags.to_le_bytes());
        rec[4..8].copy_from_slice(&target.to_le_bytes());
        rec[8..12].copy_from_slice(&listener.to_le_bytes());
        let ts = self.started_at.elapsed().as_secs_f64() * 1000.0;
        rec[12..20].copy_from_slice(&ts.to_le_bytes());
        rec[20..36].copy_from_slice(&eo.payload);
        rec[36..40].copy_from_slice(&(text.len() as u32).to_le_bytes());
        rec[40..40 + text.len()].copy_from_slice(text);
        self.memory.write(&mut self.store, ptr as usize, &rec)?;
        self.store.set_epoch_deadline(CALL_BUDGET_TICKS);
        let out = self.fn_events.call(&mut self.store, 1)?;
        Ok(out)
    }

    /// Deliver an animation frame tick if the guest exports gwb_frame.
    pub fn deliver_frame(&mut self, dt_ms: f32) -> Result<()> {
        if let Some(f) = &self.fn_frame {
            self.store.set_epoch_deadline(CALL_BUDGET_TICKS);
            f.call(&mut self.store, dt_ms)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "abi_tests.rs"]
mod tests;
