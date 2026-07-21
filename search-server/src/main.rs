//! search-server — the reference RPC backend for the search-rs demo.
//!
//! One RPC method: `search.query.v1/search`. Public (no login/session — a
//! search engine has nothing to authenticate a user against), but every
//! request still travels the same host-mediated, channel-signed envelope as
//! shop's server (docs/04-WEB-RPC.md §2): the renderer signs on the guest's
//! behalf with its app-scoped ed25519 key, and this server verifies it.
//! Mirrors `server/auth.go`'s channel-authn logic in Rust.

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Response, Server};

const SIG_MAX_SKEW_MS: i64 = 60_000; // ±60s replay window, matches server/auth.go

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// canonical = iface\nmethod\nreq_id\nts\nhex(sha256(body)) — identical bytes
/// to what the renderer's rpc_call thread signs (renderer/src/abi.rs) and what
/// server/auth.go verifies. Any compliant GWB host/server pair interop here.
fn canonical_bytes(iface: &str, method: &str, req_id: &str, ts: &str, body: &[u8]) -> Vec<u8> {
    let hash = hex::encode(Sha256::digest(body));
    format!("{iface}\n{method}\n{req_id}\n{ts}\n{hash}").into_bytes()
}

struct RpcError {
    status: u16,
    body: &'static str,
}
fn err(status: u16, body: &'static str) -> RpcError {
    RpcError { status, body }
}

/// Verifies the channel signature (§2 of 04-WEB-RPC.md). No server-side key
/// is needed for this check — the caller's pubkey travels in GWB-App-Key and
/// the signature is verified against it, exactly like server/auth.go's
/// verifyChannel (which also carries no fixed keypair — only session tokens,
/// unused here, need one).
fn verify_channel(
    require_channel: bool,
    iface: &str,
    method: &str,
    req_id: &str,
    app_key_b64: &str,
    sig_b64: &str,
    ts_str: &str,
    body: &[u8],
) -> Result<(), RpcError> {
    if !require_channel && app_key_b64.is_empty() {
        return Ok(()); // dev/tests: unsigned allowed
    }
    if app_key_b64.is_empty() || sig_b64.is_empty() || ts_str.is_empty() {
        return Err(err(401, "missing channel auth (GWB-App-Key/Sig/Ts)"));
    }
    let ts: i64 = ts_str.parse().map_err(|_| err(401, "bad GWB-Ts"))?;
    let skew = now_ms() - ts;
    if skew > SIG_MAX_SKEW_MS || skew < -SIG_MAX_SKEW_MS {
        return Err(err(401, "stale GWB-Ts (replay window)"));
    }
    let pub_bytes = base64::engine::general_purpose::STANDARD
        .decode(app_key_b64)
        .map_err(|_| err(401, "bad GWB-App-Key"))?;
    let pub_bytes: [u8; 32] = pub_bytes.try_into().map_err(|_| err(401, "bad GWB-App-Key"))?;
    let vk = VerifyingKey::from_bytes(&pub_bytes).map_err(|_| err(401, "bad GWB-App-Key"))?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|_| err(401, "bad GWB-Sig"))?;
    let sig_bytes: [u8; 64] = sig_bytes.try_into().map_err(|_| err(401, "bad GWB-Sig"))?;
    let sig = Signature::from_bytes(&sig_bytes);
    let msg = canonical_bytes(iface, method, req_id, ts_str, body);
    vk.verify_strict(&msg, &sig)
        .map_err(|_| err(401, "channel signature does not verify"))
}

// ---------------------------------------------------------------- corpus

struct Doc {
    title: &'static str,
    url: &'static str,
    body: &'static str,
}

