# 04-WEB-RPC.md — the first-class RPC API model (wire, auth, manifest binding)

### Status: 🟡 FIRST DRAFT + local-dev binding being IMPLEMENTED (storefront demo).
### The design is in `00-WEBNEXT-OVERVIEW.md` §4. This doc is the normative,
### implementable spec: the guest ABI import, the wire, the auth model, the
### manifest capability binding, and the local-dev HTTP transport. Enforces
### Constitution rules 1 (verify), 3 (host-mediated), 4 (capabilities, no
### ambient authority), 6 (authn/authz).

## 0. The shape (why RPC, not fetch)

An app asks a **service** to execute a **method**: `{iface, method, id, payload}`.
No routes, no verbs, no controllers — the server's API *is* its method table.
The browser **host owns the transport**; the guest stays freestanding (no sockets
in the sandbox — same law as `fetch`). Two guarantees the transport enforces:

- **Authn is channel-level.** Every call carries cryptographic caller identity
  (the app-scoped key, §1). Layered on top, a user principal (login) rides the
  request for human-facing authz.
- **Authz is one guard at the top of the method:** `can(principal, method, args)`.

## 1. Guest ABI — the `rpc_call` import (mirrors `fetch`)

New import in module `"gwb"`, and one new event kind. This is the ONLY ABI
surface RPC adds; it follows the async `fetch`→`NET_RESULT` pattern exactly.

```
import  gwb.rpc_call(ptr: u32, len: u32) -> u32      // returns req_id (0 = enqueue failed)
event   RPC_RESULT = kind 41                          // completion, correlated by req_id
```

**Request buffer** (guest memory at ptr..ptr+len) — fixed header + three UTF-8
strings + payload bytes:

```
header (8 bytes): service_len u16 | iface_len u16 | method_len u16 | flags u16
then:  service utf8 | iface utf8 | method utf8 | payload bytes (rest of buffer)
```

- `service` — the **capability name** the app declared in its manifest
  (`"catalog"`, `"auth"`, `"orders"`). NOT a URL — the host resolves it to an
  endpoint + signing key via the granted service registry (§4). An undeclared
  service name is rejected before any network (capability security).
- `iface` — versioned interface id (`"shop.catalog.v1"`); the server checks it.
- `method` — the method name (`"list"`, `"get"`, `"login"`).
- `payload` — request body bytes. v1 demo uses **JSON**; the wire is
  content-type-tagged so CBOR is a drop-in (`flags` bit0 = payload is CBOR).
