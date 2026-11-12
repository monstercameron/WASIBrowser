//! Tests for abi.rs: batch decoding, wasm resource limits, and apply_batches
//! against a real Blitz document.

use super::*;

// ---- batch encoding helper (mirrors sdk/gwb's encoder) -------------------

pub(crate) struct Enc {
    ops: Vec<[u8; 16]>,
    pub(crate) heap: Vec<u8>,
}

impl Enc {
    pub(crate) fn new() -> Self {
        Enc { ops: Vec::new(), heap: Vec::new() }
    }
    pub(crate) fn s(&mut self, s: &str) -> u32 {
        let off = self.heap.len() as u32;
        self.heap.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.heap.extend_from_slice(s.as_bytes());
        while self.heap.len() % 4 != 0 {
            self.heap.push(0);
        }
        off
    }
    pub(crate) fn op(&mut self, code: u8, a: u16, b: u32, c: u32, d: u32) {
        let mut r = [0u8; 16];
        r[0] = code;
        r[2..4].copy_from_slice(&a.to_le_bytes());
        r[4..8].copy_from_slice(&b.to_le_bytes());
        r[8..12].copy_from_slice(&c.to_le_bytes());
        r[12..16].copy_from_slice(&d.to_le_bytes());
        self.ops.push(r);
    }
    pub(crate) fn finish(&self) -> Vec<u8> {
        let heap_off = 16 + 16 * self.ops.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"GWB1");
        out.extend_from_slice(&(self.ops.len() as u32).to_le_bytes());
        out.extend_from_slice(&(heap_off as u32).to_le_bytes());
        out.extend_from_slice(&(self.heap.len() as u32).to_le_bytes());
        for r in &self.ops {
            out.extend_from_slice(r);
        }
        out.extend_from_slice(&self.heap);
        out
    }
}

#[test]
fn decode_roundtrip() {
    let mut e = Enc::new();
    let t = e.s("hello");
    e.op(1, 0, 2, 1, NO_STR);
    e.op(2, 0, 3, 0, t);
    e.op(8, 0, 2, 3, 0);
    e.op(15, 4, 2, 0, 0);
    let ops = decode_batch(&e.finish()).unwrap();
    assert_eq!(ops.len(), 4);
    assert!(matches!(&ops[1], Op::CreateText { id: 3, text } if text == "hello"));
    assert!(matches!(ops[3], Op::Listen { id: 2, kind: 4 }));
}

