//! GWB ABI v0 — host implementation (see docs/ABI.md).
//!
//! v0 scope: batch decode (fixed 16-byte records + string heap), well-known +
//! dynamic atoms, Listen subscriptions, click delivery into the guest event
//! region. Hacky corners are marked; the contract shape matches the spec.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use blitz_dom::{BaseDocument, LocalName, QualName, ns};
use blitz_shell::BlitzShellProxy;
use wasmtime::{Caller, Engine, Extern, Linker, Memory, Module, Store, TypedFunc};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::ui::{ConsoleMsg, Source};

pub const ABI_MAJOR: u32 = 1;

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
            crate::logger::log("rpc", "generated app-scoped key (.gwb-app-key.bin)");
            s
        }
    };
    let key = SigningKey::from_bytes(&seed);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
    (Arc::new(key), pub_b64)
}

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
    pub status: u16,
    pub ok: bool,
    pub err_class: u8,
    pub body: String,
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

fn decode_batch(bytes: &[u8]) -> Result<Vec<Op>> {
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
    if heap_off + heap_len > bytes.len() || 16 + op_count * 16 > bytes.len() {
        bail!("batch bounds");
    }
    let heap = &bytes[heap_off..heap_off + heap_len];

    let get_str = |d: u32| -> Result<String> {
        if d == NO_STR {
            return Ok(String::new());
        }
        let off = d as usize;
        if off + 4 > heap.len() {
            bail!("str offset out of range");
        }
        let len = u32::from_le_bytes(heap[off..off + 4].try_into().unwrap()) as usize;
        if off + 4 + len > heap.len() {
            bail!("str out of range");
        }
        Ok(String::from_utf8_lossy(&heap[off + 4..off + 4 + len]).into_owned())
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
            14 => Op::DefineAtom { atom: b, name: get_str(d)? },
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
}

/// Drain all pending batches into the document. Returns (ops applied,
/// blitz node focused by a Focus op — the caller should fix the caret).
pub fn apply_batches(
    doc: &mut BaseDocument,
    maps: &mut NodeMaps,
    shared: &Arc<Mutex<Shared>>,
) -> (usize, Option<usize>) {
    let mut guard = shared.lock().unwrap();
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
    for ops in &batches {
        for op in ops {
            applied += 1;
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
                    let Some(entry) = guard.atoms.get_mut(*tag as usize).and_then(|e| e.as_mut())
                    else {
                        continue;
                    };
                    let nid = m.create_element(entry.qual(), Vec::new());
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::CreateText { id, text } => {
                    let nid = m.create_text_node(text);
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::SetAttr { id, name, value } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get_mut(*name as usize).and_then(|e| e.as_mut()),
                    ) {
                        m.set_attribute(n, entry.qual(), value);
                    }
                }
                Op::RemoveAttr { id, name } => {
                    if let (Some(n), Some(entry)) = (
                        node(maps, *id),
                        guard.atoms.get_mut(*name as usize).and_then(|e| e.as_mut()),
                    ) {
                        m.clear_attribute(n, entry.qual());
                    }
                }
                Op::SetText { id, text } => {
                    if let Some(n) = node(maps, *id) {
                        m.set_node_text(n, text);
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
                Op::AppendChild { parent, child } => {
                    if let (Some(p), Some(c)) = (node(maps, *parent), node(maps, *child)) {
                        m.append_children(p, &[c]);
                    }
                }
                Op::InsertBefore { child, before } => {
                    if let (Some(c), Some(anchor)) = (node(maps, *child), node(maps, *before)) {
                        m.insert_nodes_before(anchor, &[c]);
                    }
                }
                Op::ReplaceWith { old, new } => {
                    if let (Some(o), Some(n)) = (node(maps, *old), node(maps, *new)) {
                        m.replace_node_with(o, &[n]);
                        if let Some(g) = maps.rev.remove(&o) {
                            maps.fwd.remove(&g);
                        }
                    }
                }
                Op::Remove { id } => {
                    if let Some(n) = node(maps, *id) {
                        m.remove_and_drop_node(n);
                        if let Some(g) = maps.rev.remove(&n) {
                            maps.fwd.remove(&g);
                        }
                    }
                }
                Op::Clear { id } => {
                    if let Some(n) = node(maps, *id) {
                        m.remove_and_drop_all_children(n);
                    }
                }
                Op::SetInnerHtml { id, html } => {
                    if let Some(n) = node(maps, *id) {
                        m.set_inner_html(n, html);
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
    drop(m);
    if let Some(n) = focus_target {
        doc.set_focus_to(n);
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
pub fn try_load(path: &str, proxy: BlitzShellProxy) -> Result<Option<GuestRuntime>> {
    let engine = Engine::default();
    let module = Module::from_file(&engine, path)
        .map_err(|e| anyhow!("loading {path}: {e}"))?;

    let is_gwb = module.exports().any(|e| e.name() == "gwb_abi_version");
    if !is_gwb {
        return Ok(None);
    }

    // Phase 1 dev seed: a single "echo" service at the local dev server, so the
    // RPC transport is testable before the manifest resolver (Phase 3) exists.
    // Phase 3 replaces this with services parsed from the app manifest.
    let mut services = HashMap::new();
    let dev_endpoint = std::env::var("GWB_RPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
    services.insert(
        "echo".to_string(),
        ServiceEntry { endpoint: dev_endpoint.clone(), iface: "gwb.echo.v1".to_string(), server_key: String::new() },
    );

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
    }));

    let mut linker: Linker<HostState> = Linker::new(&engine);
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
            let (ptr, len) = (ptr as usize, len as usize);
            if ptr + len > data.len() {
                return u32::MAX;
            }
            let t0 = std::time::Instant::now();
            match decode_batch(&data[ptr..ptr + len]) {
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
                    shared.lock().unwrap().batches.push(ops);
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
            caller.data().shared.lock().unwrap().event_region = (ptr, len);
        },
    )?;

    linker.func_wrap(
        "gwb",
        "log",
        |mut caller: Caller<'_, HostState>, level: u32, ptr: u32, len: u32| {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else { return };
            let data = mem.data(&caller);
            let (ptr, len) = (ptr as usize, len as usize);
            if ptr + len > data.len() {
                return;
            }
            let text = String::from_utf8_lossy(&data[ptr..ptr + len]).into_owned();
            let source = if level >= 2 { Source::Stderr } else { Source::Stdout };
            crate::logger::log("guest:log", &text);
            console(&caller.data().proxy, source, &text);
        },
    )?;

    // Async HTTP: the host owns the event loop and the sockets (the guest is
    // freestanding wasm — no libuv/libcurl possible or needed). Each fetch
    // runs on a short-lived host thread; completion returns to the UI thread
    // as a NetResult and reaches the guest as a NET_RESULT event record.
    linker.func_wrap(
        "gwb",
        "fetch",
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return 0;
            };
            let data = mem.data(&caller);
            let (ptr, len) = (ptr as usize, len as usize);
            if ptr + len > data.len() {
                return 0;
            }
            let url = String::from_utf8_lossy(&data[ptr..ptr + len]).into_owned();
            let id = {
                let mut g = caller.data().shared.lock().unwrap();
                g.next_fetch_id += 1;
                g.next_fetch_id
            };
            crate::logger::log("net", &format!("fetch #{id}: GET {url}"));
            let proxy = caller.data().proxy.clone();
            std::thread::Builder::new()
                .name(format!("fetch-{id}"))
                .spawn(move || {
                    let started = Instant::now();
                    let result = ureq::get(&url)
                        .timeout(std::time::Duration::from_secs(15))
                        .call();
                    let (ok, status, body) = match result {
                        Ok(resp) => {
                            let status = resp.status();
                            (true, status, resp.into_string().unwrap_or_default())
                        }
                        Err(ureq::Error::Status(code, resp)) => {
                            (false, code, resp.into_string().unwrap_or_default())
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
                        NetResult { id, status, ok, body },
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
        |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
            let Some(Extern::Memory(mem)) = caller.get_export("memory") else {
                return 0;
            };
            let data = mem.data(&caller);
            let (p, l) = (ptr as usize, len as usize);
            if p + l > data.len() || l < 8 {
                return 0;
            }
            let buf = &data[p..p + l];
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
            let payload = buf[mstart + method_len..].to_vec();
            let want_session = flags & 0b10 != 0;

            let (req_id, entry, session, app_key, app_key_b64) = {
                let mut g = caller.data().shared.lock().unwrap();
                // Capability check: the app may call only manifest-declared
                // services. An unknown name never reaches the network.
                let Some(entry) = g.services.get(&service).cloned() else {
                    crate::logger::log(
                        "rpc",
                        &format!("rpc_call REJECTED: undeclared service '{service}' (capability)"),
                    );
                    return 0;
                };
                g.next_rpc_id += 1;
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
            std::thread::Builder::new()
                .name(format!("rpc-{req_id}"))
                .spawn(move || {
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
                    let mut req = ureq::post(&url)
                        .timeout(std::time::Duration::from_secs(15))
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
                            (true, status, 0u8, resp.into_string().unwrap_or_default())
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
                            (false, code, ec, resp.into_string().unwrap_or_default())
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
            let (p, l) = (ptr as usize, len as usize);
            if p + l > data.len() {
                return;
            }
            let tok = String::from_utf8_lossy(&data[p..p + l]).into_owned();
            crate::logger::log("rpc", &format!("session_set ({} bytes)", tok.len()));
            caller.data().shared.lock().unwrap().session_token = Some(tok);
        },
    )?;
    linker.func_wrap("gwb", "session_clear", |caller: Caller<'_, HostState>| {
        caller.data().shared.lock().unwrap().session_token = None;
    })?;

    linker.func_wrap("gwb", "request_frame", |caller: Caller<'_, HostState>| {
        // Just mark the request. GwbApplication::about_to_wait pumps pending
        // frames via window.poll() + request_redraw(), so animation is paced
        // by the redraw/vsync stream instead of spinning the event loop.
        caller.data().shared.lock().unwrap().frame_requested = true;
    })?;

    let wasi = WasiCtxBuilder::new().inherit_stdio().build_p1();
    let mut store = Store::new(
        &engine,
        HostState { wasi, shared: shared.clone(), proxy },
    );

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
        self.fn_start.call(&mut self.store, (w, h, scale, flags))?;
        let region = self.shared.lock().unwrap().event_region;
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
        let (ptr, cap) = self.shared.lock().unwrap().event_region;
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
        let out = self.fn_events.call(&mut self.store, 1)?;
        Ok(out)
    }

    /// Deliver an animation frame tick if the guest exports gwb_frame.
    pub fn deliver_frame(&mut self, dt_ms: f32) -> Result<()> {
        if let Some(f) = &self.fn_frame {
            f.call(&mut self.store, dt_ms)?;
        }
        Ok(())
    }
}
