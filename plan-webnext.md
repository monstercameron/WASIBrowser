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
web://<authority>/<path>[?query]

authority :=  b3:<blake3-hash>            immutable bundle (identity == bytes)
           |  ed:<ed25519-pubkey>         mutable identity (signed pointer)
           |  <label>[.<label>...]        native name: a delegation path,
                                          root-left — top label §1b, chain §1d
           |  <label>[...]~<keytag>       any label pinned to its key (§1c)
           |  dns:<name.tld>              explicit legacy DNS ramp
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
- **`dns:name.tld` — the ramp.** An explicitly-marked legacy name, resolved
  via DNS TXT / `.well-known` to an `ed:` or `b3:` authority, so existing
  names onboard without asking anyone. The `dns:` marker is required because
  dots now mean *native delegation* (§1d), not DNS — legacy is the tagged
  exception, native is the default. The endgame human-name layer is §1b–§1d.

## 1b. Human names without registrars

Key scarcity is solved (2^252 Ed25519 keys — "domain capacity" is unlimited
at the identity layer). The remaining scarce thing is *short memorable
strings*, and every prior system monetized that scarcity (registrars) or
tokenized it (Handshake/ENS — no). The next.0 answer: **free,
first-come-first-served claims on federated transparency logs**, with the
squat economy killed structurally instead of priced out:

```
claim   := { name, owner: ed25519 pubkey, expiry, sig }   -- FREE, NON-EXCLUSIVE
log     := append-only, merkle-audited (CT-style); N independent operators
           cross-sign each other's heads; anyone can mirror/audit a log;
           a claim counts only when present in a QUORUM of logs
resolve := a name is NOT a monopoly. Many keys may claim "cam"; the durable,
           globally-unique address is name~keytag (§1c) — infinite and
           unstealable. Bare "cam" is a SOFT lookup: petname/history first,
           then namespace policy, then trust ranking, then an explicit
           chooser — never a silent winner. Logs order claims (earliest
           first) for tie-break DISPLAY, not for exclusion; a back-dating or
           censoring log is provably cheating and drops out of client quorum
           sets (operators are replaceable, not trusted)
```

Anti-squat without money — four structural deterrents:

1. **Non-transferable.** A name is bound to its key forever; there is no
   transfer record type. You cannot sell what cannot move — squatting has
   no exit value. (Succession = the §7 M-of-N recovery path, which names
   humans, not buyers.)
2. **Liveness renewal.** Claims expire (say, yearly) and renew with one
   free signature. Abandoned names recycle naturally; mass-squatting means
   mass key custody + mass renewal forever, for inventory you can't sell.
3. **Rate limits + proof-of-work per claim.** Filing costs nothing in money
   and something in compute/patience; hoarding 10⁶ names is expensive in
   the only currencies left.
4. **No pressure on any single string**: apps are equally reachable by
   `ed:` — a name is a convenience label, not existence. Losing the string
   war doesn't unpublish anyone.

**What a free name looks like.** A *top* label is one flat, dot-free string
claimed from the global pool; dotting it delegates downward (§1d):

```
name    := [a-z0-9] [a-z0-9-]{1,61} [a-z0-9]     (2-63 chars, ascii v1)

web://cam                      a person
web://tasks                    an app
web://cam/settings             route inside cam's app (path = app's, always)
web://cam/todos/today          if cam's manifest is a DIRECTORY (below),
                               "todos" selects the app, rest is its route
```

- **No TLDs.** TLDs exist to shard registry databases; delegation (§1d)
  shards by key instead, so `.com` dies with the registrar. A bare top
  label (`google`) is a §1b claim; a dotted path (`google.deepmind`) is a
  delegation chain (§1d); a `dns:`-tagged name is the legacy ramp. Native
  and legacy cannot collide because legacy carries the explicit marker.
- **One claim, many apps.** A name binds to one key; that key's manifest
  may be a **directory** — a signed map of sub-apps (`todos -> b3:...,
  blog -> b3:...`), selected by the first path segment. Namespacing comes
  from the publisher, not from the registry — no sub-name claims, no
  sub-registrars, nothing new to squat.