#[test]
fn decode_rejects_bad_bounds() {
    let mut e = Enc::new();
    e.op(1, 0, 2, 1, NO_STR);
    let good = e.finish();
    // op_count larger than the buffer / than the cap
    for count in [2u32, u32::MAX, (MAX_OPS_PER_BATCH + 1) as u32] {
        let mut b = good.clone();
        b[4..8].copy_from_slice(&count.to_le_bytes());
        assert!(decode_batch(&b).is_err(), "count {count}");
    }
    // heap that overflows usize arithmetic / runs past the end
    for (off, len) in [(u32::MAX, 8u32), (16, u32::MAX), (u32::MAX, u32::MAX)] {
        let mut b = good.clone();
        b[8..12].copy_from_slice(&off.to_le_bytes());
        b[12..16].copy_from_slice(&len.to_le_bytes());
        assert!(decode_batch(&b).is_err(), "heap {off}+{len}");
    }
    // string offsets that point outside the heap
    for d in [100u32, u32::MAX - 1, u32::MAX - 3] {
        let mut e = Enc::new();
        e.s("ab");
        e.op(2, 0, 3, 0, d);
        assert!(decode_batch(&e.finish()).is_err(), "str {d}");
    }
    // string length that runs past the heap
    let mut e = Enc::new();
    let s = e.s("abc");
    e.heap[s as usize..s as usize + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    e.op(2, 0, 3, 0, s);
    assert!(decode_batch(&e.finish()).is_err());
}

#[test]
fn decode_limits_atom_range() {
    for (atom, ok) in [
        (0u32, false),
        (100, false),
        (1023, false),
        (1024, true),
        (MAX_ATOMS - 1, true),
        (MAX_ATOMS, false),
        (u32::MAX, false),
    ] {
        let mut e = Enc::new();
        let s = e.s("x-name");
        e.op(14, 0, atom, 0, s);
        assert_eq!(decode_batch(&e.finish()).is_ok(), ok, "atom {atom}");
    }
}

/// One canonical line per op, matching testdata/abi/all-ops.txt.
fn op_line(op: &Op) -> String {
    match op {
        Op::CreateElement { id, tag } => format!("CreateElement id={id} tag={tag}"),
        Op::CreateText { id, text } => format!("CreateText id={id} text={text:?}"),
        Op::SetAttr { id, name, value } => format!("SetAttr id={id} name={name} value={value:?}"),
        Op::RemoveAttr { id, name } => format!("RemoveAttr id={id} name={name}"),
        Op::SetText { id, text } => format!("SetText id={id} text={text:?}"),
        Op::SetStyle { id, prop, value } => format!("SetStyle id={id} prop={prop} value={value:?}"),
        Op::RemoveStyle { id, prop } => format!("RemoveStyle id={id} prop={prop}"),
        Op::AppendChild { parent, child } => format!("AppendChild parent={parent} child={child}"),
        Op::InsertBefore { child, before } => format!("InsertBefore child={child} before={before}"),
        Op::Remove { id } => format!("Remove id={id}"),
        Op::ReplaceWith { old, new } => format!("ReplaceWith old={old} new={new}"),
        Op::Clear { id } => format!("Clear id={id}"),
        Op::SetInnerHtml { id, html } => format!("SetInnerHtml id={id} html={html:?}"),
        Op::DefineAtom { atom, name } => format!("DefineAtom atom={atom} name={name:?}"),
        Op::Listen { id, kind } => format!("Listen id={id} kind={kind}"),
        Op::Unlisten { id, kind } => format!("Unlisten id={id} kind={kind}"),
        Op::Observe { id, what } => format!("Observe id={id} what={what}"),
        Op::Unobserve { id, what } => format!("Unobserve id={id} what={what}"),
        Op::Focus { id } => format!("Focus id={id}"),
    }
}

/// Shared GWB1 conformance vector: produced by the Go SDK encoder
/// (sdk/gwb/conformance_test.go), decoded here. Both sides must agree.
#[test]
fn conformance_vectors_from_go_sdk() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/abi");
    let bin = std::fs::read(dir.join("all-ops.bin")).expect("testdata/abi/all-ops.bin");
    let txt = std::fs::read_to_string(dir.join("all-ops.txt")).expect("testdata/abi/all-ops.txt");
    let ops = decode_batch(&bin).expect("vector decodes");
    let got: Vec<String> = ops.iter().map(op_line).collect();
    let want: Vec<&str> = txt.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(got, want);
    // Every op code in the ABI is covered by the vector.
    let mut codes: Vec<u8> = (0..ops.len()).map(|i| bin[16 + 16 * i]).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes, (1..=19).collect::<Vec<u8>>());
}

/// Deterministic fuzz: random buffers and mutated valid batches must never
/// panic the decoder (it runs inside a host import).
#[test]
fn decode_never_panics() {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut seed = Enc::new();
    let s = seed.s("seed text");
    seed.op(1, 0, 2, 1, NO_STR);
    seed.op(2, 0, 3, 0, s);
    seed.op(3, 0, 2, 100, s);
    seed.op(14, 0, 1024, 0, s);
    let valid = seed.finish();
    for i in 0..50_000 {
        let buf: Vec<u8> = if i % 2 == 0 {
            let len = (rnd() % 256) as usize;
            let mut b: Vec<u8> = (0..len).map(|_| rnd() as u8).collect();
            if b.len() >= 4 && rnd() % 2 == 0 {
                b[..4].copy_from_slice(b"GWB1");
            }
            b
        } else {
            let mut b = valid.clone();
            for _ in 0..(1 + rnd() % 4) {
                let at = (rnd() as usize) % b.len();
                b[at] = rnd() as u8;
            }
            if rnd() % 8 == 0 {
                b.truncate((rnd() as usize) % b.len());
            }
            b
        };
        let _ = decode_batch(&buf);
    }
}

#[test]
fn guest_bytes_is_overflow_safe() {
    let data = [0u8; 16];
    assert!(guest_bytes(&data, 0, 16).is_some());
    assert!(guest_bytes(&data, 8, 9).is_none());
    assert!(guest_bytes(&data, u32::MAX, u32::MAX).is_none());
    assert!(guest_bytes(&data, 16, 0).is_some());
    assert!(guest_bytes(&data, 17, 0).is_none());
}

