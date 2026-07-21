# 05 — The Client↔Service Interconnect (living design doc)

**Status: DRAFT / living.** This is a working document to iron out over time, not a
spec. It captures where the client↔service relationship stands today, what's good
and bad about it, and a north-star design to argue with and chip away at.
`04-WEB-RPC.md` is the *current* spec; this doc is the *future* we're steering toward.

**Legend for every claim below:**
- ✅ **Built** — exists and is exercised today (renderer + `server/` + tests).
- 🧩 **Designed** — architecturally sound, not yet built.
- 🔬 **Research** — high ceiling, real unknowns, largest cost.

Last updated: 2026-07-21.

---

## 0. The thesis (the thing we're actually arguing about)

Today we have five separate concepts: `fetch`, RPC, subscribe, auth, and offline.
The bet of this doc is that a 10/10 interconnect has **one** primitive instead:

> **A capability to a live, replicated, typed object.**

Everything collapses into it:
- **fetch** = read a cap once
- **RPC** = invoke a method on a cap
- **subscribe** = hold a cap and watch it change
- **auth** = you hold the cap or you don't
- **offline** = the cap's object is locally replicated
- **delegation** = attenuate the cap and hand it off

Whether that thesis survives contact is the point of ironing this out. Everything
after §1 is either evidence for it or a step toward it.

---

## 1. What exists today (the baseline) ✅

- **Host-mediated transport.** `gwb.rpc_call(service, iface, method, payload) → req_id`;
  the reply lands later as an `RPC_RESULT` event (kind 41), correlated by `req_id`.
  Mirrors `fetch` exactly. The guest never touches a socket.
- **Capability-scoped by manifest.** `web://name` resolves to a bundle **plus a service
  registry**. `rpc_call`'s `service` arg indexes that registry; an undeclared name is
  rejected *before any network I/O*.
- **Two-layer auth** (`docs/04-WEB-RPC.md` §3):
  - **L1 channel authn** — app-scoped **ed25519** key signs every request over canonical
    bytes (`iface\nmethod\nreqID\nts\nsha256(body)`); server verifies, rejects stale ts
    (±60 s replay window).
  - **L2 user authn** — `auth.login` → server-signed session token, stashed via a
    `session_set` host import; per-method authz guard: `public | user | admin`.
- **Transport today:** JSON over HTTP/1.1 POST. Envelope: `{iface, method, id, payload}`
  + signature headers. Native QUIC/Noise is the declared future, unbuilt.
- **Proof:** `server/` (Go stdlib) with 9 auth/authz unit tests passing; a scripted e2e
  drives browse→login→add→checkout through the real UI.

---

## 2. Pros of the relationship as built ✅

- **Host-mediated ⇒ the guest sandbox stays intact.** No socket access; can't be tricked
  into an arbitrary endpoint (no SSRF); can only *name* declared services. Security-
  critical, and free from the architecture.
- **Capability-scoped by manifest.** The service list *is* the permission set; undeclared
  calls fail deterministically, pre-network. Not a runtime check you can forget.
- **Cryptographic caller identity is structural, not bolted on.** Every call ed25519-signed;
  the channel identity is not a stealable cookie.
- **Transport-agnostic envelope.** `{iface, method, id, payload, sig}` is identical whether
  the pipe is HTTP or QUIC — genuinely forward-compatible; handlers don't change when the
  transport swaps.
- **RPC-shaped, not URL-munging.** Typed method calls; one authz guard per method. Clean
  mental model.

---

## 3. Cons / weaknesses (technical, no sugar)

| # | Weakness | Why it bites | Tag |
|---|----------|--------------|-----|
| C1 | **Replay inside the window** | ed25519 signs but there's no nonce/req-id dedup → a captured request replays for ±60 s. Undercuts the "cryptographic identity" claim. | ✅ real gap |
| C2 | **Unary only — no server push/stream/subscribe** | Mirrors `fetch`, so strictly client-pull. Live order status, inventory, chat, presence, progress — none of it. **Biggest functional gap.** | ✅ real gap |
| C3 | **No pipelining** | N dependent calls = N round-trips. The DOM side batches; the RPC side doesn't. | ✅ real gap |
| C4 | **Authz is RBAC, not ocap** | The *channel* is capability-scoped but the *method* guard is public/user/admin roles. Coarse, and it contradicts the elegant half of our own model. | ✅ real gap |
| C5 | **Untyped JSON blobs** | Hand-parsed (`gwbjson.h`); no schema/codegen → client and server can silently drift. | ✅ real gap |
| C6 | **No offline / local-first** | Thin caller; network down = nothing works. No local write log, no sync. | ✅ real gap |
| C7 | **Session token unrevocable + coarse** | 24 h signed blob; no revocation, no refresh, no fine-grained scope. | ✅ real gap |
| C8 | **Server trust incomplete** | Manifest names `ed:dev-server` but there's no pinned-key handshake; bundles load unverified (no `b3:` check). | ✅ real gap |

---

## 4. The 10/10 — six pillars 🧩🔬

