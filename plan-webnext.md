# Web next.0 — a transport, naming, and RPC layer for WASIBrowser
### Plan draft 5 — for critique, not yet pinned

The app layer is done differently already (wasm-first, no JS, binary DOM ABI).
This plan does the same to everything *below* the app: how apps are named,
found, moved, and how they talk to services. "web3" burned the branding on
tokens; none of this uses a coin, a chain, or consensus. Everything here is
hashes, signatures, and caches — boring cryptography, no speculation.

> **Draft-2 note.** The structure below keeps draft-1's §1–§8 numbering (an
> active round-by-round review references it) and prepends the missing spine:
> a **North Star** (§0) and an **invariants-vs-aspirational tiering** (§0b),
> plus per-section maturity tags. A full renumber to a clean
> North-Star → Invariants → Runtime → Bundle/Identity → RPC → Distribution →
> Naming → Privacy → Governance → Phases order is deferred to draft-3
> cleanup, once the ideas stop moving — renaming sections mid-review costs
> more than it buys.

## 0. North Star

WASIBrowser is not a faster webview and not a JavaScript replacement. It is a
different **contract** between applications, users, services, and the
network. One sentence unifies the entire document:

> **The current web made LOCATION the root of trust.
> next.0 makes VERIFIABLE IDENTITY and CONTENT the root of trust.**

`https://example.com/app` means: trust DNS, trust the CA tree, trust the
server, trust the current deployment, trust cookies/session, and trust that
the URL still means today what it meant yesterday. `web://b3:<hash>` means
*these exact bytes*; `web://ed:<pubkey>` means *whatever this publisher key
currently signs*. Everything else flows from that one shift:

- Web binds app identity to **server location** → next.0 binds it to
  **verifiable content + publisher keys**.
- Web grants apps **ambient authority** (cookies, origins, hidden browser
  state) → next.0 grants **explicit host-mediated capabilities**.
- Web treats **offline as an exception** → next.0 treats **local verified
  storage as the default**, the network as a source of missing chunks,
  updates, and live services.
- Web makes **names scarce property** → next.0 treats **names as social
  hints over cryptographic identity**.
- Web assumes the **origin server is the center** → next.0 assumes **any**
  cache, peer, gateway, archive, or USB stick can serve the bytes, because
  trust comes from verification, not location.

The corollary that governs every user-facing surface:

> **Discovery may be social, searchable, and convenient. Execution must be
> cryptographic, verified, and capability-scoped.**

A search engine, an index, a shared link, a friend's petname — all are
*pointer sources*. They help you find; they never confer trust. The browser
verifies what you found (`b3:` bytes, `ed:` signature) before a single
instruction runs. That is what lets discovery stay as easy as today's web
while execution stops inheriting today's web's trust-by-location. (Worked
end-to-end in §9's three flows.)

### The three layers (know which one you're reading)

1. **Runtime layer** — the WASM-first browser/app runtime: binary DOM ABI,
   host capabilities, storage, events, permissions, RPC host. *Mostly built
   — see README.md / plan-blitz.md.*
2. **Sovereign-app layer** — apps as signed, content-addressed bundles:
   `b3:` immutable apps, `ed:` publisher identity, lifecycle, offline
   relaunch, delta updates, local + compile cache. *This plan, phases P1–P2.*
3. **Web-next layer** — the aspirational network: native names, delegation,
   swarm distribution, fountain transfer, oblivious fetch, IPv6-native peer
   services, petnames. *This plan, P3+ — designed now so early choices can't
   foreclose it, landed later.*

## 0c. Getting started, reach, and migration (the developer's first question)  *(Tier 0 core UX; the full `publish` loop lands at P2)*

A protocol nobody can adopt is a paper. The honest answers to "how do I try
it," "who can open my link," and "what happens to my existing app":

**Hello world in five minutes** (the whole loop, no ceremony):

```
$ web keygen                 # once: writes ~/.web/id.ed25519 (your publisher key)
$ web new hello              # scaffold: hello/app.c + manifest.toml
$ cd hello && edit app.c     # a gwbc component; no JS, no build config
$ web pack .                 # -> hello.webbundle  (content-addressed, b3:…)
$ web publish --key ~/.web/id.ed25519 --channel stable
  published  web://hello~blue-otter-cedar   (ed:cam.blue-otter-cedar, seq 1, 41 KB)
$ web open web://hello~blue-otter-cedar      # runs locally; also now offline-cached
```

`new`, `pack`, `publish`, `open` is the whole toolchain surface — no bundler
config, no framework install, no server. **Phasing honesty:** `web pack` +
`web open web://b3:<hash>` is the P1 experience (content-addressed, no
identity); the `publish` step (an `ed:` manifest + a `~word-keytag`) is P2,
and word-keytag rendering lands at P2.5 (§8). Until then a first app is shared
by its `b3:` hash. Nothing here claims the identity loop works before P2.

**Who can open it — the reach problem, answered honestly.** A new runtime
starts at zero reach; pretending otherwise is how "better webs" die. Two
concrete on-ramps instead of "install our browser or leave":

1. **Embeddable engine + extension — full verification, capped capabilities.**
   next.0 execution ships as a spec + a small reference engine that plugs into
   existing browsers as a WASM module / extension, so Chrome/Firefox/Safari
   users can open `web://` links *without switching browsers*. **Verification
   is identical to native: the extension performs the same `b3:`/`ed:` hash
   and signature checks — C0/C1 are pure cryptographic facts and do not get
   "less true" because the UI is a popup.** What the extension cannot do is
   draw genuine OS/browser-native chrome (§10.6), so what's capped is **which
   manifest capabilities may EXECUTE, never whether verification happened**:
   security-sensitive capabilities — payments (§4a), identity-sharing, private
   profiles — require the native WASIBrowser install, because their prompts
   demand trusted chrome the extension can only approximate. For everything
   else, the extension uses the host browser's own trusted surfaces (action
   popup, `chrome.identity`-class prompts) for a persistent, non-dismissable,
   host-drawn identity badge — never a content-drawn overlay a hostile page
   could clickjack. So: same verification, same C0–C4 ladder shown honestly,
   fewer capabilities executable — §10.6's "trusted chrome is sacred" is
   honored by *withholding capabilities that need it*, never by faking it.
   **Tradeoff a commercial dev must plan for:** since payments are
   native-only, a monetized app's extension-mode experience is necessarily
   read/preview-only — the extension buys reach for the *free* funnel, the
   native install unlocks the *paid* one (stated so no one is surprised after
   building; see §4a).
   *Host-detection for graceful degradation:* the host exposes a read-only
   `host.trust_tier` (native | extension) so an app can adapt its own UI —
   hide the pay button and show "open in WASIBrowser to purchase" under the
   extension, rather than letting a native-only capability silently fail. The
   two-tier experience is a first-class, detectable state, not a surprise.
2. **Honest v1 scoping.** Until native reach exists, v1 explicitly targets the
   audiences where "you need the runtime" is already acceptable: internal
   tools, LAN/classroom/offline deployments, developer tooling, and
   privacy-sensitive apps whose users *want* the guarantees. General-web
   replacement is the horizon (§7), not the v1 claim.

**Migration — there is no magic port, and the plan won't pretend there is.**
No JS → no automatic React/Vue/Svelte port; that is a real cliff (scorecard:
authoring floor 7→5, tooling 8→3). The graded on-ramp:

- **Greenfield** apps are written directly against the gwbc/Go/Rust SDKs.
- **Legacy apps stay reachable via `dns:` — as the OLD web, clearly demoted,
  NOT ported.** Opening `dns:x.com` (§1a) launches the existing site in a
  demoted, explicitly-labeled **legacy browsing surface**: its own JS,
  cookies, and CA-TLS apply and it runs exactly as it does in Chrome today —
  with **zero** next.0 properties (no verification, no offline, no permanence,
  no capability sandbox). This is the honest escape hatch, equivalent to
  clicking an external link; the Tier-0 "no cookies, ever" invariant governs
  *next.0 apps*, and this surface is explicitly *not* one — it is the old web,
  reachable but quarantined behind a legacy-web label so a user always knows
  which world they're in. It is a *reachability* ramp, not a migration.