#[test]
fn rpc_method_names() {
    for ok in ["list", "setQty", "upsert_product", "a-b", "x1"] {
        assert!(valid_method(ok), "{ok}");
    }
    let long = "x".repeat(65);
    for bad in ["", "../admin", "a/b", "a?x=1", "a#b", "a.b", "a b", "%2e%2e", long.as_str()] {
        assert!(!valid_method(bad), "{bad}");
    }
}

// ---- wasm resource limits --------------------------------------------------

#[test]
fn runaway_guest_is_interrupted() {
    let engine = new_engine().unwrap();
    spawn_epoch_ticker(engine.clone());
    let module = Module::new(&engine, r#"(module (func (export "spin") (loop br 0)))"#).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_epoch_deadline(5); // ~50 ms
    let inst = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let spin = inst.get_typed_func::<(), ()>(&mut store, "spin").unwrap();
    let started = Instant::now();
    let err: anyhow::Error = spin.call(&mut store, ()).unwrap_err().into();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let why = guest_fault(&err).expect("an interrupt is a guest fault");
    assert!(why.contains("CPU"), "{why}");
    assert!(guest_fault(&anyhow!("host-side problem")).is_none());
}

#[test]
fn guest_memory_is_capped() {
    let engine = new_engine().unwrap();
    let module = Module::new(
        &engine,
        r#"(module (memory 1)
             (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0))))"#,
    )
    .unwrap();
    struct S {
        limits: StoreLimits,
    }
    let mut store = Store::new(&engine, S { limits: guest_store_limits() });
    store.limiter(|s| &mut s.limits);
    store.set_epoch_deadline(u64::MAX / 2);
    let inst = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let grow = inst.get_typed_func::<i32, i32>(&mut store, "grow").unwrap();
    let pages_over = (MAX_GUEST_MEMORY / 65536) as i32; // 1 page already in use
    assert_eq!(grow.call(&mut store, pages_over).unwrap(), -1, "growth past the cap fails");
    assert_eq!(grow.call(&mut store, 10).unwrap(), 1, "modest growth still works");
}

// ---- apply_batches against a real Blitz document ----------------------------

struct Harness {
    doc: blitz_html::HtmlDocument,
    maps: NodeMaps,
    shared: Arc<Mutex<Shared>>,
    mount: usize,
    styles: usize,
    chrome: usize,
}

impl Harness {
    fn new() -> Self {
        let doc = blitz_html::HtmlDocument::from_html(
            r#"<!DOCTYPE html><html><body>
               <div id="chrome">chrome</div>
               <div id="styles"></div>
               <div class="gwb-scope-t0"><div id="mount"></div></div>
               </body></html>"#,
            blitz_dom::DocumentConfig {
                html_parser_provider: Some(Arc::new(blitz_html::HtmlProvider)),
                base_url: Some(DOC_BASE_URL.to_string()),
                ..Default::default()
            },
        );
        let q = |s: &str| doc.query_selector(s).unwrap().unwrap();
        let (mount, styles, chrome) = (q("#mount"), q("#styles"), q("#chrome"));
        let maps = NodeMaps::for_mount(
            mount,
            StyleScope { mount, styles_host: styles, prefix: ".gwb-scope-t0>*".to_string() },
        );
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let shared = Arc::new(Mutex::new(Shared {
            atoms: well_known_atoms(),
            batches: Vec::new(),
            event_region: (0, 0),
            next_fetch_id: 0,
            frame_requested: false,
            window_id: None,
            services: HashMap::new(),
            next_rpc_id: 0,
            session_token: None,
            app_key: Arc::new(key),
            app_key_b64: String::new(),
            links: HashSet::new(),
            gesture_active: false,
            tab_id: 0,
        }));
        Harness { doc, maps, shared, mount, styles, chrome }
    }

    fn apply(&mut self, ops: Vec<Op>) {
        self.shared.lock_ok().batches.push(ops);
        apply_batches(&mut self.doc, &mut self.maps, &self.shared);
    }