- **Confusables are folded before claiming.** Names are stored and matched
  by their skeleton (Unicode confusable-folding; ascii-only in v1 makes
  this trivial): `cam`, `càm`, and cyrillic `сam` are the *same claim* —
  the phishing classic dies at the registry instead of in the URL bar.
- **The browser shows the keytag.** Chrome (not the name itself) displays
  `cam · a7f2` — a short fingerprint of the bound key. Names are for
  humans; the keytag is the tell when something claims to be `cam` but
  isn't bound to the key your petname book / history knows.

Zooko's triangle honesty: this is *trust-minimized*, not trustless — global
human names need SOME shared ordering, and a quorum of mutually-auditing,
individually-disposable logs is the cheapest one that doesn't mint a token
or a landlord. Below it, **petnames** remain first-class: the browser lets
users/communities bind their own labels to `ed:` keys (imported sets, like
a contacts book), which covers the "names I actually type" case with zero
global coordination at all.

## 1c. Expansive & equitable: why no one parks

An exclusive flat namespace ("the one global `cam`, earliest wins") is the
*least* equitable design — it manufactures scarcity and rewards the fastest
land-grabber. Draft-1's FCFS-exclusive framing was wrong on this axis;
§1b above is now non-exclusive, and this section is why that makes parking
economically dead rather than merely discouraged.

**The parking economy needs three legs. next.0 removes all three:**

1. **Exclusivity — gone.** You cannot corner a name others may also claim.
   The durable, globally-unique address is `name~keytag` (a short
   fingerprint of the bound key): `web://cam~a7f2`. That gives ~2^k anchors
   per string, so *the namespace is effectively infinite* — every person
   named cam gets a real, permanent `cam~<theirs>`. Bare `cam` is only a
   local convenience, resolved per-user (§1b), never a deed.
2. **Transferability — gone** (§1b: names are key-bound, no transfer record
   type). You cannot sell what cannot move.
3. **Passive yield — gone.** A claimed-but-unpublished name resolves to
   *nothing*: no origin server, no default landing page, no ad slot to
   inject — the entire parking-page business model requires a server that
   auto-serves ads on an idle domain, and there isn't one. A dead name is a
   dead resolve. Holding names yields exactly zero while costing renewal
   (§1b liveness) forever, over inventory that isn't even scarce.

**Namespaces are commons, not TLDs.** The expansive layer replaces
ICANN-auctioned TLDs with permissionless, non-property namespaces:

```
web://nyc/hall            "nyc" is a namespace: its manifest is not a
web://md/dr-chen          directory of apps but a POLICY + governance keys
web://os/tetris           that governs how names WITHIN it are allocated
```

- A namespace is just a name (§1b) that opts into governing a sub-space by
  publishing an allocation policy. It is **owned by no one**:
  non-transferable like every name, and it **cannot extract rent** — a
  namespace that charges fees is visible to clients and competes against
  free ones; the default commons set shipped in the browser is free.
- **Anyone can create a competing namespace**, permissionlessly. There is
  no fixed TLD list to auction and no scarcity to corner — if a namespace
  governs badly, communities fork it, exactly like the log operators. No
  namespace is a monopoly.
- **Policies vary by community, which is where equity lives**: geographic
  (`nyc`, allocation gated on location attestation), professional (`md`,
  gated on a credential attestation), open (`os`, pure FCFS-within, still
  non-exclusive + keytag), curated (editorial). Underrepresented groups
  self-govern their own space by their own fairness rules instead of
  bidding against squatters in one global pool.