Each pillar names its lineage (ocap, Cap'n Proto, CRDT/local-first) **to show it isn't
hand-waving — not to conform to those designs.** We're free to diverge.

### Pillar 1 — Object-capabilities, not roles 🔬
Kill `public/user/admin`. Authority *is* an unforgeable, **attenuable** token referencing a
specific object + rights. The host is the capability broker (holds the app key, mints and
narrows caps). Least privilege becomes the default; delegation is free — hand someone a
read-only cap to *just this cart*, no shared credential, no role.
*Lineage: ocap / macaroons.* Fixes **C4**.

### Pillar 2 — Bidirectional, streaming, promise-pipelined channel 🧩
One multiplexed QUIC stream. Server can push; client can subscribe; calls return
**promises you can pipeline** — invoke a method on the *result* of a not-yet-returned call,
so a chain of N calls costs **one** round-trip. New event kind for unsolicited
server→guest pushes (`RPC_PUSH` / `SUBSCRIPTION_EVENT`).
*Lineage: Cap'n Proto.* Fixes **C2 + C3**.

### Pillar 3 — Atomize the wire, like we atomized the DOM 🧩
An IDL → codegen typed bindings for every language; field names as **u32 atoms**,
binary-packed, zero-string — the exact philosophy that makes DOM traffic cheap, applied to
RPC. Client/server can't drift; the wire is as lean as the op stream. This is the pillar
that makes the whole platform feel like *one* idea, not "fast DOM + ordinary RPC."
Fixes **C5**.

### Pillar 4 — Local-first by default (opt-in per object) 🔬
The guest works against a **local replica** of the objects it holds caps to. Writes apply
instantly (optimistic), queue to a durable local log, sync in the background; conflicts
resolve **CRDT-style _or_ server-authoritative rebase — chosen per object type** (financial
data → server-authority; a draft doc → CRDT). The network becomes a sync detail. The host
owns the replica store + sync engine; the guest stays a pure function of local state.
*Lineage: local-first (Linear/Figma), CRDTs.* Fixes **C6**.

### Pillar 5 — Host as the single policy enforcement point 🧩
Lean into what's unique: the host already mediates. The guest expresses *intent* (invoke
cap X with payload); the host does identity, nonce/freshness, ordering, dedup, retry,
offline queueing, streaming, and attenuation. Every security property is enforced and
auditable in **one** place; the guest stays tiny and unprivileged.

### Pillar 6 — Close the trust holes as first-class, not patches 🧩
- **Nonce cache** within the freshness window → replay dies (**C1**).
- **Idempotency:** `req_id` becomes a real exactly-once token.
- **Mutual auth:** server key pinned in the manifest and verified in the handshake (**C8**).
- **Optional end-to-end envelope encryption** so the host-broker can be zero-knowledge for
  sensitive payloads.
- **Revocation + expiry** on caps/sessions (**C7**).

---

## 5. Honest tensions in this design (argue with these)

- **ocap revocation is hard.** Attenuation is easy; *revoking* a granted cap needs
  expiry/caveats or a revocation service. Don't pretend it's free.
- **Promise pipelining complicates errors.** A poisoned intermediate kills the whole chain;
  need a clean failure-propagation story.
- **Local-first is a large consistency bill.** This is *why* Pillar 4 is opt-in per object,
  not mandatory. Not every app wants CRDT semantics.
- **Host-as-broker concentrates trust in the host.** Acceptable only because the host is
  already the TCB in this architecture — but worth stating out loud.
- **The thesis (§0) may over-unify.** Collapsing five concepts into one primitive is elegant
  until a real use case wants them separate. Watch for where the abstraction leaks.

---

## 6. Build order (cheap → research)

The 10/10 is a path, not a dream. First two rows get ~80% of the felt improvement for a
fraction of the cost. **Start there, not with the CRDT research.**

| Effort | Work | Buys | Tag |
|--------|------|------|-----|
| **Days** | Nonce cache, idempotency keys, server-key pinning | Closes C1, C8, half of C7 | 🧩 |
| **Weeks** | Bidirectional streaming + subscriptions (one new event kind; QUIC or WS) | Server push — the biggest functional gap (C2), start on C3 | 🧩 |
| **Weeks** | IDL + atomized binary wire + codegen | Typed, drift-proof, cheap (C5) | 🧩 |
| **Research** | ocap + attenuation (C4); local-first replica + CRDT sync (C6) | Highest ceiling, most cost | 🔬 |

---

## 7. Open questions (to resolve over time)

- [ ] Is §0's one-primitive thesis right, or do we keep fetch/RPC/subscribe distinct?
- [ ] QUIC/Noise vs WebSocket-ish vs raw-TCP-with-framing for the bidirectional channel?
- [ ] Cap format: macaroon-style (caveats) vs SPKI-style vs custom signed struct?
- [ ] Where does the local replica live — host process memory, or a durable host-owned store?
- [ ] CRDT library/algorithm, or hand-rolled per-type merge? (Cost vs control.)
- [ ] Revocation mechanism: short expiry + refresh, or an online revocation check?
- [ ] Does the IDL share the atom table with the DOM ABI, or keep separate atom spaces?
- [ ] Zero-knowledge envelope: which payloads warrant it, and what's the key model?

---

## 8. Changelog

- **2026-07-21** — Initial draft. Captured current baseline (§1), pros (§2), cons (§3),
  the six-pillar 10/10 (§4), self-critique (§5), build order (§6), open questions (§7).
  Authored from a working session reviewing the storefront RPC interconnect.