    fn node(&self, gid: u32) -> Option<&blitz_dom::Node> {
        self.maps.fwd.get(&gid).and_then(|&n| self.doc.get_node(n))
    }

    fn tag(&self, nid: usize) -> String {
        self.doc
            .get_node(nid)
            .and_then(|n| n.element_data())
            .map(|e| e.name.local.to_string())
            .unwrap_or_default()
    }

    fn all_tags_under(&self, root: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(n) = self.doc.get_node(id) {
                stack.extend_from_slice(&n.children);
                if id != root && n.element_data().is_some() {
                    out.push(self.tag(id));
                }
            }
        }
        out
    }

    fn shadow_css(&self) -> String {
        self.doc.get_node(self.styles).unwrap().text_content()
    }
}

const STYLE_ATOM: u32 = 1030;

#[test]
fn guest_style_is_inert_and_scoped() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::DefineAtom { atom: STYLE_ATOM, name: "style".into() },
        Op::CreateElement { id: 2, tag: STYLE_ATOM },
        Op::SetInnerHtml { id: 2, html: "body{margin:0} #ab-input{display:none} .x{color:red}".into() },
        Op::AppendChild { parent: 1, child: 2 },
    ]);
    // The guest's element is the inert stand-in, not a <style>.
    assert_eq!(h.tag(h.maps.fwd[&2]), GUEST_STYLE_TAG);
    assert!(!h.all_tags_under(h.mount).contains(&"style".to_string()));
    // The live sheet is host-owned and fully scoped.
    let css = h.shadow_css();
    assert!(css.contains(".gwb-scope-t0>*{margin:0}"), "{css}");
    assert!(css.contains(".gwb-scope-t0>* #ab-input{display:none}"), "{css}");
    assert!(css.contains(".gwb-scope-t0>* .x{color:red}"), "{css}");

    // Text updates through a text child (gwbc's path) re-scope.
    h.apply(vec![
        Op::CreateElement { id: 3, tag: STYLE_ATOM },
        Op::CreateText { id: 4, text: ".a{b:c}".into() },
        Op::AppendChild { parent: 3, child: 4 },
        Op::AppendChild { parent: 1, child: 3 },
    ]);
    assert!(h.shadow_css().contains(".gwb-scope-t0>* .a{b:c}"));
    h.apply(vec![Op::SetText { id: 4, text: ".changed{b:c}".into() }]);
    let css = h.shadow_css();
    assert!(css.contains(".changed") && !css.contains(".gwb-scope-t0>* .a{"), "{css}");

    // Removing the guest element unloads its sheet; Clear does too.
    h.apply(vec![Op::Remove { id: 3 }]);
    assert!(!h.shadow_css().contains(".changed"));
    h.apply(vec![Op::Clear { id: 1 }]);
    assert!(h.shadow_css().trim().is_empty(), "{}", h.shadow_css());
}

#[test]
fn guest_style_created_and_removed_in_one_batch_is_safe() {
    // A live <style> dropped in the same mutator session used to panic at
    // flush; the inert stand-in + reusable shadows make this a no-op.
    let mut h = Harness::new();
    h.apply(vec![
        Op::DefineAtom { atom: STYLE_ATOM, name: "style".into() },
        Op::CreateElement { id: 2, tag: STYLE_ATOM },
        Op::SetInnerHtml { id: 2, html: ".x{y:z}".into() },
        Op::AppendChild { parent: 1, child: 2 },
        Op::Remove { id: 2 },
        Op::CreateElement { id: 3, tag: STYLE_ATOM },
        Op::AppendChild { parent: 1, child: 3 },
        Op::SetInnerHtml { id: 3, html: ".y{y:z}".into() },
    ]);
    let css = h.shadow_css();
    assert!(css.contains(".y") && !css.contains(".x{"), "{css}");
}

#[test]
fn detached_guest_style_does_not_apply() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::DefineAtom { atom: STYLE_ATOM, name: "style".into() },
        Op::CreateElement { id: 2, tag: STYLE_ATOM },
        Op::SetInnerHtml { id: 2, html: ".x{y:z}".into() },
    ]);
    assert!(h.shadow_css().is_empty());
    h.apply(vec![Op::AppendChild { parent: 1, child: 2 }]);
    assert!(h.shadow_css().contains(".x"));
}