/// A small in-memory corpus about browsers/wasm/the web platform — thematically
/// fitting for a demo living inside a browser research project.
const CORPUS: &[Doc] = &[
    Doc { title: "What WebAssembly actually is", url: "wasm://intro", body: "WebAssembly wasm is a binary instruction format for a stack based virtual machine designed as a portable compilation target for programming languages enabling deployment for client and server applications on the web" },
    Doc { title: "The DOM boundary problem", url: "wasm://dom-boundary", body: "In a normal browser wasm code still must call through JavaScript bindings to touch the DOM every mutation crosses a costly JS to native boundary this per call marshaling overhead is the JS tax that no wasm framework running inside a browser can avoid" },
    Doc { title: "Host-mediated RPC design", url: "wasm://rpc", body: "A freestanding wasm guest has no sockets and no threads so the host owns the network the guest asks a service to run a method and the reply arrives later as an event correlated by request id this mirrors fetch exactly" },
    Doc { title: "Capability security instead of ambient authority", url: "wasm://capabilities", body: "A guest may call only the services its manifest declares an undeclared service name is rejected before any network io the wasm import list becomes the permission manifest extended to the network" },
    Doc { title: "Atoms: the string killer", url: "wasm://atoms", body: "Tag attribute and style names travel as u32 atoms well known atoms cover every renderable html tag and common style properties so common dom traffic carries zero string bytes across the abi boundary" },
    Doc { title: "Why in-process wasm is fast", url: "wasm://in-process", body: "When the wasm runtime lives inside the renderer process a guest write is a function call into the renderer address space instead of a message across a process boundary measured at nanoseconds per operation" },
    Doc { title: "Reactified C: components in a header", url: "wasm://reactified-c", body: "A single header turns the c preprocessor into a component model with hooks state effects and a keyed identity reconciler proving that even freestanding c can have a pleasant react shaped authoring layer" },
    Doc { title: "The polyglot wire claim", url: "wasm://polyglot", body: "Go rust and c bindings for the same todo application emit byte identical traffic on the wire proving that the abi is truly language neutral and any community can build its own idiom on top" },
    Doc { title: "Async fetch without JavaScript", url: "wasm://fetch", body: "A wasm guest calls fetch and receives a request id completion arrives later as a net result event with status and body the host does the http work on a background thread while the guest stays single threaded" },
    Doc { title: "Object capabilities and attenuation", url: "wasm://ocap", body: "Authority is an unforgeable attenuable token that references a specific object and a set of rights rather than a role a caller can narrow a capability and hand it to someone else without sharing a credential" },
    Doc { title: "Why browsers keep a semantic DOM", url: "wasm://semantic-dom", body: "Find in page password managers reader mode and screen readers all depend on a machine readable tree of real elements a fully custom painted surface forfeits every one of these for free platform features" },
    Doc { title: "wasmtime and the Engine lifecycle", url: "wasm://wasmtime", body: "A wasmtime engine is designed to be cheaply cloned and shared across many stores so a host running several guest tabs should create one engine process wide rather than one per guest load" },
    Doc { title: "Promise pipelining explained", url: "wasm://pipelining", body: "A pipelined rpc call invokes a method on the result of a call that has not returned yet collapsing a chain of dependent round trips into a single network round trip" },
    Doc { title: "Local-first sync", url: "wasm://local-first", body: "A guest can work against a local replica of the objects it holds capabilities to writes apply instantly and queue to a durable log while the network becomes a background sync detail rather than a blocking dependency" },
];

#[derive(Deserialize)]
struct SearchReq {
    q: String,
}

#[derive(Serialize)]
struct SearchHit {
    title: String,
    url: String,
    snippet: String,
    score: u32,
}

#[derive(Serialize)]
struct SearchResp {
    query: String,
    took_ms: u64,
    results: Vec<SearchHit>,
}

fn search(query: &str) -> SearchResp {
    let started = std::time::Instant::now();
    let terms: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for doc in CORPUS {
        let hay = format!("{} {}", doc.title.to_lowercase(), doc.body.to_lowercase());
        let score: u32 = terms.iter().map(|t| hay.matches(t.as_str()).count() as u32).sum();
        if score == 0 {
            continue;
        }
        let snippet = if doc.body.len() > 140 {
            // Truncate on a word boundary, not a raw byte offset — cutting
            // mid-word reads as broken text once the client wraps it.
            let cut = doc.body[..140].rfind(' ').unwrap_or(140);
            format!("{}...", &doc.body[..cut])
        } else {
            doc.body.to_string()
        };
        hits.push(SearchHit { title: doc.title.to_string(), url: doc.url.to_string(), snippet, score });
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    SearchResp {
        query: query.to_string(),
        took_ms: started.elapsed().as_micros() as u64 / 1000,
        results: hits,
    }
}

// ---------------------------------------------------------------- http

fn header_value<'a>(headers: &'a [Header], name: &str) -> &'a str {
    headers
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
        .unwrap_or("")
}

fn json_response(status: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_string(body).with_status_code(status).with_header(header)
}