- **Meta-scarcity** ("but who gets bare `nyc`?") dissolves the same way:
  namespace names are themselves non-exclusive + keytag-anchored +
  trust-resolved. The actual city wins bare `nyc` by attestation and
  petname adoption, not by an exclusive grant — and if it doesn't, `nyc~<city
  key>` still works for everyone who trusts that key.

**Equity is not automatic — the honest open issues:**

- *Script equity.* ASCII-only v1 privileges Latin scripts. Full IDN with
  confusable-folding is a v-next *requirement*, not a nicety, or the
  "equitable for all participants" claim is hollow for most of the planet.
- *Compute equity.* Anti-sybil PoW (§1b) taxes the resource-poor if it's
  heavy. It must stay cheap + one-time, or move to rate-limits / verifiable
  delay functions (wall-clock, not hashpower) so a phone can claim a name
  as easily as a datacenter.
- *Discovery equity.* Soft resolution must not collapse to "rank by
  popularity" — that's rich-get-richer with extra steps. Petname/history
  first + an explicit chooser keeps the long tail reachable; the default
  trust-ranking is itself a kingmaker surface and must be pluggable,
  auditable, and never a single vendor's list.

## 1d. Hierarchy by cryptographic delegation (not by decree)

`web://us.google.deepmind.chat`, reading jurisdiction ▸ org ▸ team ▸ app, is
the right *allocation* model for one structural reason: a delegation tree
makes almost every name uncontended. Only *top* labels compete in the global
pool (§1b); everything below is minted by its parent's signature. There is
exactly one `google.deepmind` because only google's key can sign it — no
global race, ever. Infinite `chat`s (one per org), infinite `deepmind`s (one
per parent). Hierarchy is the most expansive namespace there is, and the
most human-readable — a strict improvement over §1c's flat pool for anyone
who wants a structured name.

**Dots delegate; each link is signed:**

```
us . google . deepmind . chat        big-endian: root (us) on the left
 │      │         │        └ app,  delegated by deepmind's key
 │      │         └ team,  delegated by google's key
 │      └ org,   VOUCHED by the us namespace (not owned — see below)
 └ top label (§1b): non-exclusive, trust-resolved, ~keytag-anchored

delegation := parent_key signs { label, child_key, revocable?, expiry }
resolve    := walk the chain from a trust root you accept; verify every
              signature; a broken or absent link => the name does not resolve
```