#[test]
fn entity_encoded_braces_stay_scoped() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::DefineAtom { atom: STYLE_ATOM, name: "style".into() },
        Op::CreateElement { id: 2, tag: STYLE_ATOM },
        Op::SetInnerHtml { id: 2, html: ".x{a:b&#125; #chrome{display:none}".into() },
        Op::AppendChild { parent: 1, child: 2 },
    ]);
    // What Blitz parses is the entity-decoded shadow text.
    let engine_sees = html_escape::decode_html_entities(&h.shadow_css()).into_owned();
    assert!(!engine_sees.contains("}#chrome") && !engine_sees.contains("} #chrome"), "{engine_sees}");
    assert!(engine_sees.contains(".gwb-scope-t0>* #chrome"), "{engine_sees}");
}

#[test]
fn denied_tags_are_not_created() {
    let mut h = Harness::new();
    let mut ops = Vec::new();
    for (i, t) in ["script", "link", "title", "meta", "base", "iframe", "TITLE"].iter().enumerate() {
        let atom = 2000 + i as u32;
        ops.push(Op::DefineAtom { atom, name: t.to_string() });
        ops.push(Op::CreateElement { id: 10 + i as u32, tag: atom });
        ops.push(Op::AppendChild { parent: 1, child: 10 + i as u32 });
    }
    h.apply(ops);
    assert!(h.all_tags_under(h.mount).is_empty(), "{:?}", h.all_tags_under(h.mount));
}

#[test]
fn inner_html_is_sanitized() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 1 },
        Op::AppendChild { parent: 1, child: 2 },
        Op::SetInnerHtml {
            id: 2,
            html: r#"<p class="ok">hi <b>there</b></p><style>#chrome{display:none}</style>
                     <link rel=stylesheet href="http://evil/x.css"><title>spoof</title>
                     <script>x</script><svg><style>#chrome{color:red}</style></svg>
                     <iframe src="http://evil"></iframe><img src="a.png" alt="x">"#
                .into(),
        },
    ]);
    let tags = h.all_tags_under(h.mount);
    for bad in ["style", "link", "title", "script", "iframe"] {
        assert!(!tags.contains(&bad.to_string()), "{bad} in {tags:?}");
    }
    for good in ["p", "b", "img", "svg"] {
        assert!(tags.contains(&good.to_string()), "{good} missing from {tags:?}");
    }
    let p = h.doc.query_selector("#mount p.ok").unwrap();
    assert!(p.is_some(), "attributes are preserved");
}

#[test]
fn url_attributes_cannot_crash_the_host() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 10 }, // img
        Op::SetAttr { id: 2, name: 107, value: "a.png".into() },
        Op::CreateElement { id: 3, tag: 10 },
        Op::SetAttr { id: 3, name: 107, value: "http://a:99999999/x.png".into() },
        Op::CreateElement { id: 4, tag: 9 }, // a
        Op::SetAttr { id: 4, name: 103, value: "http://[::1".into() },
        Op::AppendChild { parent: 1, child: 2 },
        Op::AppendChild { parent: 1, child: 3 },
        Op::AppendChild { parent: 1, child: 4 },
    ]);
    let attr = |gid: u32, a: &str| {
        h.node(gid)
            .and_then(|n| n.attrs())
            .and_then(|attrs| attrs.iter().find(|x| x.name.local.as_ref() == a).map(|x| x.value.clone()))
    };
    assert_eq!(attr(2, "src").as_deref(), Some("a.png"));
    assert_eq!(attr(3, "src"), None, "unresolvable URL dropped");
    assert_eq!(attr(4, "href"), None, "unresolvable URL dropped");
}

#[test]
fn form_control_attached_and_removed_in_one_batch() {
    // Upstream Blitz queues form-owner reset at attach and runs it at flush;
    // dropping the node in between panicked ("form.rs: invalid key").
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 7 }, // button
        Op::CreateElement { id: 3, tag: 8 }, // input
        Op::AppendChild { parent: 1, child: 2 },
        Op::AppendChild { parent: 1, child: 3 },
        Op::Remove { id: 2 },
        Op::Clear { id: 1 },
    ]);
    assert!(h.doc.get_node(h.mount).unwrap().children.is_empty());
}

