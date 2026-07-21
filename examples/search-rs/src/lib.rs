//! search-rs: a small search-engine frontend proving a real (non-toy),
//! RPC-backed wasm guest authored entirely in Rust — the sdk-rust analogue of
//! examples/shop-c, exercising the RPC/fetch bindings added to sdk-rust for
//! this demo (see sdk-rust/src/lib.rs).
//!
//! Styled via an injected `<style>` element (same technique shop-c uses) —
//! a real class-based stylesheet rather than per-element inline styles, so
//! hover states and a considered visual identity are actually possible.

use std::cell::RefCell;

use gwb::{Batch, Event, atom, ev, new_id};
use serde::{Deserialize, Serialize};

/// "style" isn't a well-known atom (document-level tags are deliberately
/// absent from the well-known table) — define it once, like todo-rs/-go
/// define their own custom atoms for properties outside the base set.
const ATOM_STYLE: u32 = 1030;

const CSS: &str = r#"
.ws{width:100%;min-height:100%;background:#0b0c09;color:#d7ddc9;
    font-family:ui-monospace,'Cascadia Mono','SF Mono',Consolas,monospace;
    padding:40px 44px}
.ws-wrap{max-width:820px;margin:0 auto}
.ws-head{display:flex;align-items:baseline;gap:2px;margin:0 0 6px}
.ws-title{font-size:26px;font-weight:700;color:#ffb400;letter-spacing:-.01em}
.ws-cursor{color:#ffb400}
.ws-sub{color:#71785f;font-size:12.5px;margin:0 0 26px;max-width:620px;line-height:1.6}
.ws-bar{display:flex;border:1px solid #2a2f22;background:#101208;max-width:620px}
.ws-input{flex:1;min-width:0;background:transparent;border:none;color:#d7ddc9;
          font-family:inherit;font-size:13.5px;padding:12px 14px}
.ws-go{background:#ffb400;color:#0b0c09;border:none;font-family:inherit;
       font-weight:700;font-size:12.5px;padding:0 22px;cursor:pointer;letter-spacing:.06em}
.ws-go:hover{background:#ffc23d}
.ws-status{color:#565c47;font-size:11.5px;margin:18px 0 6px;letter-spacing:.03em;
           text-transform:uppercase}
.ws-rec{display:flex;gap:16px;padding:15px 0;border-bottom:1px dashed #21251a}
.ws-rank{flex:0 0 30px;color:#565c47;font-size:13px;padding-top:1px}
.ws-body{flex:1;min-width:0}
.ws-rtitle{color:#ffcf66;font-size:14.5px;font-weight:700;margin:0 0 3px 0}
.ws-rmeta{color:#565c47;font-size:11px;margin:0 0 6px 0;letter-spacing:.02em}
.ws-rsnip{color:#a6ab95;font-size:13px;line-height:1.55;margin:0}
.ws-empty{color:#71785f;font-size:13px;padding:8px 0}
.ws-links{display:flex;gap:18px;margin:34px 0 0;padding:16px 0 0;border-top:1px solid #21251a}
.ws-link{background:transparent;border:none;color:#8a6a1f;font-family:inherit;font-size:12px;
         cursor:pointer;letter-spacing:.02em;padding:0}
.ws-link:hover{color:#ffb400}
"#;

#[derive(Serialize)]
struct SearchReq<'a> {
    q: &'a str,
}

#[derive(Deserialize)]
struct SearchHit {
    title: String,
    url: String,
    snippet: String,
    score: u32,
}

#[derive(Deserialize)]
struct SearchResp {
    query: String,
    took_ms: u64,
    results: Vec<SearchHit>,
}

#[derive(Default)]
struct App {
    batch: Batch,
    input: u32,
    go_btn: u32,
    link_shop: u32,
    link_retailer: u32,
    /// The status line's inner TEXT NODE (not its wrapping element —
    /// `Batch::set_text` targets a text node directly; calling it on the
    /// element silently no-ops instead of replacing the child's content).
    status_text: u32,
    results: u32,
    query_value: String,
    /// The in-flight request id, if any — guards against a stale/duplicate
    /// RPC_RESULT (e.g. a second search fired before the first replied).
    pending_req_id: Option<u32>,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App::default());
}

impl App {
    fn el(&mut self, tag: u32, class: &str) -> u32 {
        let id = new_id();
        self.batch.create_element(id, tag);
        if !class.is_empty() {
            self.batch.set_attr(id, atom::ATTR_CLASS, class);
        }
        id
    }

    /// Element with a plain text child; returns (element, text-node).
    fn text_el(&mut self, tag: u32, class: &str, text: &str) -> (u32, u32) {
        let id = self.el(tag, class);
        let t = new_id();
        self.batch.create_text(t, text);
        self.batch.append_child(id, t);
        (id, t)
    }

    fn start(&mut self) {
        self.batch.define_atom(ATOM_STYLE, "style");
        let style = self.el(ATOM_STYLE, "");
        self.batch.set_inner_html(style, CSS);
        self.batch.append_child(gwb::ROOT, style);

        let card = self.el(atom::DIV, "ws");
        self.batch.append_child(gwb::ROOT, card);

        let wrap = self.el(atom::DIV, "ws-wrap");
        self.batch.append_child(card, wrap);

        let head = self.el(atom::DIV, "ws-head");
        self.batch.append_child(wrap, head);
        let (title_span, _) = self.text_el(atom::SPAN, "ws-title", "wasm-search");
        self.batch.append_child(head, title_span);
        let (cursor, _) = self.text_el(atom::SPAN, "ws-cursor", "_");
        self.batch.append_child(head, cursor);

        let (sub, _) = self.text_el(
            atom::P,
            "ws-sub",
            "A search engine's frontend, written entirely in Rust, talking to a Rust RPC backend on :8788. Indexes a small corpus about how this browser works.",
        );
        self.batch.append_child(wrap, sub);

        let bar = self.el(atom::DIV, "ws-bar");
        self.batch.append_child(wrap, bar);

        self.input = self.el(atom::INPUT, "ws-input");
        self.batch.set_attr(self.input, atom::ATTR_ID, "q");
        self.batch.set_attr(self.input, atom::ATTR_TYPE, "text");
        self.batch.set_attr(self.input, atom::ATTR_PLACEHOLDER, "wasm, dom, rpc, capabilities...");
        self.batch.append_child(bar, self.input);
        self.batch.listen(self.input, ev::INPUT);
        self.batch.listen(self.input, ev::KEY_DOWN);

        let (go, _) = self.text_el(atom::BUTTON, "ws-go", "search");
        self.go_btn = go;
        self.batch.set_attr(self.go_btn, atom::ATTR_ID, "search-btn");
        self.batch.append_child(bar, self.go_btn);
        self.batch.listen(self.go_btn, ev::CLICK);

        let status = self.el(atom::P, "ws-status");
        self.status_text = new_id();
        self.batch.create_text(self.status_text, "ready");
        self.batch.append_child(status, self.status_text);
        self.batch.append_child(wrap, status);

        self.results = self.el(atom::DIV, "");
        self.batch.append_child(wrap, self.results);

        // Cross-site links (docs/04-WEB-RPC.md-style capability model, but for
        // navigation: gwb::navigate only succeeds if "shop.local"/"retailer.local"
        // are in this app's manifest "links" array — see manifests/search.local.json).
        let links = self.el(atom::DIV, "ws-links");
        self.batch.append_child(wrap, links);
        let (shop_link, _) = self.text_el(atom::BUTTON, "ws-link", "\u{2192} browse the Aurelia storefront");
        self.link_shop = shop_link;
        self.batch.set_attr(self.link_shop, atom::ATTR_ID, "link-shop");
        self.batch.append_child(links, shop_link);
        self.batch.listen(self.link_shop, ev::CLICK);
        let (retailer_link, _) = self.text_el(atom::BUTTON, "ws-link", "\u{2192} circuit. electronics (new tab)");
        self.link_retailer = retailer_link;
        self.batch.set_attr(self.link_retailer, atom::ATTR_ID, "link-retailer");
        self.batch.append_child(links, retailer_link);
        self.batch.listen(self.link_retailer, ev::CLICK);
    }

    fn set_status(&mut self, text: &str) {
        self.batch.set_text(self.status_text, text);
    }

    fn submit_search(&mut self) {
        let q = self.query_value.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.set_status("searching...");
        let payload = serde_json::to_vec(&SearchReq { q: &q }).unwrap_or_default();
        let req_id = gwb::rpc("search", "search.query.v1", "search", &payload, 0);
        if req_id == 0 {
            self.set_status("search unavailable (service undeclared)");
            return;
        }
        self.pending_req_id = Some(req_id);
    }

    fn render_results(&mut self, resp: SearchResp) {
        self.batch.clear(self.results);
        self.set_status(&format!(
            "{} result{} for \"{}\" in {}ms",
            resp.results.len(),
            if resp.results.len() == 1 { "" } else { "s" },
            resp.query,
            resp.took_ms
        ));
        for (i, hit) in resp.results.iter().enumerate() {
            let row = self.el(atom::DIV, "ws-rec");
            self.batch.append_child(self.results, row);

            let (rank, _) = self.text_el(atom::SPAN, "ws-rank", &format!("[{:02}]", i + 1));
            self.batch.append_child(row, rank);

            let body = self.el(atom::DIV, "ws-body");
            self.batch.append_child(row, body);

            let (title, _) = self.text_el(atom::H3, "ws-rtitle", &hit.title);
            self.batch.append_child(body, title);

            let (meta, _) = self.text_el(atom::SPAN, "ws-rmeta", &format!("{}  ·  relevance {}", hit.url, hit.score));
            self.batch.append_child(body, meta);

            let (snip, _) = self.text_el(atom::P, "ws-rsnip", &hit.snippet);
            self.batch.append_child(body, snip);
        }
        if resp.results.is_empty() {
            let (empty, _) = self.text_el(atom::P, "ws-empty", "No matches. Try \"wasm\", \"rpc\", \"capabilities\", or \"dom\".");
            self.batch.append_child(self.results, empty);
        }
    }

    fn on_event(&mut self, e: &Event) -> u32 {
        match e.kind {
            ev::INPUT if e.listener == self.input => {
                self.query_value = e.text.clone();
            }
            ev::KEY_DOWN if e.listener == self.input && e.pressed && e.text == "Enter" => {
                self.submit_search();
            }
            ev::CLICK if e.listener == self.go_btn => {
                self.submit_search();
            }
            ev::CLICK if e.listener == self.link_shop => {
                let code = gwb::navigate("web://shop.local", 0);
                if code != 0 {
                    self.set_status(&format!("navigate blocked (code {code})"));
                }
            }
            ev::CLICK if e.listener == self.link_retailer => {
                let code = gwb::navigate("web://retailer.local", gwb::NAV_F_NEW_TAB);
                if code != 0 {
                    self.set_status(&format!("navigate blocked (code {code})"));
                }
            }
            ev::RPC_RESULT if Some(e.rpc_req_id) == self.pending_req_id => {
                self.pending_req_id = None;
                if !e.rpc_ok {
                    self.set_status(&format!("search failed (err_class={})", e.rpc_err_class));
                    return 0;
                }
                match serde_json::from_str::<SearchResp>(&e.text) {
                    Ok(resp) => self.render_results(resp),
                    Err(err) => self.set_status(&format!("bad response: {err}")),
                }
            }
            _ => {}
        }
        0
    }
}

// ---------------------------------------------------------------- exports

#[unsafe(no_mangle)]
extern "C" fn gwb_abi_version() -> u32 {
    1 << 16
}

#[unsafe(no_mangle)]
extern "C" fn gwb_start(_w: f32, _h: f32, _scale: f32, _flags: u32) {
    gwb::register_event_region();
    APP.with_borrow_mut(|app| {
        app.start();
        app.batch.submit();
    });
}

#[unsafe(no_mangle)]
extern "C" fn gwb_events(count: u32) -> u32 {
    APP.with_borrow_mut(|app| {
        let ret = gwb::decode_events(count, |e| app.on_event(e));
        app.batch.submit();
        ret
    })
}

#[unsafe(no_mangle)]
extern "C" fn gwb_frame(_dt_ms: f32) {}