- **A compatibility shim — a SEPARATE, narrower mechanism** (do not conflate
  with the `dns:` surface above): it repackages a site's *assets* as a
  verified next.0 bundle. A genuinely *static*
  site shims cleanly — full functionality, plus the permanence + offline wins,
  no rewrite. An *SPA* shims only its static shell: packaging preserves the
  markup/media bytes (so they're content-addressed, offline-cacheable, and
  permanent), but the JS-driven behavior *is* the app and does not survive —
  a real SPA becomes a rewrite against the gwbc/RPC model, not a free port.
  The shim is an asset-permanence on-ramp, not a behavior-preservation one,
  and the doc won't let "SPA" ride along with "static" as if the trade were
  symmetric.

The pitch to a working dev is therefore not "rewrite everything to reach
nobody"; it is "greenfield here for the guarantees, keep your legacy app
reachable over the ramp, and adopt incrementally." (Onboarding + static shim
are P1 deliverables, §8.)

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

## 0b. Core invariants vs. aspirational surface

Not every mechanism here has the same maturity, and the plan must not pretend
it does. Four tiers — and the one rule that ties them together: **the first
implementation may be small, but it must be architecturally faithful.** It
may skip any accelerator or social layer; it may NOT bake in an assumption
that forecloses one.

**Tier 0 — Invariants.** True from P1, non-negotiable, load-bearing for
correctness; everything rests on these:
- content-addressed bundles (`b3:`) with mandatory client-side verification
- publisher identity as keys (`ed:` signed manifests), never as servers
- host-mediated capabilities — **no ambient authority**, ever
- the binary DOM ABI and the binary RPC ABI (frames, not documents)
- a local verified cache as the default execution substrate (offline-first)
- **no cookies, no global user id** — *structural invariants, present day
  one.* (These are Tier 0, not civilizational goals: they require us not to
  build a thing, not the world to adopt one.)
- **gateways are disposable** — P1 may fetch over a single HTTPS gateway,
  but every line must behave as if that gateway is an untrusted, replaceable
  cache. The future is encoded into the present here or nowhere.
- **the host is the security kernel** — apps are sandboxed WASM with only
  manifest-declared capabilities; no app draws browser chrome, receives
  ambient authority, or escalates on update silently (§10).
- **verification is non-bypassable** — a hash or signature mismatch is a
  hard stop. There is no "continue anyway" button; mismatch means corruption
  or attack (§10). Authority is cryptographic, never linguistic: a *string*
  name never grants trust — only a hash, a key, or a pinned binding does.

**Tier 1 — Distribution accelerators.** Additive, swappable, none load-
bearing for correctness: HTTP gateways, LAN peers, DHT discovery,
RaptorQ/fountain coding, IPv6 multicast, relay/oblivious fetch. Losing every
one of them degrades to "fetch from a gateway," never to "broken."

**Tier 2 — Human/social layer.** Designed now, requires real network effects
to be *usable*, not shippable in isolation: petnames, native log names,
transparency logs, delegation chains, namespace commons, country/org/team/
app provenance.

**Tier 3 — Civilizational claims.** The destination, achieved only at scale
and adoption: no registrars, no origin monopoly, no app-store monopoly, no
server-location-as-identity. Distinct from Tier 0 precisely because these
need the *world* to move, where "no cookies" needs only *us* to abstain.

Section headers below carry a **[Tier N]** tag so it's always clear whether
you're reading an invariant, an accelerator, a social layer, or a horizon.

---

## 1. Naming: kill the location, keep the name  *(Tier 0 core + Tier 2 names)*

A URL today conflates four things: *identity* (which app), *version* (which
bytes), *location* (which server), and *route* (which screen). next.0 splits
them:

```
web://<authority>/<path>[?query]

authority :=  b3:<blake3-hash>            immutable bundle (identity == bytes)
           |  ed:<ed25519-pubkey>         mutable identity (signed pointer)
           |  @<petname>                  YOUR petname book only — local,
                                          never shared (§1b); the app never
                                          offers "copy @name" — share emits ~
           |  <label>[.<label>...]        native name: a delegation path,
                                          root-left — top label §1b, chain §1d
           |  <label>[...]~<keytag>       any label pinned to its key (§1c) —
                                          keytag is WORD-ENCODED (below)
           |  dns:<name.tld>              legacy DNS (tag optional — silent
                                          fallback, §1a)
path      :=  route WITHIN the app (the app owns it; never a server path)
```

**The spine of the model — one ladder, absolute → social:**

```
web://b3:blue-otter-cedar-…    these exact bytes             ABSOLUTE
web://ed:cam.blue-otter-cedar        what this key signs           CRYPTOGRAPHIC
web://cam~blue-otter-cedar           human label + crypto anchor   VERIFIABLE
web://@maya                    your petname book (local)     PERSONAL
web://maya                     global soft lookup            SOCIAL
web://google.deepmind.chat     signed delegation chain (§1d)
web://us.google.deepmind.chat  optional jurisdiction view (§1d)
```

> Hashes are absolute. Keyed names are cryptographic. Bare names are social.

Trust degrades *gracefully* down the ladder and is **never bootstrapped from
location**. A name is not property; it is a human hint attached to a key, and
the key (or the hash) is the identity.

- **`b3:` — content identity.** The authority *is* the BLAKE3 root hash of
  the bundle. Anyone — peer, CDN, USB stick — can serve it; the client
  verifies. Immutable, cache-forever, offline-forever.
- **`ed:` — publisher identity.** An Ed25519 public key. It resolves to a
  small **signed manifest**: `{seq, bundle: b3:..., prev, title, publisher,
  sig}`. Updating an app = publishing a new signed manifest. Trust is in the
  key, not any server. Key rotation via signed successor record; loss of key
  → §7 recovery (personal M-of-N *or* the institutional custody tier, §10.5).

## 1a. Address ergonomics — the part that actually decides adoption  *(Tier 0 UX)*

Raw hashes and pubkeys are unspeakable, untypeable, and unmemorable, and a
naming system that fails the billboard/phone-call test loses to `x.com` no
matter how good its cryptography is. Five hard rules make the addresses
human — they are Tier-0 UX, not polish:

1. **No raw hash or pubkey is ever the primary user-facing string — but the
   keytag is never optional.** Every manifest MUST carry `title` + `publisher`
   labels; chrome renders `Title · publisher · short-tag`, full hash behind
   copy-link / QR. **Labels are free text and therefore NOT trusted:** a
   phishing bundle can self-declare `title: "Bank of America"`. Two defenses
   make that inert. (a) The `~keytag` is *unavoidable*, always shown adjacent
   to the label, never suppressible — the label is a hint, the tag is the
   truth. (b) The **first time** a `publisher` label is seen bound to a *new*
   key, chrome visibly demotes/flags it ("new publisher — not one you've
   trusted"), rather than rendering it neutrally beside a familiar-looking
   name; a label only earns un-flagged chrome once its key is pinned
   (petname/history). So "Bank of America · <unknown tag> · NEW" reads as
   suspicious, not legitimate — the population §10.7 worries about sees the
   flag, not just the name. Even an anonymous `b3:` bundle shows `Untitled ·
   b3:blue-otter-cedar…`, never 64 raw hex.
2. **Keytags are word-encoded, not hex.** A keytag is drawn from a fixed
   2048-word dictionary (BIP39-style: short, unambiguous, autocorrect-able,
   no homographs), ~11 bits per word. The **minimum is 3 words / ~33 bits**
   (`cam~blue-otter-cedar`), extending to 4+ as a name gets popular.
   Speakable over a phone, typo-resistant (dictionary snap), no shift keys.
   Fixed-hex tags are dead.
3. **Keytag length scales with claim density; collisions are resolved at
   CLAIM time, not just display time.** The browser shows the shortest tag
   (≥ the 3-word floor) that is collision-free against the name's visible
   quorum claim set. Crucially, uniqueness is enforced when a name is
   *claimed*, not merely when it's shown: **the first key to claim a given
   `name~tag` at a given length owns that short form permanently; any later
   claim whose tag would collide is rejected at registration and must take a
   longer tag.** So an already-printed `cam~blue-otter-cedar` can't be
   silently shadowed by a newcomer — the short form is a first-claim resolved
   by the log quorum. (One honest caveat: the quorum is federated and
   asynchronous, so two concurrent claims for the same short tag hitting
   *different* logs before cross-sign can both look briefly valid; the tie
   breaks deterministically at quorum, and the loser is auto-bumped to a
   longer tag — a brief pre-quorum ambiguity window, not indefinite. Chrome
   shows a tag as "confirmed" only post-quorum.) (This closes the
   draft-2 contradiction where a fixed 4-hex/16-bit tag both claimed
   "infinite anchors" *and* fell to a birthday collision and to
   prefix-grinding.)
4. **Input is lenient; canonicalization is strict.** The address bar accepts
   `cam~blue-otter-cedar`, `cam blue otter cedar`, `cam-blue-otter-cedar`,
   and pasted links interchangeably, normalizing to the canonical `~` form —
   so `~` (a shift/long-press key on many layouts) is never a *required*
   keystroke. Over a phone: *"cam, tilde, blue otter cedar"* dictates as
   `cam~blue-otter-cedar`; the listener types the label, then three
   dictionary words the keyboard auto-completes, and the browser normalizes
   spacing/casing — the word/label boundary is the `~` (or the first
   dictionary word), never ambiguous run-on.
5. **Legacy `dns:` resolves silently — and legacy TLDs are RESERVED from
   native claiming.** Typing a bare dotted string (`amazon.com`) whose final
   label is in the browser's legacy-TLD table is resolved via `dns:`,
   shown labeled **"legacy web (DNS)"**, never confused with a native
   identity. Critically, to prevent a hijack — an attacker claiming native
   top-label `com`, then signing a delegated `amazon` child to intercept
   `amazon.com` before the ramp is tried — **every string in the legacy-TLD
   table is reserved: `com`/`org`/`net`/`io`/… cannot be claimed as a native
   top label at all (§1b claim validation rejects them).** So a dotted string
   ending in a reserved TLD has *no* valid native chain to shadow it and
   always routes to `dns:`; §1b's "native and legacy cannot collide" holds by
   construction, not by resolution-order luck. The table itself is not a
   private vendor list — it ships **inside the signed, forkable defaults
   bundle (§1c)**, versioned and auditable like the other three lists, and it
   decides *only* the reserved set + DNS-ramp eligibility, never what is
   trusted (§10 R1 still holds).

> Net: users type what they always typed (`amazon.com` still works), share a
> speakable pinned form (`cam~blue-otter-cedar`), and never hand-copy a hash. The
> ugly cryptographic truth stays *underneath* the human label, exactly where
> §10.7 needs it.

## 1b. Human names without registrars  *(Tier 2 — social; designed now, lands at scale)*

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

- **No TLDs — and both legacy TLDs and keytag-shaped strings are reserved.**
  TLDs exist to shard registry databases; delegation (§1d) shards by key
  instead, so `.com` dies with the registrar. A bare top label (`google`) is
  a §1b claim; a dotted path (`google.deepmind`) is a delegation chain (§1d);
  a dotted string ending in a legacy TLD routes to `dns:` (§1a rule 5).
  **Claim validation rejects two reserved string-classes at claim time:**
  (a) any top-label in the legacy-TLD reserved set (`com`/`org`/`net`/`io`/…)
  — so no native chain can shadow a legacy domain; and (b) **any string
  matching the keytag word-pattern** (hyphen-joined dictionary words, e.g.
  `blue-otter-cedar`) — so nobody can register a bare name that is visually
  identical to someone's `~keytag` minus the tilde. Without (b), a
  dropped-tilde or chat-app-mangled link (`cam~blue-otter-cedar` →
  `cam blue-otter-cedar` → a claimed bare `blue-otter-cedar`) would land on a
  hostile squat instead of erroring — a confusable class the old hex tags
  couldn't have (nobody types hex as a name), closed the same way TLDs are.
  Native/legacy *and* name/keytag collisions are impossible by construction.
- **One claim, many apps.** A name binds to one key; that key's manifest
  declares `kind: app` **or** `kind: directory`. A directory is a signed map
  of sub-apps (`todos -> b3:…, blog -> b3:…`) selected by the first path
  segment; an app owns its whole path. The explicit `kind` discriminator
  removes any ambiguity between "`cam/blog` = the directory's blog app" and
  "`cam/blog` = a route inside a single `cam` app." Namespacing comes from
  the publisher, not the registry — nothing new to squat.
- **Confusables are folded before claiming.** Names are stored and matched
  by their skeleton (Unicode confusable-folding; ascii-only in v1 makes
  this trivial): `cam`, `càm`, and cyrillic `сam` are the *same claim* —
  the phishing classic dies at the registry instead of in the URL bar.
- **The browser shows the word-keytag.** Chrome (not the name itself)
  displays `cam · blue-otter-cedar` — the word-encoded fingerprint of the bound
  key (§1a). Names are for humans; the keytag is the tell when something
  claims to be `cam` but isn't bound to the key your petname book / history
  knows.

Zooko's triangle honesty: this is *trust-minimized*, not trustless — global
human names need SOME shared ordering, and a quorum of mutually-auditing,
individually-disposable logs is the cheapest one that doesn't mint a token
or a landlord. Below it, **petnames** remain first-class: the browser lets
users/communities bind their own labels to `ed:` keys (imported sets, like
a contacts book), which covers the "names I actually type" case with zero
global coordination at all.

**The `@` sigil = your book, and it NEVER leaves your machine.** `web://@maya`
resolves *only* through your local petname book — no global lookup, no
ambiguity, no chooser. It is deliberately **not** the shareable form: 15 years
of Twitter/Mastodon/email trained everyone that `@name` is a global handle, so
next.0 makes the *portable* form the pretty one instead. The browser's
**Share** button always emits `maya~dusk-otter-cedar` (the pinned, portable,
key-anchored form), never `@maya`, and the UI never offers "copy @name" —
killing the "I sent my friend @maya and it didn't work" footgun before it
exists. First contact is explicit: you open a shared `maya~dusk-otter-cedar` (or
`ed:` link), chrome shows `Maya · dusk-otter-cedar — unknown to you`, and one tap
"save as Maya" writes `Maya → ed:5cb92a…`, keytag pinned. Thereafter `@maya`
is a hard, personal, typeable binding, and the pinned keytag is the
impersonation tripwire: if a later `maya` presents a different key, chrome
flags `dusk-otter-cedar ≠ saved` rather than silently resolving. Bare global
`web://maya` stays soft/social (§1c); `@maya` is yours and typed; `maya~keytag`
is what you share.

## 1c. Expansive & equitable: why no one parks  *(Tier 2 — social)*

An exclusive flat namespace ("the one global `cam`, earliest wins") is the
*least* equitable design — it manufactures scarcity and rewards the fastest
land-grabber. Draft-1's FCFS-exclusive framing was wrong on this axis;
§1b above is now non-exclusive, and this section is why that makes parking
economically dead rather than merely discouraged.

**The parking economy needs three legs. next.0 removes all three:**

1. **Exclusivity — gone.** You cannot corner a name others may also claim.
   The durable, globally-unique address is `name~keytag` — the word-encoded
   fingerprint of the bound key (§1a): `web://cam~blue-otter-cedar`. Because the
   keytag *lengthens with claim density* (§1a rule 3, hard floor ~33 bits,
   growing), the anchor space per string is effectively unbounded *and*
   collision-safe at every popularity level — every person named cam gets a
   real, permanent `cam~<theirs>`. Bare `cam` is only a local convenience
   (§1b), never a deed.
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

**The defaults — four INDEPENDENTLY-forkable sub-objects, not one bundle.**
Soft resolution needs *some* shipped defaults: the trust-ranking, the
commons-namespace set, the R7 high-risk category list (§10.2), and the
legacy-TLD reserved/ramp table (§1a rule 5). Consolidating them into one
object would concentrate risk — a single bad actor or coerced publisher would
miscalibrate all four at once, and a community disputing *one* would have to
fork *all*. So instead: a top-level **defaults manifest** references four
**independently-versioned, independently-forkable `ed:` sub-objects**, each
with its own signer. A community can adopt someone else's trust-ranking while
keeping the spec's TLD table, or fork only the R7 list — mix and match, no
monolith. Each sub-object's initial content is published *with the spec*, and
the default trust-ranking is a documented, criticizable function, not a black
box:

```
default_rank(claim) = w1·petname_import_frequency   (how many books pin it)
                    + w2·log_quorum_age             (how long quorum-attested)
                    + w3·revocation_clean_history
                    − w4·look_alike_penalty         (confusable to a pinned name)
```

The **legacy-TLD sub-object mirrors IANA's root zone + pending-delegation
set** on a defined cadence (tracking *applied-for* gTLDs, not just live ones,
so the reservation leads go-live and closes the race window). Retroactive
reservation **grandfathers** any prior native claim — safe because dns-routing
keys on a string's *final* label while native claim-blocking keys on its *top*
label: a grandfathered native top-label `zip` only affects `zip.*` native
addresses, never `*.zip` legacy ones, so it can't shadow the eventual gTLD.
Any user or community swaps any sub-object in one action; a fork ships its own.
Residual soft power of "the default most never change" is real and stated
(§7.1) — but four specified, auditable, independently-forkable defaults beat
one unspecified monolith.

**Equity is not automatic — the honest open issues:**

- *Script equity.* ASCII-only v1 privileges Latin scripts — and this extends
  to the keytag layer too: the BIP39-style word dictionary (§1a) is
  implicitly English/Latin, so word-tags are less speakable/memorable for
  non-English users, not just bare names. Per-locale keytag dictionaries +
  full IDN with confusable-folding are a v-next *requirement*, not a nicety,
  or the "equitable for all participants" claim is hollow for most of the
  planet.
- *Compute equity.* Anti-sybil PoW (§1b) taxes the resource-poor if it's
  heavy. It must stay cheap + one-time, or move to rate-limits / verifiable
  delay functions (wall-clock, not hashpower) so a phone can claim a name
  as easily as a datacenter.
- *Discovery equity.* Soft resolution must not collapse to "rank by
  popularity" — that's rich-get-richer with extra steps. Petname/history
  first + an explicit chooser keeps the long tail reachable; the default
  trust-ranking is itself a kingmaker surface and must be pluggable,
  auditable, and never a single vendor's list.

## 1d. Hierarchy by cryptographic delegation (not by decree)  *(Tier 2 — social)*

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
armies. The governing principle, stated flat:

> **Jurisdictional hierarchy is a *view*, not a root of existence. A country
> can vouch for an entity; it cannot cryptographically erase it.**

Three rules make that principle enforceable, so the readable hierarchy never
becomes DNS-with-a-flag:

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

## 1e. Discovery: how a URL-less web stays searchable  *(Tier 2 — social)*

If there are no origin URLs to crawl, how does anyone *find* anything? The
same way trust works: discovery is a separate, untrusted layer that only ever
produces **pointers**, which the runtime then verifies (§0's principle made
mechanical).

- **What gets indexed**: `b3:` bundle roots, `ed:` publisher manifests,
  `name` / `name~keytag` labels, delegation paths — harvested from HTTP
  gateways, public manifest feeds, publisher submissions, and crawled
  *signed* directories (a directory bundle is itself `b3:`-addressable, so an
  index is auditable against its source).
- **A search result is a pointer, not bytes.** "SemanticPortrait ·
  cam~blue-otter-cedar · runs offline · 12 MB" is metadata over an `ed:`/`b3:` authority;
  clicking runs the standard resolve → verify → permission → load path (§2).
  An index may lie about *ranking* and *description*; it cannot forge
  *identity*, because identity is the hash/key the browser re-verifies.
- **Search is just an app + a service** (§4): `web://google.search` is a
  signed app calling a `search.query` RPC (Flow 2, §9). There is no
  privileged search API — anyone can publish a competing index, and the
  browser trusts none of them with *execution*.

So Google (or any engine) can index the WebNext object graph and stay
maximally useful, while losing the one power it must never have: changing
what the bytes *are*. **Discovery is allowed to be centralized and convenient
precisely because it confers no trust.**

## 2. Bundles: the unit of distribution  *(Tier 0 — invariant)*

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

## 2b. Versioning, freshness, and natural decay  *(Tier 0 signed lifecycle + Tier 1 GC/decay)*

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
Per-user live data whose operator vanished is not magically resurrectable —
no protocol revives a dead database. But that must not be a shrug, because
**most real software (SaaS, CRUD, anything with per-user state) is exactly
this shape** — not the read-mostly blog the "immortal bytes" pitch flatters.
So the honest position, sharpened:

- **The pitch is scoped honestly.** next.0 is the strongest substrate for
  static/read-mostly apps *and* for RPC-backed services with responsible
  operators; per-user live-data durability is the operator's responsibility,
  exactly as it is today. It does not claim to make a careless operator's DB
  survive their carelessness.
- **Service succession is promoted from paragraph to P1/P2 deliverable**
  (§8): a *reference implementation* of a service handing off to a successor
  key mid-session, with the client following the succession record and
  re-verifying — because that mechanism is what actually lets a CRUD app be
  trusted for the long term, and a mechanism only counts once it's testable.
- **Data export is a capability, not a favor.** An app declaring per-user
  storage must expose a signed export (the user's own data as portable `b3:`
  objects), so operator death degrades to "you still hold your data" instead
  of "it's gone" — the sovereignty invariant (§0b) applied to app state.

## 3. Distribution: swarm + fountain, gateway as training wheels  *(Tier 1 accelerators; gateway ramp is Tier-0-faithful)*

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
- **Codec decision (made): default to the open `Wirehair`/LT-code family**,
  behind a `SymbolCodec` trait. RaptorQ (RFC 6330) has better overhead but
  carries Qualcomm RAND patent statements, and shipping the wire format on a
  patented codec then migrating means a format break — inconsistent with the
  no-gatekeeper ethos. RaptorQ stays an *optional pluggable accelerator*,
  negotiated per-transfer, usable only where its RAND terms are acceptable.
  The trait keeps the wire self-describing (codec id per symbol block), so
  the choice never fossilizes.

**Bus-proofing** (the other reading of the brief): every layer above must
survive any single party vanishing — publisher (mirrors keep seeding;
manifests are signed, so mirrors can't tamper), gateway (any other gateway
or peer serves the same hashes), DHT bootstrap (multiple hardcoded +
user-suppliable bootstrap sets; LAN discovery works with zero bootstrap),
even *this project* (the formats are specs first, implementation second).

## 4. RPC: the first-class API model (not a transport detail)  *(Tier 0 — invariant ABI)*

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

### 4a. Payment as a first-class capability (how developers get paid)  *(Tier 1/2 — capability declared, rails unbuilt)*

The web's one native monetization model is surveillance advertising, and
next.0 deletes its fuel (cookies, tracking, ambient identity). Removing the
bad model without offering *any* replacement would be a regression for the
majority of working developers (commercial SaaS, content, indie) — so payment
is a declared capability, peer to storage and RPC, not an afterthought:

```
capability payments := a declared payment-service the app may call, exactly
  like any RPC service (ed: identity, iface hash, user-granted, revocable)

  payments.quote(item, amount, currency)      -> signed quote
  payments.charge(quote_id)                   -> one-shot, user-confirmed
  payments.subscribe(plan_id)                 -> recurring, user-confirmed
  payments.status(id) / payments.refund(id)
```

- **Rail-agnostic**: the payment *service* bridges to whatever settles —
  cards, bank rails, a stablecoin, a prepaid browser balance — the app and
  the browser only see the capability + signed receipts. next.0 does not mint
  a coin (it never does); it standardizes the *request*, not the money.
- **User-confirmed in trusted chrome** (§10.6): a charge is a browser-native
  prompt showing amount + payee identity (`Reflection Service · moss-lantern-quill`)
  — apps can't draw fake payment UI, and app-scoped identity means the payee
  learns only a per-app pseudonym, not a cross-site profile (privacy that ad
  networks structurally cannot offer).
- **Micropayments become viable** precisely because there is no per-request
  session/redirect tax: "$0.002 per article, no account" is one signed
  `charge`, enabling models the ad web made impossible. This is a *feature
  the incumbent can't match*, not just parity.

Honest scope: this **declares the capability and its trust/privacy shape** —
which is necessary, not sufficient. The hard parts — an actual settlement-rail
operator, KYC/AML and compliance, chargeback/dispute resolution beyond a bare
`refund`, and bootstrapping a two-sided market (payer *and* payee both need a
working rail) — are **unsolved and out of scope for draft 3**. What this
changes is the *shape* of the answer to "how do I get paid": from "run an ad
network / data broker" to "declare a payment capability and charge directly,"
with a real path (the RPC + capability machinery already exists) rather than a
void. It narrows the monetization hole; it does not claim to have closed it.

## 4b. The wire is binary — no document transport  *(Tier 0 — invariant)*

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

## 5. Privacy: separate the who from the what  *(Tier 0 wire encryption + Tier 1 oblivious mode)*

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
  src/policy.rs     AppPolicy + per-import capability gates (§11.3) — the
                    policy layer over Wasmtime's sandbox (§11.1)
  src/taskmgr.rs    Store::limiter + fuel/epoch budgets -> health states,
                    the Task Manager (§11.4)
  src/update.rs     4-axis update diff + Auto-safe/Ask/Block + rollback (§11.6)
docs/
  WEB-SECURITY.md  WEB-NAMING.md  WEB-BUNDLE.md  WEB-SWARM.md  WEB-RPC.md
  WEB-PERMISSIONS.md (§11 capability vocab + update UX)   (specs first)
```

The division of labor is the whole design: **Wasmtime** = memory isolation,
WASI sandbox, preopen model, fuel/epoch, resource limits, compile cache;
**WASIBrowser** = manifest, permission vocabulary, trusted prompts, capability
gates, RPC policy, identity chrome, update diffing, rollback, safe mode,
per-app privacy profiles (§11).

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
1. **Human names + the three kingmaker surfaces — now with a concrete
   mechanism (§1c-defaults).** Non-exclusive + keytag-anchored + namespaces-
   as-commons kills the parking *economy* but trades a hard `web://bank`
   guarantee for soft resolution, putting weight on three defaults that could
   silently centralize: the trust-ranking, the commons-namespace set, and the
   R7 high-risk category list. **Decision:** all three ship as ONE signed,
   versioned **defaults bundle** — itself a `web://` `b3:`/`ed:` object,
   forkable and swappable exactly like an app update (§11.6), with its initial
   content *published alongside the spec*, not deferred. The default
   trust-ranking is a **documented, criticizable algorithm** — `score =
   petname-import-frequency + log-quorum-age + revocation-history −
   look-alike-penalty` — not "TBD." A specified, auditable, forkable default
   is strictly better than an unspecified one; the residual question is only
   *who ships the first bundle*, and the answer is "anyone can ship a
   competing one." Remaining honest risk: most users never swap defaults, so
   the default author has real soft power — mitigated, not eliminated, by
   forkability + transparency.
2. **Key loss — three options, not one (§10.5).** Personal identities use
   M-of-N social recovery — but that fails the *isolated user* with no social
   graph to nominate, exactly the population least served by a social model,
   so it is not the only option: a **cold-backup path** (a printed
   BIP39-style seed phrase, or a hardware key held in a drawer/safe-deposit
   box, with *no named humans*) is a first-class alternative for anyone who
   prefers self-custody over a social graph. Regulated/institutional
   identities add the **institutional custody tier** (§10.5): hardware-backed
   threshold signing via named, bonded custodians that assist *recovery*
   without ever gaining authority over content — the analog to registrar/IdP
   support apparatus, opt-in and non-seizing. Social / self / custodial:
   every user picks the recovery model that fits, none is forced.
3. **DHT realities**: sybil attacks, NAT traversal, mobile churn. The
   gateway ramp means the DHT can be *best-effort* for years without hurting
   reliability. Is that honest enough or does it quietly re-centralize us
   onto gateways (my main worry)?
4. **RaptorQ IPR — decided (§3).** Default to open Wirehair/LT behind the
   `SymbolCodec` trait; RaptorQ is an optional negotiated accelerator only.
   No longer open.
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
8. **High-risk name list is a governance surface (§10.2 R7).** Special-casing
   banks/gov/health for conservative resolution is right, but *who defines the
   category set and the attestations?* A hardcoded list is the same kingmaker
   problem as the default trust set (risk 1). It must be pluggable + auditable
   — but a pluggable safety list that users can disable is also a
   footgun. Unresolved.
9. **"Keys must become human" is existential, not cosmetic (§10.7).** DNS won
   on human-legible names. If the chrome can't make `Maya · dusk-otter-cedar · no new
   permissions` feel as safe and simple as `maya.com` feels today, users
   route around WebNext to the thing they understand, and every security
   property above is moot. This is a UX research problem the protocol can't
   solve alone — arguably the highest-risk item in the whole plan.
10. **Spec-first discipline**: security model (§10) + naming + bundle + RPC
    specs before serious code. Agreed?

## 8. Phases (each lands a demo in this repo)

- **P0 — specs + the existential UX test.** WEB-SECURITY (§10) / WEB-NAMING /
  WEB-BUNDLE / WEB-RPC drafts (WEB-SWARM sketched). **AND the §10.7 gate,
  treated as a real deliverable, not a closing remark:** build the actual
  identity chrome (word-keytag chip, not a diagram) and usability-test it
  against a control group trying to detect impersonation — `google.com` vs
  `Google · slate-harbor-vale`. If it can't beat the naive lock-icon + domain model
  in a real study, the project's core bet ("keys can be made human", §10.7)
  is falsified and *that* is the finding, before more protocol code. Exit:
  specs critiqued **and** the chrome beats the control.
- **P1 — hashes over HTTP + the 5-minute onboarding.** Chunk store + bundler
  CLI (`web new`/`web pack`/`web open`, §0c) + `web://b3:...` loading via a
  gateway with full verification + offline relaunch + compile cache, **plus a
  legacy-app compatibility shim** (package a static/SPA site as a bundle) so
  existing apps get an on-ramp. *Demo: `web new` → `web pack` → open under 5
  minutes; task-dashboard-c loads from `web://b3:...`, then again with the
  cable pulled; a legacy static site opens as a bundle offline.*
- **P2 — publisher identity + lifecycle.** `ed:` manifests with the §2b
  lifecycle fields, `web publish`, update flow with delta fetch (only
  changed chunks move), BUNDLE_OUTDATED event + gwbc `onUpdateAvailable`,
  store GC honoring retain/sunset. *Demo: v2 of the dashboard ships by
  moving <10% of its bytes; the still-running v1 shows an update banner
  within a minute; `web publish --sunset` makes the store stop seeding it.*
- **P2.5 — names, equity, and devtools.** IDN + confusable-folding + a
  verifiable-delay-function claim cost (VDF, wall-clock not hashpower, so a
  phone claims as easily as a datacenter) — *gating any name-claim UI before
  this ships*; word-keytag rendering (§1a); the defaults bundle (§7.1) with
  its published initial content; and a **devtools MVP** (wasm stack traces
  mapped to source, an RPC-call inspector, the §11.4 Task Manager surfaced).
  *Demo: claim a name from a phone; a look-alike claim is flagged; step a
  wasm crash back to its source line.*
- **P3 — RPC v1 + payments + service succession.** Host import + RPC_RESULT
  events + gwbc `useRpc`; rpc-over-HTTPS binding; the §4a `payments`
  capability with trusted-chrome confirm; **a reference service that hands
  off to a successor key mid-session** (§2b), the client following + re-
  verifying. *Demo: RemoteTodos endpoint moves hosts AND rotates to a
  successor key mid-session, nothing breaks; a $0.01-per-read charge clears
  in a browser-native prompt.*
- **P4 — LAN + swarm.** mDNS peer fetch, then DHT + QUIC + Wirehair fountain
  codec (§3). *Demo: two machines, cable between them, no internet: app
  transfers and runs.*
- **P5 — oblivious mode.** Relay protocol + private fetch toggle in the
  toolbar. *Demo: gateway logs show relay ip, relay logs show no hashes.*

**Renderer-capability scoping (honest, per the crown-jewel rule).** The
scorecard is blunt that UI richness regresses (9→7): Blitz trails Blink on
forms/IME/CSS/media. So v1's UI ambition is scoped to what the renderer
already does well — **forms, lists, dashboards, media-light apps** — which is
exactly the internal-tools/SaaS target of §0c. General-web UI parity is a
tracked, resourced work-stream against the vendored Blitz branch, not a v1
claim; next.0 does not assert "build literally anything" until the renderer
can, and says so rather than over-promising the incumbent's crown jewel.

**Every phase is small but architecturally faithful (§0b).** The first slice
is not a toy that gets replaced; it is a small instance of the final
philosophy, carrying the Tier-0 invariants from day one so nothing later is
foreclosed:
- P1 already speaks `web://b3:`, verified chunk store, offline relaunch, the
  binary DOM ABI, **capability declarations + the §11.3 permission gate + the
  §11.4 Task Manager + the §11.7 identity chip** — even with no DHT. (QoL is
  not a later polish phase; the gate and chip ship with the first app that
  loads.)
- P2 already speaks `web://ed:`, signed update manifests, lifecycle, delta
  fetch, update events, **and the §11.6 4-axis update diff + rollback** —
  even with no global naming.
- P3 already speaks host-mediated RPC, service identity, interface hashes,
  declared capabilities, app-scoped caller identity — even with no
  oblivious relay.
Accelerators (Tier 1) and the social/naming layers (Tier 2) bolt on without
touching those invariants.

Sequencing rationale: P1/P2 deliver bus-proofing + data-cost wins with ZERO
novel networking (it's files + hashes + any web server), which means the
risky parts (DHT, fountain, relays) are additive upgrades to an
already-useful system — the same commodity-behind-a-seam bet that worked
for the render engine. The gateway ramp is deliberately Tier-0-faithful: P1
uses HTTPS, but treats the gateway as a disposable untrusted cache, so the
swarm is an upgrade, never a migration.

## 9. Concrete flows (does the model survive contact with users?)

Three walkthroughs at spec density — human surface reducing to protocol.
They double as **acceptance criteria**: if the built system can't do these,
the invariants aren't really wired. The one principle every flow enforces:
*discovery is social/searchable/convenient; execution is
cryptographic/verified/capability-scoped.*

### Flow 1 — Google as bridge (the adoption ramp)

*Human.* User searches normal `google.com` for "offline wasm journal app".
Results interleave `https://…` legacy hits with WebNext cards
("SemanticPortrait · verified · cam~blue-otter-cedar · runs offline · 12 MB"). User
clicks one.

*Protocol.* The card's authority is `web://ed:…` or `web://b3:…`. Google is
a **pointer source only** — the browser ignores Google's copy of the bytes
and runs resolve → fetch chunks → verify BLAKE3 / Ed25519 → check manifest →
show permissions → load `app.wasm` → cache + compile-cache. Google ranks and
describes; it cannot substitute code or impersonate the publisher. *The whole
ramp: the world keeps its search engine, WebNext keeps trust.*

### Flow 2 — Google as a native WebNext app

*Human.* User opens `web://google.search` (chrome: "Google Search · slate-harbor-vale ·
verified"); the cached app paints instantly; a query returns native objects,
each with publisher, bundle hash, last-signed date, offline/RPC flags;
clicking opens an app, a document viewer, or prompts for service permission.

*Protocol.* `google.search` resolves as a delegation path (`google~slate-harbor-vale` key
→ signed `search` sub-app, §1d); its `ed:` manifest verifies to a `b3:`
bundle; the app's declared capability is one RPC service (`ed:<search svc>`,
iface `b3:<schema>`, methods `search.query|suggest`). The app opens no
sockets — it calls `gwb.rpc_call(...)`; the host checks the capability,
derives the app-scoped user key, connects (QUIC/Noise or HTTPS ramp),
verifies the service key, ships the frame, returns `RPC_RESULT`. Identity is
**app-scoped, not a browser-wide cookie**: `app-scoped id → verified service
key → typed method`, never `cookie → session → request`. Google becomes *a
search app + a search service + an index of verifiable objects* — no longer
the authority over the objects themselves.

### Flow 3 — a friend's page (social + scoped identity)

*Human.* Maya shares `web://maya~dusk-otter-cedar`; first open shows "Maya · dusk-otter-cedar ·
unknown to you", the signed profile loads, user taps "save as Maya"; later
`web://@maya` opens her latest signed page as a "known contact". A
friends-only section prompts "share your app-scoped identity with Maya?"; on
approve, private posts load. A stranger instead gets "request access?", which
Maya approves later from her own profile app.

*Protocol.* `@maya` → petname book → `ed:5cb92a…` (keytag `dusk-otter-cedar` pinned) →
fetch signed manifest (seq / stale_after) → verify sig + keytag + bundle →
load. Private content is a **capability**: the app declares
`posts.privateList` on Maya's profile service; the host derives
`HKDF(user_master, maya_profile_authority)` and calls with that scoped key;
Maya's service checks it against her friends ACL and returns posts or
`access_required`; the access request is a signed `friends.requestAccess`
from the same scoped key. Maya recognizes Earl on *her* page; **no other app
can correlate that identity** — unlinkable by construction (§1). No "login
with X", no email/password, no tracking cookie, no global social graph — the
whole ambient-identity apparatus of the current web is simply absent.

## 10. Security model: hostile network, hostile names, hostile apps  *(Tier 0 — invariant)*

WebNext is **not automatically safer than DNS.** It is safer only if the
browser enforces one rule, and the whole design lives or dies on it:

> **Discovery can be messy. Execution cannot.**

Everything that helps you *find* — search, the `dns:` ramp, transparency
logs, DHT peers, namespaces, friends, relays — is **untrusted**. Only these
confer execution or privileged trust: a verified content hash, a verified
publisher signature, a pinned identity, an explicit user grant,
browser-owned security UI. If a soft name ever *silently* becomes authority,
WebNext is just DNS with extra cryptography. This section formalizes as
**mandatory** what §1b–§1e merely designed (bare-name softness, `@`-pinning,
discovery-is-not-trust).

### 10.1 Eight claims stated up front

1. Bare names are **not** authorities. 2. Search results are **not**
authorities. 3. Gateways are **not** authorities. 4. DHT peers are **not**
authorities. 5. Transparency logs are **not** authorities. 6. Apps receive
**no** ambient authority. 7. The **browser host is the security kernel**.
8. **Verified hashes and pinned keys are the only execution roots.**

### 10.2 Seven enforced rules

- **R1 — authority is cryptographic, not linguistic.** A string grants
  nothing; only a hash, key, or pinned binding does.
- **R2 — all network sources are hostile.** Gateway, peer, DHT, relay,
  search, resolver: the browser verifies everything, trusts none.
- **R3 — all apps are hostile until constrained.** Signed ≠ safe; popular ≠
  safe; known publisher ≠ safe. WASM sandbox + declared capabilities are
  non-negotiable.
- **R4 — updates are security events.** New code = new authority = new risk.
  Silent update is allowed *only* when permissions **and** publisher identity
  are unchanged; a new capability or key change requires fresh consent.
- **R5 — identity changes are never silent.** Key rotation, name rebinding,
  service-key change, new recovery key, new delegation parent — all get
  browser-native, user-visible handling.
- **R6 — privacy is tiered, not promised.** *Normal* (fast verified fetch,
  some metadata leak) / *Private* (relay/oblivious for manifests + rare
  bundles) / *Paranoid* (no public DHT, no auto-seed, gateway+relay only).
  No "total anonymity" claim (§5).
- **R7 — high-risk name categories get conservative handling** (banks,
  payments, government, health, cloud, package registries, identity
  providers): require a pinned identity or explicit attestation, never a
  bare-name resolve. *Caveat (my addition): the category list is itself a
  governance surface — it must be pluggable and auditable, never one
  vendor's hardcoded list, or it becomes the kingmaker §7 warns about.*

### 10.3 The identity certainty ladder (drives chrome treatment)

Distinct from §0b's maturity **Tiers** — these are **certainty levels
C0–C4** governing how much the chrome trusts an authority. They map onto §1's
absolute→social spine:

```
C0  b3:<hash>          exact immutable bytes      highest — no trust decision
C1  ed:<key>           exact publisher identity   high — verify signature
C2  name~keytag        label pinned to a key      good — key is the anchor
C2  a.b.c delegation   verified signature chain   good — C2-equiv per link;
                                                   top label follows its own
                                                   C0–C4 (a `~keytag` top =
                                                   C2, a bare top = C4)
C3  @petname           your own pinned binding    good after first pin
C4  bare name          ambiguous discovery        NOT TRUSTED — search-like
```

**Bare names (C4) must never get the same chrome as pinned identities.** Not
`Google`, but either `Google · slate-harbor-vale — verified by your saved trust set`, or a
disambiguation: `"google": multiple identities found — choose one` with the
newly-seen and look-alike claims flagged. Bare names are search queries, not
destinations.

**Reconciling the sales examples with the security model (read this before
§9).** Pretty dotted/bare addresses used to sell the ergonomics
(`google.deepmind.chat`, `nyc/hall`, Flow 2's `google.search`) render at
**full confidence only once their top label is pinned** via petname/history;
on genuine *first contact* they are C4 — a chooser/search, not a smooth "just
works" destination. Flow 2 looks seamless because it assumes `google` is
already pinned (the common steady-state); the honest first-run of any of them
is the disambiguation above. This is not a contradiction between the marketing
and the model — it's the same "discovery is social, execution is verified"
split (§0): the *first* visit is discovery (C4, you choose and pin), every
visit *after* is execution (C2, verified against your pin).

### 10.4 Adversary sweep — mitigation → honest verdict

| Adversary | Core mitigation | Honest verdict |
|---|---|---|
| Malicious gateway/CDN | every chunk hash-verified; root must match `b3:`; mismatch = hard fail, never substitution; race gateways; cache-first | **Strong** if verification is non-bypassable |
| DNS incumbents / registrars | `b3:`/`ed:` work with no DNS; legacy is explicit `dns:`, never native authority | Technically defensible, **politically hostile**; danger = ramp re-centralizes |
| Squatters / phishing clones | C0–C4 ladder; bare names non-authoritative; look-alike + confusable flagging; keytag pin trips impersonation | **Manageable** iff bare names never feel authoritative |
| Malicious transparency logs | quorum across independent logs; cross-signed heads; equivocation detection; replaceable log sets; **never required for `b3:`/`ed:` access** | **OK for discovery**, dangerous if made a root |
| Malicious search engines | results are pointers; identity metadata shown in-chrome; deep-link to `ed:`/`b3:`/`~keytag`; local history/petnames beat ranking | **Search finds, browser verifies, user grants** |
| Malicious publishers | no raw sockets/FS/global storage/ambient identity by default; manifest-declared, user-visible perms; §10.6 chrome | **Safe only if host owns authority** |
| Compromised publisher key | §10.5 TUF-style: offline root, threshold, rotation/revocation records, rollback protection, update delay, log inclusion | Single-key OK for personal, **too weak** for bank/OS/registry scale |
| Malicious services | `ed:`-verified service; hashed iface; declared caps; user approval; public data as signed `b3:` objects | Protocol verifies *who answered*, **not that the answer is fair** |
| DHT/swarm attackers | verify all chunks; never run unverified bytes; cap resources; no auto-seed of private/rare content; reputation = perf hint only; LAN/gateway fallback; oblivious for sensitive | **Good for integrity, hard for privacy/availability** under active attack |
| Governments / censors | identity independent of DNS; replaceable gateways + DHT bootstrap; LAN/USB/offline sharing; open specs; namespaces not required for access | **More resistant than DNS web, not censorship-proof** — "have the bytes + the hash → run the app" |
| Malware / hostile endpoint | OS keychain / TPM / Secure Enclave; encrypted profile; hardware-backed master key; perm audit log; profile lock | **Reduces damage, cannot beat full compromise** |
| UI spoofing | §10.6 — trusted chrome is sacred | **Mandatory; without it the system collapses** |

### 10.5 Publisher key management (escalation, reusing §1d delegation)

Single-key `ed:` identity is fine for prototypes and personal pages. It is
**not** enough for banks, OS vendors, package managers, or Google-scale apps.
For those, the publisher's own key becomes a small delegation tree (the §1d
machinery pointed inward):

```
ed:<publisher_root>   (offline, threshold-held)  delegates ->
    release key       (signs day-to-day updates)
    recovery key set  (M-of-N social/institutional, §7)
    service key       (signs live-data responses)
    namespace key     (governs sub-app allocation)
```

Plus: rollback protection (monotone `seq`), update delay for widely-installed
apps, revocation records, transparency-log inclusion of updates, and a
browser-native warning on any unusual key change (R5).

**Institutional custody tier (the "call GoDaddy" analog, done right).**
Regulated entities need a recovery/assurance apparatus that personal M-of-N
can't provide. The opt-in institutional tier: the publisher root is
**hardware-backed threshold-signed** (HSM/TPM, e.g. 3-of-5) and its recovery
key set names **bonded custodians** — escrow/legal entities that can
*assist recovery* (reconstitute a lost threshold share under documented
process) but **never gain authority over content**: they hold recovery
shares, not the release key, and cannot sign an update or seize a name. This
gives banks/health/gov the support-desk + legal-process + insurance model
they require, as an explicit higher-assurance opt-in, without reintroducing a
seizing authority (§1d) or a CA-tree root. Personal users keep the frictionless
M-of-N default; the tiers differ in custody rigor, not in who owns the bytes.

### 10.6 Trusted chrome is sacred (the browser-killer class)

Apps cannot draw permission prompts, identity badges, address bars, or "verified"
claims — **those surfaces are browser-native only.** Apps cannot draw outside
their viewport; fullscreen keeps a persistent identity overlay; clipboard
writes and sensitive approvals require a trusted-UI user gesture; key-change
warnings are browser-native. This is non-negotiable: without trusted chrome,
every other guarantee here is spoofable and the system collapses.

### 10.7 Keys must become human (or DNS wins anyway)

DNS won because names were human. WebNext survives only if **keys become
emotionally legible** — the browser needs a UX language for cryptographic
trust that ordinary people read at a glance. Not `ed25519:5cb92a99…`, but:

```
Maya · dusk-otter-cedar                         Google Search · slate-harbor-vale
known contact since May 2026        pinned publisher · update verified
signed profile · no new permissions calls: Google Search Service
```

This is a hard requirement, not polish: it is the single most likely place
the whole project loses to the incumbent it is trying to replace.

## 11. Quality of life: living with a hostile-by-default runtime  *(Tier 0 host + Tier 2 polish)*

§10 says *trust nothing*; §11 answers *how a user lives with that without
drowning in scary prompts.* The bridge from "protocol runtime" to "browser
people use daily" is three things: a browser-level permission vocabulary over
Wasmtime's sandbox, update UX that is signed + diffed + rollbackable, and
identity-aware chrome. This is a core pillar, not a finishing pass.

### 11.1 Wasmtime is the sandbox; WASIBrowser is the policy layer

Wasmtime supplies *primitives*, not a UX: memory isolation, the WASI preopen
model (no ambient FS — access is explicit preopened handles), `Store::limiter`
(cap memories/tables/instances), **fuel** (deterministic, metered trap/yield)
and **epoch interruption** (cheap wall-clock interruption of untrusted code),
and a compile cache. App authors must never think in WASI powers (`preopen
dir`, `inherit network/env/stdio`); they think in **browser capabilities**,
and the host maps each to a specific import + WASI config + runtime gate.

> Wasmtime enforces the import boundary. WASIBrowser defines what the imports
> *mean* and who may call them.

### 11.2 The capability vocabulary (manifest-declared, browser-granted)

**Storage** (app-private · user-picked file r/w · folder r/w · signed export)
· **Network** (none · RPC-to-declared-service · declared-gateway-fetch ·
LAN-peer · public-swarm · raw-socket ≈ never) · **Identity** (anonymous ·
app-scoped · contact-share · org-share) · **Payments** (declared
payment-service · charge · subscribe, §4a) · **System** (notifications ·
clipboard r/w · camera+mic · background · device discovery) · **Execution
budgets** (memory · fuel/CPU · background-minutes · storage quota · network
quota). No raw WASI term ever reaches the user surface.

### 11.3 The gate — AppPolicy + per-import checks (the heart)

Each instance carries `AppPolicy { authority, bundle_hash, publisher_key,
granted_permissions, resource_limits, service_grants, storage_namespace }`;
every host import consults it:

```
dom.create_element     default-allow
storage.open           require app-private-storage grant
fs.open_user_file      require user-picked-file grant (+ picker gesture)
rpc.call               require {service key, iface, method} grant
clipboard.read         require clipboard grant + user gesture
notifications.show     require notification grant
swarm.seed             require public-seeding grant
identity.sign          require scoped-identity grant
```

Wasmtime handles memory isolation + the import boundary; this layer handles
*meaning*. It's the security kernel of §10 R3 made concrete.

### 11.4 App health = resource limits made visible (a Task Manager)

`Store::limiter` + fuel + epoch aren't only safety — they're the data behind
a browser **Task Manager**: per-app CPU / memory / network / storage + a
health state. *Fuel* is deterministic (metered, reproducible, some overhead);
*epoch interruption* is the cheap choice for reining in a main-loop hog — use
epoch for "this app is spinning," fuel where determinism matters. Health
states unify runtime **and** lifecycle (§2b): running · sleeping ·
background-limited · network-blocked · permission-needed · throttled ·
out-of-fuel · out-of-memory · crashed · update-available · stale · sunset.
Row actions: pause · restart · revoke network · clear storage · pin version ·
rollback · view permissions/publisher. **§2b's `retain`/`stale_after`/
`sunset`/`BUNDLE_OUTDATED` stop being hidden protocol fields and become
visible states.**

### 11.5 Prompts: rare, staged, specific

Launch with minimum safe capabilities; request only on the action that needs
it (click "enable reminders" → the notification prompt, never a launch-time
wall of asks). Every prompt is **specific**, never "do you trust this app?":
*"…call AI Reflection Service · moss-lantern-quill (publisher blue-otter-cedar) to generate
reflections? [This time] [Always] [Deny]."* Every prompt carries a **why?**
(plain-language purpose + the technical grant/limit) to avoid Android-style
fatigue. Manifest marks **required vs optional**; the user can **run with
reduced permissions** ("allow required only"). *Refinement: the `required`
flag is publisher-declared and therefore over-askable — the browser still
runs-and-observes, and an app that declares a power "required" yet functions
without it is a negative quality signal. The flag is a hint; reputation is the
check.* The governing rule: **specific trust is manageable, generic trust is
how users get owned.**

### 11.6 Updates: signed, 4-axis diffed, rollbackable

Every update is judged on four axes — **code** (bundle hash changed?),
**publisher** (same trusted key/chain?), **capability** (permissions
changed?), **data** (migration safe/reversible?) — feeding three policies
(this is §10 R4 made operable):

- **Auto-safe** — same publisher, same permissions, compatible ABI,
  reversible/compatible migration, unflagged → silent update.
- **Ask** — new permission/service/identity-sharing, major-version jump, key
  rotation, or irreversible migration → paused with a *specific* diff
  ("current: storage + notifications; new adds: background + RPC to
  Analytics · rust-copper-fern — [Update & allow] [Update, deny new] [Stay] [Notes]").
- **Block** — invalid signature, rollback attack, revoked key, hash mismatch,
  hidden escalation, known-malicious bundle.

**Channels** (stable default / beta / nightly / pinned / enterprise /
friend-shared / archive) ride the signed publisher model
(`ed:` → per-channel seq → `b3:`). **Rollback** is native because bundles are
content-addressed and `retain: N` keeps recent good versions — a crash-looping
update auto-restores the previous good version and pauses. **Storage
migrations** declare reversibility + backup; irreversible ones force an
export-first prompt. The manifest carries `"permissions_may_expand": false` —
a publisher promise that posture holds; *flipping it to `true` is itself an
Ask-tier event, never silent.*

### 11.7 The identity chip = the new lock icon

A per-app trust chip in chrome (`SemanticPortrait · blue-otter-cedar`) expands to
identity (publisher, loaded-from `ed:`, bundle `b3:`, first-opened),
permissions (granted + denied + RPC grants), and updates (channel, current,
rollback-available). The TLS lock meant "encrypted to this domain"; the
WebNext chip means **"these exact bytes / this exact publisher key are
running, with these exact powers"** — strictly more useful. Trusted-chrome
only (§10.6): the chip, the prompts, and key-change warnings are
browser-native, never app-drawable.

### 11.8 Per-app privacy profiles + safe mode

Per app: **identity mode** (anonymous / app-scoped / contact / org),
**fetch mode** (fast / private-relay / no-public-swarm — the §5 tiers), and
**storage** (keep / clear-on-close / encrypted-only). Maya's page runs
Maya-scoped + private-relay + keep; a random game runs anonymous + fast +
clear-on-close — far more precise than one global incognito. **Safe mode**
(per app): deny optional perms, disable background, network = required-RPC
only, load the previous known-good storage snapshot, no experimental ABI
imports — for debugging, malware suspicion, and bad updates.

### 11.9 Install = open

A verified bundle with a local cache is already an installed app; there is no
website-vs-app choice to make. First open shows publisher + size + "runs
offline after first load" + permissions and an [Open]; afterward, optional
[Pin] / [Add shortcut]. Progressive, never a gate.

### The pillar, in four lines

```
Safe        because apps are sandboxed (Wasmtime + capabilities, §10).
Usable      because permissions are human-level, rare, staged, specific.
Resilient   because updates are signed, cached, rollbackable, and diffed.
Transparent because identity, powers, resources, and history are visible.
```

## 12. Prior art — and why this isn't just $EXISTING_THING

Nearly every primitive here has a precedent, and a technical audience will
ask "how is this not X?" on sight. Honest lineage + the actual differentiator:

| System | Closest overlap | What next.0 adds that it lacks |
|---|---|---|
| **Web Bundles / Signed HTTP Exchanges (SXG)** | Google's own signed, content-addressed "bundle" format — same core bet, same word | next.0 adds the runtime, RPC, capability, and security model that initiative never had — and doesn't tie the signature to the origin/CA the way SXG does; Google prototyped the bundle and abandoned the platform |
| **Beaker Browser / `dat://`** | the *only* browser that shipped an end-user P2P content-addressed URL scheme (closest shipped UX) | capability sandboxing, RPC services, the whole security/permission/update model, and address ergonomics Beaker lacked; Beaker proved the UX is possible and stalled on trust/safety — exactly this plan's focus |
| **Nostr** | pubkey-as-identity + untrusted relays for discovery — philosophically identical to §1e's discovery-is-untrusted / execution-is-verified split | an execution runtime + capability model + naming ergonomics; Nostr is a social/messaging protocol, not an app platform |
| **Tor onion services** | self-certifying, key-derived addresses running at real scale for 15+ years | the 15-year proof that "verifiable key *is* the address" works; next.0 adds human ergonomics (word-keytags/petnames over Tor's unspeakable base32), an app runtime, and payments — onion validates the core bet, this makes it usable |
| **IPFS / IPNS** | content-addressed chunks + mutable name pointers | an *application runtime* — capability sandbox, RPC-first services, no-JS DOM ABI, a security/permission/update model; IPFS distributes bytes, it doesn't run or govern apps |
| **Dat / Hypercore** | signed append-only logs over content-addressed chunks (the closest analog) | the app + service + capability + payments + browser-chrome stack on top; Hypercore is the distribution substrate, not the platform |
| **Secure Scuttlebutt** | signed identities, offline-first, no global namespace | web-scale distribution (swarm/gateways/fountain), RPC services, a runtime; SSB is a social-feed protocol, not an app platform |
| **Urbit** | personal signed identity + own runtime | wasm/any-language + browser integration + no bespoke VM/address-space or land-sale economics; Urbit reinvents the whole stack and sells identity |
| **Solid** | user-owned data, decoupled from apps | binary DOM ABI + capability sandbox + no-JS + content addressing; Solid is data-pods over the *existing* web stack (HTTP/JS/OAuth), which is the stack next.0 replaces |
| **DID / Verifiable Credentials** | decentralized keys as identity | a concrete runtime, naming ergonomics, and distribution; DIDs are an identity spec, not a web |
| **BitTorrent** | swarm distribution, fountain-adjacent | verification-as-trust, mutable identities, RPC, an app model; BT moves files, full stop |

**The one-line differentiator:** the prior art is almost all *distribution or
naming or identity* layers. next.0 is an **application runtime with a
security, capability, payment, and UX model** that happens to use
content-addressed distribution — the layer all of the above deliberately left
to "someone else's app." The bet is that packaging them into one coherent
runtime with *excellent ergonomics* is the thing none of them shipped, and
the thing that decides adoption.