#[test]
fn removing_the_focused_node_then_refocusing() {
    // Blitz kept focus_node_id on the dropped node; the next focus change
    // blurred (snapshotted) the freed slot and panicked.
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 8 },
        Op::CreateElement { id: 3, tag: 8 },
        Op::AppendChild { parent: 1, child: 2 },
        Op::AppendChild { parent: 1, child: 3 },
        Op::Focus { id: 2 },
    ]);
    let removed = h.maps.fwd[&2];
    assert_eq!(h.doc.get_focussed_node_id(), Some(removed));
    h.apply(vec![Op::Remove { id: 2 }]);
    // (With nothing focused Blitz reports the root element.)
    assert_ne!(h.doc.get_focussed_node_id(), Some(removed));
    h.apply(vec![Op::Focus { id: 3 }]);
    assert_eq!(h.doc.get_focussed_node_id(), Some(h.maps.fwd[&3]));
    // Focusing a node removed later in the same batch is a no-op, not a panic.
    h.apply(vec![
        Op::CreateElement { id: 4, tag: 8 },
        Op::AppendChild { parent: 1, child: 4 },
        Op::Focus { id: 4 },
        Op::Clear { id: 1 },
    ]);
}

#[test]
fn mount_cannot_be_removed_moved_or_recreated() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 1 },
        Op::AppendChild { parent: 1, child: 2 },
        Op::Remove { id: 1 },
        Op::AppendChild { parent: 2, child: 1 },
        Op::ReplaceWith { old: 1, new: 2 },
        Op::InsertBefore { child: 1, before: 2 },
        Op::CreateElement { id: 1, tag: 1 },
    ]);
    assert_eq!(h.maps.fwd[&1], h.mount);
    let mount = h.doc.get_node(h.mount).expect("mount still exists");
    assert!(mount.parent.is_some(), "mount still attached");
    assert_eq!(mount.children.len(), 1);
}

#[test]
fn cycles_and_detached_anchors_are_refused() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 1 },
        Op::CreateElement { id: 3, tag: 1 },
        Op::CreateElement { id: 4, tag: 1 },
        Op::AppendChild { parent: 1, child: 2 },
        Op::AppendChild { parent: 2, child: 3 },
        // cycle: 2 into its own child
        Op::AppendChild { parent: 3, child: 2 },
        // self-append
        Op::AppendChild { parent: 3, child: 3 },
        // detached anchor (4 has no parent) used to panic in Blitz
        Op::InsertBefore { child: 3, before: 4 },
        Op::ReplaceWith { old: 4, new: 3 },
    ]);
    assert_eq!(h.node(2).unwrap().parent, Some(h.mount));
    assert_eq!(h.node(3).unwrap().parent, Some(h.maps.fwd[&2]));
}

#[test]
fn removed_subtree_ids_are_forgotten() {
    let mut h = Harness::new();
    h.apply(vec![
        Op::CreateElement { id: 2, tag: 1 },
        Op::CreateElement { id: 3, tag: 1 },
        Op::CreateText { id: 4, text: "x".into() },
        Op::AppendChild { parent: 1, child: 2 },
        Op::AppendChild { parent: 2, child: 3 },
        Op::AppendChild { parent: 3, child: 4 },
        Op::Remove { id: 2 },
    ]);
    for gid in [2, 3, 4] {
        assert!(!h.maps.fwd.contains_key(&gid), "guest id {gid} still mapped");
    }
    // Old ids must not reach whatever reuses the freed slots.
    let before = h.doc.get_node(h.chrome).unwrap().text_content();
    h.apply(vec![
        Op::SetText { id: 4, text: "pwned".into() },
        Op::SetAttr { id: 3, name: 101, value: "pwned".into() },
        Op::AppendChild { parent: 3, child: 1 },
    ]);
    assert_eq!(h.doc.get_node(h.chrome).unwrap().text_content(), before);
    // Clear forgets descendants too (but not the cleared node).
    h.apply(vec![
        Op::CreateElement { id: 5, tag: 1 },
        Op::CreateElement { id: 6, tag: 1 },
        Op::AppendChild { parent: 1, child: 5 },
        Op::AppendChild { parent: 5, child: 6 },
        Op::Clear { id: 5 },
    ]);
    assert!(h.maps.fwd.contains_key(&5));
    assert!(!h.maps.fwd.contains_key(&6));
}
