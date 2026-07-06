//! GWB ABI v0 — host implementation (see docs/ABI.md).
//!
//! v0 scope: batch decode (fixed 16-byte records + string heap), well-known +
//! dynamic atoms, Listen subscriptions, click delivery into the guest event
//! region. Hacky corners are marked; the contract shape matches the spec.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow, bail};
use blitz_dom::{BaseDocument, LocalName, QualName, ns};
use blitz_shell::BlitzShellProxy;
use wasmtime::{Caller, Engine, Extern, Linker, Memory, Module, Store, TypedFunc};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::ui::{ConsoleMsg, Source};

pub const ABI_MAJOR: u32 = 1;

pub mod ev {
    pub const CLICK: u16 = 4;
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
    Remove { id: u32 },
    Clear { id: u32 },
    SetInnerHtml { id: u32, html: String },
    DefineAtom { atom: u32, name: String },
    Listen { id: u32, kind: u16 },
    Unlisten { id: u32, kind: u16 },
}

// ---------------------------------------------------------------- shared state

pub struct Shared {
    pub atoms: Vec<Option<String>>,
    pub batches: Vec<Vec<Op>>,
    pub event_region: (u32, u32),
}

/// Well-known atoms 0..1024 (spec appendix; mirrored in sdk/gwb).
fn well_known_atoms() -> Vec<Option<String>> {
    let mut v: Vec<Option<String>> = vec![None; 1024];
    let entries: &[(usize, &str)] = &[
        // elements: 1..
        (1, "div"), (2, "span"), (3, "p"), (4, "h1"), (5, "h2"), (6, "h3"),
        (7, "button"), (8, "input"), (9, "a"), (10, "img"), (11, "ul"),
        (12, "ol"), (13, "li"), (14, "table"), (15, "tr"), (16, "td"),
        (17, "th"), (18, "form"), (19, "label"), (20, "section"),
        (21, "header"), (22, "footer"), (23, "main"), (24, "nav"),
        (25, "article"), (26, "pre"), (27, "code"), (28, "strong"), (29, "em"),
        (30, "textarea"), (31, "select"), (32, "option"),
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
        v[i] = Some(s.to_string());
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
            10 => Op::Remove { id: b },
            12 => Op::Clear { id: b },
            13 => Op::SetInnerHtml { id: b, html: get_str(d)? },
            14 => Op::DefineAtom { atom: b, name: get_str(d)? },
            15 => Op::Listen { id: b, kind: a },
            16 => Op::Unlisten { id: b, kind: a },
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
}

fn html_name(name: &str) -> QualName {
    QualName::new(None, ns!(html), LocalName::from(name))
}

/// Drain all pending batches into the document. Returns number of ops applied.
pub fn apply_batches(
    doc: &mut BaseDocument,
    maps: &mut NodeMaps,
    shared: &Arc<Mutex<Shared>>,
) -> usize {
    let mut guard = shared.lock().unwrap();
    if guard.batches.is_empty() {
        return 0;
    }
    let batches = std::mem::take(&mut guard.batches);

    let mut applied = 0;
    let mut m = doc.mutate();
    for ops in &batches {
        for op in ops {
            applied += 1;
            // Cloned atom lookups keep borrows of `guard` short (DefineAtom
            // mutates the same table mid-loop). v0 simplicity over zero-copy.
            let atom = |g: &std::sync::MutexGuard<'_, Shared>, a: u32| -> Option<String> {
                g.atoms.get(a as usize).and_then(|s| s.clone())
            };
            let node = |maps: &NodeMaps, g: u32| maps.fwd.get(&g).copied();
            match op {
                Op::DefineAtom { atom, name } => {
                    let idx = *atom as usize;
                    if guard.atoms.len() <= idx {
                        guard.atoms.resize(idx + 1, None);
                    }
                    guard.atoms[idx] = Some(name.clone());
                }
                Op::CreateElement { id, tag } => {
                    let Some(tag) = atom(&guard, *tag) else { continue };
                    let nid = m.create_element(html_name(&tag), Vec::new());
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::CreateText { id, text } => {
                    let nid = m.create_text_node(text);
                    maps.fwd.insert(*id, nid);
                    maps.rev.insert(nid, *id);
                }
                Op::SetAttr { id, name, value } => {
                    if let (Some(n), Some(name)) = (node(maps, *id), atom(&guard, *name)) {
                        m.set_attribute(n, html_name(&name), value);
                    }
                }
                Op::RemoveAttr { id, name } => {
                    if let (Some(n), Some(name)) = (node(maps, *id), atom(&guard, *name)) {
                        m.clear_attribute(n, html_name(&name));
                    }
                }
                Op::SetText { id, text } => {
                    if let Some(n) = node(maps, *id) {
                        m.set_node_text(n, text);
                    }
                }
                Op::SetStyle { id, prop, value } => {
                    if let (Some(n), Some(prop)) = (node(maps, *id), atom(&guard, *prop)) {
                        m.set_style_property(n, &prop, value);
                    }
                }
                Op::RemoveStyle { id, prop } => {
                    if let (Some(n), Some(prop)) = (node(maps, *id), atom(&guard, *prop)) {
                        m.remove_style_property(n, &prop);
                    }
                }
                Op::AppendChild { parent, child } => {
                    if let (Some(p), Some(c)) = (node(maps, *parent), node(maps, *child)) {
                        m.append_children(p, &[c]);
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
            }
        }
    }
    drop(m);
    crate::logger::log("abi", &format!("applied {applied} ops from {} batch(es)", batches.len()));
    applied
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

    let shared = Arc::new(Mutex::new(Shared {
        atoms: well_known_atoms(),
        batches: Vec::new(),
        event_region: (0, 0),
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
            match decode_batch(&data[ptr..ptr + len]) {
                Ok(ops) => {
                    crate::logger::log("abi", &format!("submit: {} ops, {} bytes", ops.len(), len));
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

    linker.func_wrap("gwb", "request_frame", |_caller: Caller<'_, HostState>| {
        crate::logger::log("abi", "request_frame (v0: unimplemented)");
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

    crate::logger::log("abi", &format!("gwb guest loaded: {path} abi=v{}.{}", version >> 16, version & 0xFFFF));
    Ok(Some(GuestRuntime {
        store,
        memory,
        fn_events,
        fn_start,
        started_at: Instant::now(),
        shared,
    }))
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

    /// Write one click record into the event region and deliver it.
    pub fn deliver_click(&mut self, target: u32, listener: u32, x: f32, y: f32) -> Result<u32> {
        let (ptr, cap) = self.shared.lock().unwrap().event_region;
        if cap < 44 {
            bail!("event region too small");
        }
        let mut rec = [0u8; 40];
        rec[0..2].copy_from_slice(&ev::CLICK.to_le_bytes()); // kind
        // flags u16 @2 = 0
        rec[4..8].copy_from_slice(&target.to_le_bytes());
        rec[8..12].copy_from_slice(&listener.to_le_bytes());
        let ts = self.started_at.elapsed().as_secs_f64() * 1000.0;
        rec[12..20].copy_from_slice(&ts.to_le_bytes());
        rec[20..24].copy_from_slice(&x.to_le_bytes());
        rec[24..28].copy_from_slice(&y.to_le_bytes());
        // buttons/mods/detail @28..34 = 0; str_len @36 = 0
        self.memory.write(&mut self.store, ptr as usize, &rec)?;
        let flags = self.fn_events.call(&mut self.store, 1)?;
        Ok(flags)
    }
}