fn main() {
    let require_channel = std::env::var("SEARCH_REQUIRE_CHANNEL").map(|v| v != "0").unwrap_or(true);
    let addr = "127.0.0.1:8788";
    let server = Server::http(addr).expect("bind 127.0.0.1:8788");
    println!("search RPC server on http://{addr} (channel-auth required: {require_channel})");

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let mut body = Vec::new();
        let _ = request.as_reader().read_to_end(&mut body);

        let headers = request.headers().to_vec();
        let req_id = header_value(&headers, "GWB-Req-Id").to_string();
        let app_key = header_value(&headers, "GWB-App-Key").to_string();
        let sig = header_value(&headers, "GWB-Sig").to_string();
        let ts = header_value(&headers, "GWB-Ts").to_string();

        let resp = handle(&method, &url, &req_id, &app_key, &sig, &ts, &body, require_channel);
        let _ = request.respond(resp);
    }
}

fn handle(
    method: &Method,
    url: &str,
    req_id: &str,
    app_key: &str,
    sig: &str,
    ts: &str,
    body: &[u8],
    require_channel: bool,
) -> Response<std::io::Cursor<Vec<u8>>> {
    if *method != Method::Post {
        return json_response(404, "{\"error\":\"not found\"}");
    }
    let parts: Vec<&str> = url.trim_start_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "rpc" {
        return json_response(404, "{\"error\":\"not found\"}");
    }
    let (iface, meth) = (parts[1], parts[2]);
    if iface != "search.query.v1" || meth != "search" {
        return json_response(404, "{\"error\":\"unknown method\"}");
    }

    if let Err(e) = verify_channel(require_channel, iface, meth, req_id, app_key, sig, ts, body) {
        return json_response(e.status, &format!("{{\"error\":\"{}\"}}", e.body));
    }

    let req: SearchReq = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => return json_response(400, "{\"error\":\"bad request body\"}"),
    };
    let resp = search(&req.q);
    json_response(200, &serde_json::to_string(&resp).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn gen_key() -> ed25519_dalek::SigningKey {
        let mut seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        ed25519_dalek::SigningKey::from_bytes(&seed)
    }

    fn sign(sk: &ed25519_dalek::SigningKey, iface: &str, method: &str, req_id: &str, ts: &str, body: &[u8]) -> String {
        use ed25519_dalek::Signer;
        let msg = canonical_bytes(iface, method, req_id, ts, body);
        base64::engine::general_purpose::STANDARD.encode(sk.sign(&msg).to_bytes())
    }

    #[test]
    fn signed_search_ok() {
        let sk = gen_key();
        let pk = base64::engine::general_purpose::STANDARD.encode(sk.verifying_key().to_bytes());
        let body = br#"{"q":"wasm"}"#;
        let ts = now_ms().to_string();
        let sig = sign(&sk, "search.query.v1", "search", "1", &ts, body);
        let resp = handle(&Method::Post, "/rpc/search.query.v1/search", "1", &pk, &sig, &ts, body, true);
        assert_eq!(resp.status_code().0, 200);
    }

    #[test]
    fn unsigned_rejected_when_required() {
        let body = br#"{"q":"wasm"}"#;
        let resp = handle(&Method::Post, "/rpc/search.query.v1/search", "1", "", "", "", body, true);
        assert_eq!(resp.status_code().0, 401);
    }

    #[test]
    fn tampered_body_rejected() {
        let sk = gen_key();
        let pk = base64::engine::general_purpose::STANDARD.encode(sk.verifying_key().to_bytes());
        let ts = now_ms().to_string();
        let sig = sign(&sk, "search.query.v1", "search", "1", &ts, br#"{"q":"other"}"#);
        let resp = handle(&Method::Post, "/rpc/search.query.v1/search", "1", &pk, &sig, &ts, br#"{"q":"wasm"}"#, true);
        assert_eq!(resp.status_code().0, 401);
    }

    #[test]
    fn unknown_method_404() {
        let resp = handle(&Method::Post, "/rpc/search.query.v1/nope", "1", "", "", "", b"{}", false);
        assert_eq!(resp.status_code().0, 404);
    }

    #[test]
    fn search_ranks_relevant_docs_first() {
        let resp = search("wasm dom boundary");
        assert!(!resp.results.is_empty());
        assert!(resp.results[0].score >= resp.results.last().unwrap().score);
    }
}
