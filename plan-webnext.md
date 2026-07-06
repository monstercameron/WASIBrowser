# Web next.0 — a transport, naming, and RPC layer for WASIBrowser
### Plan draft 1 — for critique, not yet pinned

The app layer is done differently already (wasm-first, no JS, binary DOM ABI).
This plan does the same to everything *below* the app: how apps are named,
found, moved, and how they talk to services. "web3" burned the branding on
tokens; none of this uses a coin, a chain, or consensus. Everything here is
hashes, signatures, and caches — boring cryptography, no speculation.

Design goals, priority-ordered (from Cam's brief):
1. **High reliability / bus-proofing** — no single operator, resolver, or
   host whose death breaks an app. Anyone can mirror anything verifiable.
2. **Resilience on bad links** — fountain coding (RaptorQ-class), loss
   tolerance, resumability, offline-first.
3. **Privacy / anonymity** — no global identity, no cookies concept at all,
   who-fetched-what separated from what-was-fetched.
4. **Lower data costs** — content addressing + dedup + delta updates; fetch
   from the nearest cache, not the origin, without trusting the cache.
5. **Lower latency** — 0-RTT transports, no per-piece round trips, local
   peers first, precompiled wasm relaunch.
6. **Legacy HTTP as a ramp, not a foundation** — day one everything works
   over plain HTTPS gateways; the gateway is just an untrusted cache.

---

## 1. Naming: kill the location, keep the name

A URL today conflates four things: *identity* (which app), *version* (which
bytes), *location* (which server), and *route* (which screen). next.0 splits
them:

```
wnx://<authority>/<path>[?query]

authority :=  b3:<blake3-hash>        immutable bundle (identity == bytes)
           |  ed:<ed25519-pubkey>     mutable identity (signed pointer to a bundle)
           |  name.tld                transitional petname, resolved via legacy rails
path      :=  route WITHIN the app (the app owns it; never a server path)
```

- **`b3:` — content identity.** The authority *is* the BLAKE3 root hash of
  the bundle. Anyone — peer, CDN, USB stick — can serve it; the client
  verifies. Immutable, cache-forever, offline-forever. This is the atom
  everything else reduces to.
- **`ed:` — publisher identity.** An Ed25519 public key. It resolves to a
  small **signed manifest**: `{seq, bundle: b3:..., prev, sig}`. Updating an
  app = publishing a new signed manifest. Trust is in the key, not in any
  server. Key rotation via a signed successor record; loss of key = loss of
  name (mitigations in §7).
- **`name.tld` — the ramp.** Resolved via DNS TXT / `.well-known` to an
  `ed:` or `b3:` authority, so existing names onboard without asking anyone.
  The human-name layer is deliberately pluggable (DNS today; petname sets /
  community registries later) because *that* is the one problem hashes don't
  solve, and pretending otherwise is how you end up re-inventing ICANN.

**Identity hashing for users** (the other half of "identity"): there is no
global user id. The browser holds a master keypair per profile; for each app
authority it derives an app-scoped keypair (`HKDF(master, authority)`).
Apps see a stable pseudonym for *their* app and nothing linkable across apps
— unlinkability by construction, not by policy. "Log in" = prove control of
the derived key (one signature); no passwords, no third-party identity
provider, nothing to breach.

## 2. Bundles: the unit of distribution

A bundle is an immutable, content-addressed archive:

```
bundle := manifest chunk + content chunks
manifest: { app wasm hash, asset map (path -> chunk list), entry point,
            min ABI version, total size, chunking params }
chunks:   content-defined chunking (FastCDC, ~64KiB target), each chunk
          addressed by its BLAKE3 hash; bundle root = merkle root
```

Why chunks and not a tarball: **delta updates for free**. Version N+1 shares
most chunks with N; a client that has N fetches only new chunks (lower data
cost #4). Chunk-level addressing also lets *any* holder of *any* version
serve the overlap — the swarm's cache is shared across versions.

The renderer keeps a local **chunk store** (single flat content-addressed
db). Everything ever fetched is a seedable cache entry. Apps relaunch from
the store with **zero network** and, with the wasmtime compile cache keyed
by the same hash, near-zero start latency (goal #5: relaunch = mmap).

## 3. Distribution: swarm + fountain, gateway as training wheels

Three ways to get chunks, tried concurrently, all verifiable, all filling
the same store:

1. **LAN peers** (mDNS discovery) — same-office/same-house fetches never
   touch the internet. Classrooms, LAN parties, disaster comms.
2. **Swarm** — DHT (Kademlia; BEP44-style mutable records hold `ed:`
   manifests, immutable records hold nothing — `b3:` needs no lookup, only
   peer discovery per root hash). Peer transport: QUIC.
3. **HTTP(S) gateways** — `GET https://gw.example/b3/<hash>` returns chunks.
   Any dumb CDN can be a gateway *because the client verifies*; the gateway
   is untrusted infrastructure. This is the legacy ramp: day one, an app
   published to one $5 static host is fully functional; the swarm is an
   upgrade, not a requirement.

**Fountain coding (the "raptor" in the brief).** Within a fetch, chunks are
transmitted as RaptorQ (RFC 6330) symbols rather than requested piece by
piece:

- Receiver needs *any* K(+ε) symbols, from *any mix of senders*, in *any
  order* — no per-piece request/response RTTs (latency #5), no duplicate
  suppression protocol between seeders (three seeders can blast symbols
  uncoordinated and nothing is wasted), loss on bad links costs ε extra
  symbols instead of a retransmit round trip (resilience #2).
- Symbols are self-identifying `(root hash, block no, symbol id)`; a merkle
  proof accompanies each block so verification stays incremental.
- IPR note: RaptorQ has Qualcomm patent statements with RAND terms; the
  open `Wirehair`/LT-code family is the fallback if that's a problem.
  Decision needed (flagging for critique).

**Bus-proofing** (the other reading of the brief): every layer above must
survive any single party vanishing — publisher (mirrors keep seeding;
manifests are signed, so mirrors can't tamper), gateway (any other gateway
or peer serves the same hashes), DHT bootstrap (multiple hardcoded +
user-suppliable bootstrap sets; LAN discovery works with zero bootstrap),
even *this project* (the formats are specs first, implementation second).

## 4. RPC: how apps talk to the world

Pages fetching URLs is the wrong shape for apps. next.0 apps speak RPC to
**services**, and the browser host owns the transport (the guest stays
freestanding, sockets stay out of the sandbox — same law as `gwb.fetch`).

**The spec** (a peer of ABI.md, language-neutral):

```
service identity  := ed25519 pubkey (same authority type as apps)
interface         := versioned schema; interface id = b3 hash of its
                     canonical schema text (no registry, no collisions)
wire              := length-prefixed CBOR: {iface, method, id, payload}
                     over one mux'd encrypted stream (QUIC / Noise XX)
guest ABI         := gwb.rpc_call(service, iface, method, payload) -> req id
                     completions arrive as RPC_RESULT events (kind 41),
                     mirroring fetch/NET_RESULT; streaming replies arrive
                     as RPC_STREAM events carrying ordered frames
host              := wasmtime import, standard across all SDKs; the SPEC
                     defines it so any wasm host (not just ours) can comply
```

- **Capabilities, not ambient authority**: an app can only call services
  its manifest declares (user-visible, grantable/revocable per app — the
  CRUD-on-inputs discipline applied to the network).
- **Location-free**: the client resolves `ed:` → current endpoints (DHT
  record or DNS ramp), connects, and *verifies the service key in the
  handshake* — a service can move hosts, be multi-homed, or sit behind a
  relay without apps noticing. TLS's CA tree is not involved.
- **Legacy ramp**: `rpc-over-HTTPS` POST binding to a gateway, so a plain
  web server can host a service on day one.

## 5. Privacy: separate the who from the what

Threats, in order of realism: (a) services correlating users, (b) network
observers seeing what you fetch, (c) swarm peers logging who wants what.

- (a) is structurally handled by app-scoped derived identities (§1) and by
  having no cookies/storage a service can read back across apps.
- (b): everything is encrypted on the wire (QUIC/Noise); content addresses
  are visible only to the parties you fetch *from*, which leads to
- (c) **oblivious fetch mode**: a 2-hop relay scheme (oHTTP-style — relay
  knows who you are but not what you want; gateway/peer knows the what but
  not the who). Opt-in per profile ("private mode" that actually means
  something), default-on for `ed:` manifest lookups since those leak intent
  the most. Full Tor-grade anonymity is explicitly out of scope v1 — we
  state the threat model honestly instead of promising magic.
- Herd effect: content addressing means you fetch the *same bytes* as
  everyone else; there's no per-user URL to fingerprint you with.

## 6. What the renderer grows (implementation shape)

The existing seam holds: all of this lives HOST-side; guests see only new
imports + event kinds. No change to the DOM ABI.

```
renderer/
  src/store.rs      chunk store (BLAKE3-keyed, mmap'd; also wasmtime
                    compile cache keyed by module hash)
  src/wnx/          resolver (b3/ed/dns ramp), manifest verify
  src/wnx/gateway.rs  HTTP(S) chunk fetch (ureq today, QUIC later)
  src/wnx/swarm.rs    DHT + peer protocol + fountain rx/tx (phase 3)
  src/rpc.rs        rpc host import + event delivery (mirrors fetch)
docs/
  WNX-NAMING.md  WNX-BUNDLE.md  WNX-SWARM.md  WNX-RPC.md   (specs first)
```

## 7. Honest risks / open questions (critique targets)

1. **Human names.** DNS-as-ramp is fine; the endgame (petnames? registries?
   first-come squat-fest?) is unsolved everywhere. I propose shipping ramp
   + raw hashes v1 and *not* inventing a naming coin. Push back if you want
   a stronger stance.
2. **Key loss = name loss** for `ed:`. Mitigations: social-recovery
   successor records (signed by M-of-N recovery keys named at creation);
   still weaker than "call GoDaddy". Acceptable?
3. **DHT realities**: sybil attacks, NAT traversal, mobile churn. The
   gateway ramp means the DHT can be *best-effort* for years without hurting
   reliability. Is that honest enough or does it quietly re-centralize us
   onto gateways (my main worry)?
4. **RaptorQ IPR** (§3). Decide: RaptorQ (best-in-class, RAND patents) vs
   Wirehair (open, slightly worse overhead). Cheap to abstract behind a
   symbol-codec trait; expensive to change wire formats later.
5. **Seeding incentives without tokens**: publishers + gateways + altruism +
   default seed-what-you-run (capped). BitTorrent proves this works for
   things people care about; it does rot for the long tail. The long-tail
   answer is probably "archives" (institutions pinning stuff), not markets.
6. **Dynamic content**: content addressing loves static bundles; live data
   goes over RPC. Is the split clean enough? (Feeds = RPC + signed inline
   objects that are themselves `b3:` addressable, so even dynamic content
   degrades into cacheable atoms.)
7. **Spec-first discipline**: 4 spec docs before serious code. Agreed?

## 8. Phases (each lands a demo in this repo)

- **P0 — specs.** WNX-NAMING / WNX-BUNDLE / WNX-RPC drafts (WNX-SWARM
  sketched). Exit: you've critiqued them.
- **P1 — hashes over HTTP.** Chunk store + bundler CLI (`wnx pack`) +
  `wnx://b3:...` loading via a gateway (a static file server) with full
  verification + offline relaunch from the store + compile cache.
  *Demo: task-dashboard-c loads from `wnx://b3:...`, then loads again with
  the network cable pulled.*
- **P2 — publisher identity.** `ed:` manifests, `wnx publish`, update flow
  with delta fetch (only changed chunks move). *Demo: v2 of the dashboard
  ships by moving <10% of its bytes.*
- **P3 — RPC v1.** Host import + RPC_RESULT events + gwbc `useRpc` hook;
  rpc-over-HTTPS binding; a demo service (the todos backend) with a signed
  key, reached by pubkey. *Demo: RemoteTodos card, but the endpoint moves
  hosts mid-session and nothing breaks.*
- **P4 — LAN + swarm.** mDNS peer fetch, then DHT + QUIC + fountain codec.
  *Demo: two machines, cable between them, no internet: app transfers and
  runs.*
- **P5 — oblivious mode.** Relay protocol + private fetch toggle in the
  toolbar. *Demo: gateway logs show relay ip, relay logs show no hashes.*

Sequencing rationale: P1/P2 deliver bus-proofing + data-cost wins with ZERO
novel networking (it's files + hashes + any web server), which means the
risky parts (DHT, fountain, relays) are additive upgrades to an
already-useful system — the same commodity-behind-a-seam bet that worked
for the render engine.