- `flags` bit1 = this call carries the current user session token (host attaches
  it from the app's session slot, set by a prior `auth.login` result).

**Result** — `RPC_RESULT` (kind 41) event record. Header `target`/`listener`
are unused (set to 0). Payload[16]:

```
status u16 @0 | ok u8 @2 | err_class u8 @3 | req_id u32 @4 | (reserved @8..16)
trailing string = response body (JSON/CBOR bytes as UTF-8/opaque)
```

- `ok` = transport+auth succeeded AND status < 400. `err_class` (when !ok):
  0 none · 1 transport (unreachable/timeout — the "backend is a zombie" signal,
  §3) · 2 authn (bad/expired identity) · 3 authz (`can` denied) · 4 not-found
  (iface/method) · 5 bad-request · 6 server-error. Distinct classes so the app
  reacts correctly (retry vs re-login vs show-forbidden), per §4/§3.

## 2. Wire — host ↔ service (local-dev HTTP binding)

The §4 native wire is length-prefixed CBOR over QUIC/Noise. The **local-dev
binding** (this demo) is the spec's declared **rpc-over-HTTP ramp** — same
logical envelope, HTTP as the pipe:

```
POST  {endpoint}/rpc/{iface}/{method}          endpoint from the manifest
Headers:
  Content-Type: application/json                 (or application/cbor)
  GWB-Req-Id:   {req_id}                         correlation
  GWB-App-Key:  {base64 ed25519 app pubkey}      channel identity (authn L1)
  GWB-Sig:      {base64 ed25519 sig}             sign(app_key, canonical bytes)
  GWB-Ts:       {unix_ms}                        replay window (±60s)
  GWB-Session:  {token}                          user principal (authn L2, optional)
Body: payload bytes

Response:
  HTTP status + JSON/CBOR body
  4xx/5xx map to err_class per §1
```

**Canonical bytes signed** (deterministic, no ambiguity):
`iface "\n" method "\n" req_id "\n" ts "\n" sha256(body)` — UTF-8, LF-joined.
The server verifies `GWB-Sig` over these before dispatch (authn L1), rejecting
stale `GWB-Ts` (replay) and unknown app keys per policy.

## 3. Auth model (the two layers, concretely)

**L1 — channel authn (always).** The host holds the app's app-scoped ed25519
keypair (derived per §1; for local dev, generated once and cached under the app
profile). Every request is signed. The server maps `GWB-App-Key` → an app
principal. This is the spec's "no session to steal": the *app's* identity is
cryptographic and stateless.

**L2 — user authn (when the app has users).** A storefront has humans. The app
calls `auth.login(email, password)` → server verifies credentials → returns a
**capability token**: `{sub, role, exp, iat}` signed by the server key (compact
JWT-shaped, but server-key-signed, not a shared secret). The host stores it in
the app's session slot; subsequent calls set `flags` bit1 and the host attaches
`GWB-Session`. Logout clears the slot.

**Authz — one guard per method.** Each method declares a required capability:

```
public         no session needed         catalog.list, catalog.get, auth.login
authenticated  valid session (any role)  cart.*, orders.place, orders.mine
admin          session.role == "admin"   admin.upsertProduct, admin.deleteProduct
```

Server dispatch is: verify L1 sig → resolve L2 session (if present) →
`can(principal, method)` → business logic → reply. A denied guard returns
`err_class 3` (authz), never runs the body.

## 4. Manifest — capability binding + `web://` navigation

The renderer today loads a hardcoded `.wasm`. The manifest replaces that: a
`web://` address resolves to a manifest that declares the bundle AND the RPC
services the app may call (capabilities). The app can call **only** declared
services (no ambient authority).

**AppManifest** (local-dev JSON; the normative CDDL is a P0 `SCHEMAS/` item):

```json
{
  "name": "shop.local",
  "title": "Aurelia — Clothing & Accessories",
  "publisher": "ed:<app pubkey>",
  "bundle": "shop.wasm",                  // b3:<hash> in production; local path in dev
  "services": {
    "catalog": { "iface": "shop.catalog.v1", "endpoint": "http://127.0.0.1:8787", "key": "ed:<server pubkey>" },
    "auth":    { "iface": "shop.auth.v1",    "endpoint": "http://127.0.0.1:8787", "key": "ed:<server pubkey>" },
    "cart":    { "iface": "shop.cart.v1",    "endpoint": "http://127.0.0.1:8787", "key": "ed:<server pubkey>" },
    "orders":  { "iface": "shop.orders.v1",  "endpoint": "http://127.0.0.1:8787", "key": "ed:<server pubkey>" },
    "admin":   { "iface": "shop.admin.v1",   "endpoint": "http://127.0.0.1:8787", "key": "ed:<server pubkey>" }
  }
}
```

**Local resolver (DevX).** `renderer web://<name>` →  reads
`./manifests/<name>.json` (dev root; overridable with `--manifest-root`). This
is the local stand-in for the §1/§2 name→bundle resolution; production resolves
`web://` via the naming substrate (`02-WEB-NAMING.md`) and verifies `bundle`'s
`b3:` hash before running (Constitution rule 1). Dev skips hash verification for
a filesystem bundle and logs that it did (no silent trust downgrade).

**Service registry.** On load, the host builds `service_name → {endpoint, iface,
server_key}` from the manifest. `rpc_call`'s `service` field indexes this map; an
unknown name is a capability violation (rejected, logged, `err_class` not even
reached — it never leaves the host).

## 5. gwbc high-level binding (`useRpc`)

The C SDK adds a retained-style helper so components don't touch raw records:

```c
// fire a call; cb runs when RPC_RESULT with this req_id arrives
gwbc_rpc(const char* service, const char* iface, const char* method,
         const char* json_payload, void (*cb)(int ok, int err_class,
         int status, const char* body, void* ud), void* ud);
```

gwbc's event loop routes kind 41 by `req_id` to the registered callback (a small
pending-call table), then frees the slot. `flags` bit1 (send session) is set
automatically when a session is active. This mirrors how gwbc already routes DOM
events to element handlers.

## 6. Interface catalog (the storefront, v1)

```
shop.auth.v1      login(email,password)->{token,role}   logout()->{}   me()->{sub,role}
shop.catalog.v1   list(filter)->[product]   get(id)->product   categories()->[cat]
shop.cart.v1      get()->cart   add(id,qty)->cart   setQty(id,qty)->cart   remove(id)->cart
shop.orders.v1    place(cart,ship)->order   mine()->[order]   get(id)->order
shop.admin.v1     upsertProduct(p)->product   deleteProduct(id)->{}   orders()->[order]
```

Authz per §3: catalog + auth.login public; cart/orders authenticated; admin admin-only.

## 7. Test contract (nothing is real until these pass)

```
- host: rpc_call with an UNDECLARED service is rejected pre-network (capability)
- host: RPC_RESULT correlates to the right req_id under concurrent calls
- server: a request with a bad/absent GWB-Sig is rejected err_class 2 (authn)
- server: catalog.list works with NO session; admin.* denied without admin (err_class 3)
- server: a stale GWB-Ts (>60s) is rejected (replay window)
- e2e (--script): browse -> add to cart -> login -> checkout -> admin add product
- wire: a golden request/response pair (bytes) per method, valid + INVALID
```

## 8. Forward-compat note (local ramp → native wire)

The local HTTP binding is deliberately isomorphic to §4's native wire:
`{iface, method, id, payload}` + a signed caller identity + a per-method authz
guard. Swapping HTTP for QUIC/Noise changes the *pipe*, not the envelope, the
auth model, or the manifest capability binding — so the storefront app and the
Go handlers are unchanged when the native transport lands (P3).