Dots are *identity* delegation (each segment is a distinct key); slashes stay
*routes* within one identity (`google.deepmind.chat/room/42`). A leaf may
bottom out in `~keytag` or `b3:` for a globally-unique anchor under a pretty
path (Cam's "unique ids" level).

**The country problem — the one place I push back hard.** A hierarchy
*rooted on countries with authority to seize* is a censorship machine bolted
to the root of every name: it hands governments an un-person button and
discards bus-proofing (#1), privacy (#3), and the whole anti-registrar
thesis in one move. Countries are the greediest registrars — they have
armies. Three rules keep the readable hierarchy from becoming DNS-with-a-flag:

1. **Every node has its own key and is addressable without its ancestors.**
   `google~keytag` resolves even if `us` revokes it or `us` vanishes.
   Ancestors *vouch*; they do not *own*. The worst an ancestor can do is stop
   vouching — it can never orphan or seize a descendant that holds its key.
2. **Jurisdiction labels are OPTIONAL provenance, never the trust-critical
   root.** `us.google…` means "the `us` namespace vouches this is
   US-jurisdiction Google" — good for legal recourse, poison for
   censorship-resistance — so it is one *view*. The self-sovereign
   `google.deepmind.chat` (org as its own top label) is the default; several
   jurisdiction roots may coexist and you choose which, if any, you trust.
3. **Revocability is per-link and declared.** Org ▸ sub-org is normally
   revocable (Google may retire DeepMind's subname); country ▸ org is
   *non-authoritative* by construction (un-vouch, not un-person). No link is
   silently authoritative.

**Privacy stays intact.** This tree is for *published* identity — apps,
services, public orgs. The per-app user pseudonyms below (`HKDF`-derived,
unlinkable) are NOT in it and never carry a jurisdiction prefix. A human who
appears in the tree (`google.alice`) is using an opt-in *public* handle,
distinct from the private per-app key that logs them in.

Net: keep the readable `country.org.team.app` shape, drive it with signed key
delegation instead of registrar decree, key-anchor every level so no ancestor
is a chokepoint, and demote the country level from root-of-authority to
optional provenance.

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

## 2b. Versioning, freshness, and natural decay

Content addressing makes bytes immortal; that is a feature for survival and
a bug for hygiene. The swarm must not become a hoard of every version ever
published, and a *running* app must be able to learn it is stale. Both are
solved by making lifecycle a **first-class, signed part of the manifest**,
and by treating deletion as a *hint the network naturally respects*, never a
capability (you cannot force strangers to delete; you can make keeping the
old thing pointless).

**The manifest carries lifecycle, not just a pointer:**

```
manifest: { seq, bundle: b3:...,  prev: b3:...,
            retain: N,            keep this + N-1 predecessors seedable
            stale_after: dur,     clients past this SHOULD recheck before run
            sunset: bool,         end-of-life: archive, stop auto-seeding
            data_ttl: dur,        default freshness for the app's cached data
            sig }
```

**Outdated signalling (app side).** Every bundle launched via `ed:` (and
`b3:` bundles that embed their publisher key) gets a standard host check:
on launch (and periodically, rate-limited) the host re-resolves the
manifest; if `seq` moved, the guest receives a **`BUNDLE_OUTDATED` event
(kind 42, payload = new seq + severity)** — surfaced in gwbc as
`onUpdateAvailable(h)`. Severity comes from the publisher: *newer-exists*
(FYI), *stale* (past `stale_after` — nag), *sunset* (this app line is dead —
the chrome shows an archived banner and the store stops seeding it by
default). The app decides what to do; the platform only guarantees the
signal exists, is authentic (signed), and is uniform across every app —
that uniformity is what lets the ecosystem purge.

**Natural purge (network side).** Store/seeder GC policy reads the same
records: versions inside the `retain` window are seedable; superseded
versions outside it decay to LRU cache (kept while cheap, evicted first,
never re-announced to the DHT); `sunset` lines stop being announced at all.
Because chunks are shared across versions, "purging v1" physically means
dropping only the chunks v-current doesn't reference — decay is incremental
and mostly free. Archives (institutions that deliberately pin history) are
the *opt-in exception*, not the default behavior — history survives because
someone chose to keep it, not because nobody could delete it.

**The zombie-data problem (named honestly).** A bundle that outlives its
backend is a zombie: immortal UI, dead RPC. This is only partially
solvable, and the plan says so. What the platform does:

1. **Design pressure toward data-as-atoms**: anything a service returns
   that is not per-user can be a signed `b3:` object (with `data_ttl`) —
   such data outlives the service in caches/archives exactly like code, and
   the app degrades to read-only instead of dead.
2. **Service succession**: service manifests (same `ed:` machinery) can
   name a successor key/endpoint — a backend can hand over without the app
   updating. A missing manifest past its TTL = the standard "service is
   gone" signal, delivered to the app as an RPC error class, not a timeout
   guess.
3. **Final snapshots**: a publisher sunsetting a service can publish a
   last read-only data bundle referenced from the sunset manifest — the
   "everlasting wasm + everlasting final dataset" ending, instead of a
   zombie.
Per-user live data whose operator vanished is declared out of scope: no
protocol resurrects a dead database, and pretending otherwise is web3-brain.

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

**IPv6-first (the substrate).** next.0 assumes IPv6 and treats IPv4 as the
compatibility case, because three of our goals fall out of v6 for free:

- **Every node is addressable.** No NAT means no hole-punching rituals, no
  relay dependence for the common case: any browser can seed, any laptop
  can host a service — which is what bus-proofing actually requires.
  (IPv4/CGNAT peers still work — via gateways and v6-capable relay peers —
  they're just second-class seeders.)
- **Identity-derived addresses.** Each `ed:` identity deterministically
  derives an ORCHID-style IPv6 (RFC 7343: hash of the pubkey in
  `2001:20::/28`) used as its stable *overlay* handle — uniform in logs,
  ACLs, and socket-level APIs. Stated honestly: ORCHIDs are not globally
  routable; real connectivity comes from signed endpoint records listing
  concrete v6 addresses. The derived address is the wormhole-proof name of
  the endpoint, not a routing miracle.
- **Address abundance as an operational tool.** A service operator's /64
  yields per-app, per-tenant, even per-session addresses — migration,
  blue/green, and abuse isolation become address games instead of
  load-balancer configs. Combined with key-verified handshakes (§4), an
  endpoint can renumber freely: apps never see addresses, only keys.
- **Multicast symbols.** IPv6 multicast + fountain codes are made for each
  other: one sender, N receivers, zero per-receiver cost, and RaptorQ makes
  loss per-receiver harmless (each just collects ε more symbols). LAN scope
  first (classroom/office: one machine multicasts an app to thirty), wider
  scopes where networks allow.

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

## 4. RPC: the first-class API model (not a transport detail)

Pages fetching URLs is the wrong shape for apps. next.0 apps speak RPC to
**services**, and the browser host owns the transport (the guest stays
freestanding, sockets stay out of the sandbox — same law as `gwb.fetch`).

**Method-call beats resource-document.** The REST/MVC web is an impedance
stack: the client wants `addTask(title, priority)`, but must encode intent
into verb+URL+headers+body, the server must decode it through routing +
controller + serializer layers, and both sides maintain session state so
the server knows who's asking. next.0 deletes the stack:

- The client **asks the server to execute a method**: `{iface, method,
  payload}`. No routes, no verb semantics debates, no controllers — the
  server's API *is* its method table, and codegen from the interface schema
  gives every language typed stubs (client) and dispatch (server).
- **Authn is protocol-level, not app-level.** Every connection is bound to
  the caller's app-scoped key (§1) in the Noise handshake; every call
  arrives with cryptographic caller identity attached. There is no session
  to establish, steal, or expire — which deletes cookies, bearer tokens,
  token refresh, CSRF, and the login-state half of MVC in one move.
- **Authz is one guard at the top of the method**, against a verified key:
  `can(caller, method, args)`. The server-side pattern collapses to
  `authz check -> business logic -> reply` — the thing backend frameworks
  approximate with middleware pyramids.
- **Call semantics live in the schema**, per method: idempotent (safe to
  retry / cache), mutating (exactly-once via request ids), streaming
  (RPC_STREAM frames). The transport enforces what REST left to convention.

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

## 4b. The wire is binary — no document transport

The same thesis that produced the DOM ABI (binary frames beat text at every
boundary; measured 29 ns/op vs the JS layer) applies below: **HTTP is a
document-delivery protocol and next.0 does not ship documents.** It ships
chunks, symbols, and RPC frames. The native wire has no HTTP anywhere:

```
native transport := QUIC datagrams/streams carrying length-prefixed frames
frame            := u8 kind | varint len | body
kinds            := SYMBOL   (root, block, symbol id, payload)   - fountain rx/tx
                    HAVE/WANT (compact bitsets, not piece lists) - swarm gossip
                    RPC      (CBOR body, §4)                     - services
                    MANIFEST (signed record)                     - resolution
```

What that deletes, per request, versus the HTTP-document web:

- **Header tax**: HTTP request+response headers run 0.5–2 KB of *text* per
  object (cookies, accept lines, cache directives, CORS theater), often
  exceeding the payload for small objects. Frames carry a few bytes of
  binary framing; there are no per-object negotiations because content is
  self-describing by hash.
- **Envelope tax**: no JSON-for-everything (2–5× size vs CBOR/raw, plus
  parse cost), no base64 (+33% for binary-in-text), no multipart MIME, no
  chunked-transfer text framing. Numbers travel as numbers.
- **Semantic tax**: no per-object request/response round trip — fountain
  symbols stream until the receiver says stop (WANT bitset flips), which is
  where the latency win compounds with §3: request-less transfer means the
  slowest link adds ε symbols, not RTTs.
- **Connection tax**: one QUIC association multiplexes swarm + RPC +
  resolution; 0-RTT resumption for repeat peers. No TCP+TLS+H2 stack-up,
  no head-of-line blocking, no per-origin connection pools.

The gateway ramp (§3) is the deliberate exception, and even there HTTP is
demoted to a **dumb byte pipe**: one `GET /b3/<root>?blocks=...` returns a
raw frame stream (`Content-Type: application/web-frames`); headers amortize
over megabytes, not per object, and nothing above the socket is HTTP-shaped.
Legacy compatibility costs one header block per bundle, not one per asset —
the 100-requests-per-page waterfall dies with the page model itself.

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
  src/webproto/          resolver (b3/ed/dns ramp), manifest verify
  src/webproto/gateway.rs  HTTP(S) chunk fetch (ureq today, QUIC later)
  src/webproto/swarm.rs    DHT + peer protocol + fountain rx/tx (phase 3)
  src/rpc.rs        rpc host import + event delivery (mirrors fetch)
docs/
  WEB-NAMING.md  WEB-BUNDLE.md  WEB-SWARM.md  WEB-RPC.md   (specs first)
```

## 7. Honest risks / open questions (critique targets)

0. **Country-rooted hierarchy (§1d) is the sharpest live question.** Cam
   wants `country.org.team.app`; I made country an *optional vouching
   overlay* over key-delegation, never a seizing root, because an
   authoritative jurisdiction root is a censorship chokepoint that betrays
   the whole project. Tension: legal-recourse and human-legibility *want* a
   real jurisdiction anchor; censorship-resistance *forbids* it being
   authoritative. My line — vouch-not-own, org self-sovereign by default —
   may be too clever; a state simply won't accept "you can label but not
   control." Does the provenance overlay actually get used if it has no
   teeth, or does it collapse back to either DNS or irrelevance?
1. **Human names (§1b/§1c).** Non-exclusive + keytag-anchored + namespaces-
   as-commons kills the parking *economy* (no exclusivity, no resale, no
   passive yield) and the scarcity that drove it — but trades a hard
   guarantee for soft resolution: bare `web://bank` is no longer a promise,
   it's a per-user lookup. That's honest (exclusive FCFS was *also* a
   phishing vector — a squatter just grabs `bank`), and safety moves to
   petnames + trust attestations + the keytag the chrome always shows — but
   it puts real weight on the default trust-ranking, which is a kingmaker
   surface. Three governance questions I can't answer alone: (a) who runs
   the first quorum of logs, (b) who curates the default commons-namespace
   set shipped in the browser, (c) how is the default soft-resolution
   ranking kept from becoming a single vendor's chokepoint? My instinct:
   all three pluggable + auditable, none baked in, ship with a small
   plural default and let it fork. Is that a cop-out or the whole point?
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
   degrades into cacheable atoms; §2b's zombie-data section is the honest
   boundary of what survives a dead operator.)
7. **Purge is advisory** (§2b): retain/sunset/TTL are signed hints that
   default-configured clients respect; a hostile pinner can keep anything
   forever (as on today's web — right-click-save exists). Is hint+default
   enough, or does anything here need to be harder?
8. **Spec-first discipline**: 4 spec docs before serious code. Agreed?

## 8. Phases (each lands a demo in this repo)

- **P0 — specs.** WEB-NAMING / WEB-BUNDLE / WEB-RPC drafts (WEB-SWARM
  sketched). Exit: you've critiqued them.
- **P1 — hashes over HTTP.** Chunk store + bundler CLI (`web pack`) +
  `web://b3:...` loading via a gateway (a static file server) with full
  verification + offline relaunch from the store + compile cache.
  *Demo: task-dashboard-c loads from `web://b3:...`, then loads again with
  the network cable pulled.*
- **P2 — publisher identity + lifecycle.** `ed:` manifests with the §2b
  lifecycle fields, `web publish`, update flow with delta fetch (only
  changed chunks move), BUNDLE_OUTDATED event + gwbc `onUpdateAvailable`,
  store GC honoring retain/sunset. *Demo: v2 of the dashboard ships by
  moving <10% of its bytes; the still-running v1 shows an update banner
  within a minute; `web publish --sunset` makes the store stop seeding it.*
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
