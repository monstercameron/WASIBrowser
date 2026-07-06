# 02-WEB-NAMING.md — social name allocation & resolver distribution

### Status: 🟡 FIRST DRAFT. This authors the **social-resolution + resolver-node
### distribution** half of naming (the transparency-log substrate §1b/§1c only
### gestured at). The rest of the naming model — the four address forms, the
### native-first resolution algorithm, keytag word-encoding, delegation, and the
### DNS-legacy ramp — is designed in `../plan-webnext.md` §1/§1a–§1d and is P0
### extraction into this same doc. Enforces Constitution rules 1, 2, 4, 7, 8.

## How this reconciles with plan-webnext.md §1 (read this first)

This doc does **not** introduce a new naming model — it specifies the *mechanism*
behind the one already in §1. The mapping is exact:

| plan-webnext.md §1 term | This doc's mechanism |
|---|---|
| "federated quorum of mutually-auditing logs" (§1a/§1b) | **Claim Log Nodes** (§4.x here) — CT-style append-only Merkle logs |
| "native-first resolution" (§1a rule, §1c ranking) | The **ResolverPolicy** ranking (§2.6) — `local_petname` + `user_history` outrank every network signal, so pins/history always win first |
| `name~keytag` durable address (§1a/§1c) | **QuorumClaim + KeytagAssignment** (§2.3/§2.4) |
| word-encoded keytag, ~33-bit floor, lengthens with density (§1a rule 3) | **KeytagAssignment.tag_bits**, first-quorum-keeps-short (§2.4) |
| bare name = soft lookup, chooser, never silent destination (§1a rule 5, §10.3) | **Bare social resolution** (§5.3) + label classes (§6) |
| C0–C5 certainty ladder + identity chip (§10.3, §18) | **`confidence` / `basis` → the C-ladder, rendered** (see next section) |
| parent-key signs child delegation (§1d) | **Delegation after social resolution** (§10) |
| "no exclusivity, no transfer, no passive yield" anti-park legs (§1c) | **Anti-squat economics** (§8) — plus the fourth leg made explicit |

**One vocabulary, not two.** The `confidence`/`basis` fields below are not a
parallel trust scheme — they are the inputs the browser renders as the **C0–C5
certainty ladder** (`01-WEB-SECURITY.md`, plan §10.3/§18). The mapping the
chrome MUST use:

```txt id="cladder-map"
ed:<key> / b3:<hash> exact              -> C0  [Exact bytes]
name~keytag, quorum-confirmed + attested -> C1  [Verified publisher]
name~keytag, quorum-confirmed, pinned    -> C2  [Pinned identity]
@petname (local binding)                 -> C3  [Your contact]
bare <label>, resolved-but-unpinned      -> C4  [Unverified name] — chooser, never a destination
legacy DNS (dns: ramp)                   -> C5  [Legacy web] — quarantined
```

Dangerous capabilities (payments/WVEP V3+, identity-sharing, private profiles)
require **C1/C2 or better** — never C4/C5. A resolver's `confidence: "high"` is
advisory input to that ladder decision, never a substitute for it.

---

## 0. Core rule

```txt id="r01n1s"
Top-level names are allocated as social aliases, not property.

A key may claim a label.
A log quorum may confirm that claim.
A resolver policy may rank that claim.
A user may pin that claim.
But no actor owns the bare word absolutely.
```

So:

```txt id="yqrkbs"
cam~blue-otter-cedar = durable, portable, key-anchored alias
cam                  = soft/social lookup
@cam                 = local petname
ed:<key>             = cryptographic identity
```

---

## 1. Actors

```txt id="nk5fts"
Claimant
  Entity holding an ed25519 key that wants a social label.

Claim Log Node
  Append-only Merkle log accepting signed label claims.

Resolver Node
  Network service that aggregates logs, attestations, policies, and rankings.

Attestor
  Entity that signs evidence about a key: DNS control, legal entity, trademark, civic role, profession, community membership.

Policy Publisher
  Publishes resolver policy: ranking weights, high-risk labels, default namespace sets.

Browser
  Final resolver. Verifies proofs, applies local policy, shows chooser/chrome, and lets user pin.

User
  Final authority for local aliases through @petnames.
```

---

## 2. Object model

> **Canonicalization & schemas.** Every object below is canonical CBOR;
> `object_id = b3(canonical_cbor(object))`. The JSON here is illustrative. The
> normative CDDL for `TopLabelClaim`, `ClaimInclusionProof`, `QuorumClaim`,
> `KeytagAssignment`, `NameAttestation`, `ResolverPolicy`, `AliasBinding`,
> `DisputeRecord`, `ClaimRenewal`, `ClaimSuccession`, `ResolverHead` lives in
> `SCHEMAS/` and is a P0 deliverable (nothing is real until its INVALID
> test-vector exists — `TEST-VECTORS/`).

### 2.1 TopLabelClaim

A claim says:

```txt id="6gljbp"
this key wants this label
```

It does **not** say:

```txt id="c7plgj"
this key owns this label
```

```json id="17ztl0"
{
  "type": "TopLabelClaim",
  "v": 1,
  "label": "cam",
  "label_skeleton": "cam",
  "owner": "ed:<pubkey>",
  "created": 1783370000,
  "expires": 1814906000,
  "nonce": "b3:<random>",
  "pow_or_vdf": "b3:<claim_cost_proof>",
  "sig": "sig_by_owner"
}
```

Rules:

```txt id="8pxo81"
- labels are canonicalized before claim
- confusables collapse to label_skeleton
- claims are non-exclusive
- claims expire
- claims are non-transferable
- owner key cannot be changed; succession is separate
```

### 2.2 ClaimInclusionProof

A log node returns:

```json id="na21dc"
{
  "type": "ClaimInclusionProof",
  "claim_hash": "b3:<claim>",
  "log": "ed:<log_node_key>",
  "log_epoch": 9182,
  "merkle_root": "b3:<root>",
  "merkle_path": ["b3:...", "b3:..."],
  "timestamp": 1783370200,
  "sig": "sig_by_log"
}
```

### 2.3 QuorumClaim

A claim becomes confirmed when enough independent logs include it.

```json id="jys6sb"
{
  "type": "QuorumClaim",
  "claim_hash": "b3:<claim>",
  "label": "cam",
  "owner": "ed:<pubkey>",
  "confirmed_at": 1783370400,
  "proofs": [
    "ClaimInclusionProof from log A",
    "ClaimInclusionProof from log B",
    "ClaimInclusionProof from log C"
  ],
  "quorum_policy": "ed:<policy_key>",
  "status": "confirmed"
}
```

Suggested default:

```txt id="urx22a"
quorum = 3 of 5 logs
```

or:

```txt id="bxu70t"
quorum = 5 of 9 logs
```

for high-risk labels.

### 2.4 KeytagAssignment

A confirmed claim gets a collision-safe word keytag (BIP39-style dictionary,
plan §1a — `blue-otter-cedar`, not hex).

```json id="4erzq6"
{
  "type": "KeytagAssignment",
  "label": "cam",
  "owner": "ed:<pubkey>",
  "keytag": "blue-otter-cedar",
  "tag_bits": 33,
  "collision_set_epoch": 9182,
  "sig": "sig_by_quorum_or_policy"
}
```

Rule:

```txt id="8x5hyc"
The first quorum-confirmed claim for label~keytag keeps that short form.
Later collisions must extend the keytag.
```

So:

```txt id="ggx0gy"
cam~blue-otter-cedar
```

is stable. (This is plan §1a rule 3 — the keytag lengthens with claim density,
hard floor ~33 bits; the loser of a collision is deterministically auto-bumped
to a longer tag at quorum.)

### 2.5 Attestation

Attestors add evidence.

```json id="g76g1x"
{
  "type": "NameAttestation",
  "subject": "ed:<pubkey>",
  "label": "google",
  "class": "legal_entity",
  "display_name": "Google LLC",
  "evidence": [
    "dns:google.com/.well-known/webnext.json",
    "trademark:GOOGLE",
    "registry:US-DE:Google LLC"
  ],
  "attestor": "ed:<attestor_key>",
  "expires": 1814906000,
  "sig": "sig_by_attestor"
}
```

Attestation classes:

```txt id="vqnjyz"
dns_control
legal_entity
trademark
government_entity
geographic_entity
professional_license
community
open_source_project
personal_web_of_trust
```

### 2.6 ResolverPolicy

A policy says how bare names are ranked. **This is the native-first rule
(plan §1a) made mechanical:** `local_petname` and `user_history` dominate every
network-sourced signal, so a user's own pins/history always resolve first.

```json id="u04kcl"
{
  "type": "ResolverPolicy",
  "v": 1,
  "name": "WASIBrowser Default Social Resolver",
  "publisher": "ed:<policy_key>",

  "quorum": {
    "ordinary": "3-of-5",
    "high_risk": "5-of-9"
  },

  "ranking": {
    "local_petname": 1000000,
    "user_history": 250000,
    "dns_control": 50000,
    "legal_entity": 40000,
    "trademark": 30000,
    "government_entity": 30000,
    "contact_import_frequency": 10000,
    "log_age": 5000,
    "clean_revocation_history": 3000,
    "earliest_claim": 1000,
    "lookalike_penalty": -100000,
    "new_key_penalty": -25000,
    "dispute_penalty": -50000
  },

  "high_risk_behavior": "chooser_or_pinned_only",
  "sig": "sig_by_policy"
}
```

Key idea:

```txt id="pjxo5h"
ResolverPolicy owns presentation, not names.
```

The ranking weights map onto plan §1c's `default_rank(claim)` — this is its
concrete, signed, replaceable form. Because `local_petname`/`user_history` sit
at the top, "native-first" is not special-cased code; it falls out of the
ranking. **The policy is itself a replaceable `ed:`-signed object** — a user or
community swaps it wholesale (§9.2).

### 2.7 AliasBinding

The browser or resolver node outputs a proposed bare-name binding. `confidence`
is advisory input to the C-ladder decision (see the reconciliation table),
never authority by itself.

```json id="ij7ody"
{
  "type": "AliasBinding",
  "label": "google",
  "selected": "ed:<google_key>",
  "display": "Google LLC",
  "keytag": "slate-harbor-vale",
  "confidence": "high",
  "basis": [
    "dns_control",
    "legal_entity",
    "trademark",
    "long_log_age"
  ],
  "policy": "ed:<resolver_policy_key>",
  "expires": 1785962000,
  "sig": "optional_sig_by_resolver_node"
}
```

Browser must verify the evidence itself or trust the resolver only as a cache.

---

## 3. Registration protocol

### 3.1 Claim creation

```txt id="klkmzp"
claim = canonicalize(label)
      + owner key
      + expiry
      + nonce
      + claim-cost proof
      + owner signature
```

Client submits to several Claim Log Nodes.

```txt id="8fil5q"
POST /claims
```

### 3.2 Log acceptance

A log accepts if:

```txt id="fiy4v1"
- signature valid
- label grammar valid
- label_skeleton not forbidden
- expiry within max window
- claim-cost proof valid
- rate limits satisfied
- owner not locally banned for abuse
```

A log does **not** reject because:

```txt id="dkc1a7"
another key already claimed the same label
```

Claims are non-exclusive.

### 3.3 Quorum confirmation

Client waits for inclusion proofs from quorum logs.

```txt id="qmdsyz"
ordinary: 3-of-5
high-risk: 5-of-9
```

When quorum exists:

```txt id="t10c46"
claim becomes confirmed
keytag becomes assignable
name~keytag becomes shareable
```

Chrome shows a tag as "confirmed" **only post-quorum** (plan §1a) — the brief
pre-quorum window is ambiguous and rendered as such, never as a destination.

### 3.4 Renewal

Claims expire, e.g. yearly. Renewal is free:

```json id="cucaml"
{
  "type": "ClaimRenewal",
  "claim_hash": "b3:<claim>",
  "owner": "ed:<same_pubkey>",
  "new_expires": 1846442000,
  "sig": "sig_by_owner"
}
```

Rules:

```txt id="btveef"
- renewal must use same owner key
- no transfer
- missed renewal enters grace period
- after grace, claim is inactive
- inactive claims stop ranking but historical record remains
```

### 3.5 Succession, not transfer

No sale/transfer. Allowed:

```txt id="9lg9xx"
owner key signs successor key
```

```json id="12q2rh"
{
  "type": "ClaimSuccession",
  "old_owner": "ed:<old>",
  "new_owner": "ed:<new>",
  "claim_hash": "b3:<claim>",
  "reason": "rotation|recovery|institutional_threshold",
  "sig_old": "sig_by_old_or_recovery_threshold"
}
```

Browser shows (this is a U-ladder key-rotation event — `06-WEB-UPDATES.md`,
plan §11.6 — and pauses for review, never auto-accepted):

```txt id="nas2b4"
cam · blue-otter-cedar rotated keys.
Reason: recovery.
Review before trusting.
```

---

## 4. Resolver node network

Claim logs are for **truth of inclusion**. Resolver nodes are for **fast lookup
and social presentation**.

### 4.1 Resolver node responsibilities

A resolver node aggregates:

```txt id="xol0fr"
- claim logs
- quorum proofs
- keytag assignments
- attestations
- disputes
- policy bundles
- revocations
- namespace manifests
```

It serves:

```txt id="8ds7jq"
resolve(label)
resolve(label~keytag)
claims(label)
attestations(key)
explain(label, selected_key)
```

### 4.2 Resolver node is not trusted

A resolver node may lie by omission or ranking. Browser defense:

```txt id="vrru9v"
- verify all signatures
- verify quorum proofs
- query multiple resolver nodes
- cache previous results
- prefer local pins/history
- fall back to explicit chooser
```

Resolver nodes are caches/indexes, not authorities. This is the same rule the
plan uses for search and gateways (§10, Constitution rule "discovery finds,
verification decides"): **discovery helps find; verification decides trust.**

### 4.3 Resolver node federation

Resolver nodes gossip signed objects:

```txt id="qa9x3g"
Claim
InclusionProof
QuorumClaim
KeytagAssignment
Attestation
DisputeRecord
ResolverPolicy
NamespaceManifest
Revocation
```

They do not gossip mutable "current truth." They gossip signed facts.

### 4.4 Resolver sync

Each resolver maintains an append-only object store:

```txt id="1ac0si"
object_id = b3(canonical_cbor(object))
```

Sync protocol:

```txt id="xmdag0"
HAVE epoch/root
WANT object range
SEND signed objects
VERIFY signatures
INDEX locally
```

Resolver nodes may index differently but the objects are the same.

### 4.5 Resolver quorum query

For high-risk bare names, browser queries multiple resolver nodes:

```txt id="u0vbsb"
query 5 resolver nodes
require 3 consistent candidate sets
if inconsistent -> chooser + warning
```

For `name~keytag`, resolver disagreement is less dangerous because the keytag
anchors the key.

---

## 5. Resolution modes

### 5.1 Exact keytag resolution

```txt id="u2scfm"
web://cam~blue-otter-cedar
```

Steps:

```txt id="pkc7li"
1. Query resolver nodes for label=cam, keytag=blue-otter-cedar.
2. Fetch QuorumClaim + KeytagAssignment.
3. Verify proofs.
4. Return owner ed key.
```

No ranking needed. (C1/C2.)

### 5.2 Local petname resolution

```txt id="a76kct"
web://@cam
```

Steps:

```txt id="ozur9y"
1. Lookup local petname book.
2. Return pinned ed key.
3. Never query network unless user asks to refresh metadata.
```

(C3 — the strongest binding *for that user*.)

### 5.3 Bare social resolution

```txt id="6cl0q1"
web://cam
```

Steps:

```txt id="hwxv20"
1. Check local petname/history.         (native-first: these win outright)
2. Query resolver nodes for candidates.
3. Verify quorum proofs.
4. Load attestations and disputes.
5. Apply ResolverPolicy.
6. If low-risk and one clear winner: present selected identity.
7. If high-risk, ambiguous, new, or disputed: show chooser.
```

Bare names are never execution authority by themselves. An unpinned bare name
is **C4 — a chooser, never a silent destination** (plan §10.3).

---

## 6. Label classes

Each label has a class under policy.

```txt id="ei9yvh"
ordinary
personal
commercial_entity
public_institution
geography
regulated_sector
profession
generic_category
legacy_collision
dangerous_word
```

Examples:

```txt id="w7dq2o"
cam       personal / ordinary
google    commercial_entity
irs       public_institution
nyc       geography
bank      regulated_sector
md        profession + geography conflict
ai        generic_category + legacy_collision
login     dangerous_word
```

Resolution behavior:

```txt id="6un8w5"
ordinary:
  policy may auto-select if confidence high

commercial_entity:
  require strong attestation to auto-select

public_institution:
  require civic/government attestation

regulated_sector:
  chooser or pinned only unless licensed

generic_category:
  show namespace directory, not single owner

dangerous_word:
  never auto-select
```

---

## 7. Disputes

Disputes affect presentation, not existence.

```json id="h95d5v"
{
  "type": "DisputeRecord",
  "label": "google",
  "subject": "ed:<key>",
  "class": "trademark_dispute",
  "status": "active",
  "issuer": "ed:<attestor_or_policy_key>",
  "evidence": ["b3:<evidence>"],
  "expires": 1785962000,
  "sig": "issuer_sig"
}
```

Effects:

```txt id="r8c3dg"
- lower ranking
- show warning
- require chooser
- remove attestation
```

Never effects:

```txt id="98hnnc"
- delete ed key
- delete b3 bundle
- delete name~keytag
- prevent direct access
```

This is the core censorship-resistance property (plan §6, `01-WEB-SECURITY.md`):
social layers can *demote*, never *erase*. `ed:`/`b3:` always bypass (§12.7).

---

## 8. Anti-squat economics

The design kills parking by making claims:

```txt id="nvte4g"
free
non-exclusive
non-transferable
renewable
expiring
key-bound
not ad-yielding
not required for app existence
```

Plan §1c identifies the three legs of parking — exclusivity, transferability,
and passive yield — and removes them. This adds the fourth, explicit:

```txt id="7db11l"
first-claim only affects claim history/keytag priority, not bare-name monopoly
```

So a squatter can grab:

```txt id="y4l33w"
bank~blue-otter-cedar
```

but cannot make:

```txt id="ypz2xg"
web://bank
```

silently resolve to them (`bank` is a `regulated_sector` label — chooser or
pinned only, §6).

---

## 9. Distribution of social resolver nodes

### 9.1 Node classes

```txt id="j9b8ms"
Rootless Claim Logs
  Append-only inclusion logs.

Resolver Mirrors
  Cache and index signed name objects.

Policy Nodes
  Publish ResolverPolicy bundles.

Attestor Nodes
  Publish attestations.

Community Resolver Nodes
  Curate policy + namespace defaults for a community.

Browser Local Resolver
  Final local policy engine.
```

No node class is globally authoritative.

### 9.2 Bootstrap

Browser ships with:

```txt id="gl1nqv"
- small default log set
- small default resolver set
- default policy publisher
- high-risk label policy
```

But all are replaceable. High-trust users/communities can use:

```txt id="vlqayh"
company resolver
university resolver
privacy-distro resolver
country/community resolver
personal resolver
```

### 9.3 Resolver discovery

Resolvers are found through:

```txt id="u38auq"
hardcoded defaults
signed resolver list bundle
DHT discovery
LAN discovery
manual user config
organization policy
```

Resolver list is itself:

```txt id="6usjb8"
b3:/ed: signed object
```

### 9.4 Resolver consistency

Resolver nodes publish periodic heads:

```json id="8jaw36"
{
  "type": "ResolverHead",
  "resolver": "ed:<resolver_key>",
  "epoch": 9182,
  "claims_root": "b3:<root>",
  "attestations_root": "b3:<root>",
  "disputes_root": "b3:<root>",
  "policy": "ed:<policy_key>",
  "sig": "resolver_sig"
}
```

Browsers can compare heads across resolvers. If inconsistent:

```txt id="0nn7tx"
show chooser / stale warning
```

---

## 10. Delegation after social resolution

Once a top label resolves to an `ed:` key, dot delegation is cryptographic
(plan §1d — parent key signs child).

```txt id="ob3dyo"
web://google.deepmind.chat
```

Steps:

```txt id="9tjyow"
1. Resolve bare google socially -> ed:<google_key>.
2. Fetch google manifest.
3. Verify signed delegation: deepmind -> ed:<deepmind_key>.
4. Fetch deepmind manifest.
5. Verify signed delegation: chat -> ed:<chat_key>.
6. Load final app/service.
```

Rule:

```txt id="t150p1"
Top-level aliasing is policy-owned.
Child delegation is parent-key-owned.
```

Only the *leftmost* label uses social resolution; every dot after it is a
key-signed delegation, so `google.deepmind.chat` cannot be hijacked by claiming
`deepmind` or `chat` at the top level (plan §1a "native-first, not final-label").

---

## 11. Required browser UI

Renders the C-ladder (`10-WEB-UX.md`, plan §10.3). Raw `ed:`/`b3:` strings are
never shown as the identity — always the label + keytag + evidence.

### 11.1 First contact (C4→save)

```txt id="c3tgd6"
cam · blue-otter-cedar
Unknown to you.
Claim confirmed by 5 logs.
No attestations.
[Open once] [Save as @cam] [Cancel]
```

### 11.2 Strong attestation (C1)

```txt id="3xv689"
Google LLC · slate-harbor-vale
Attested by DNS google.com + legal entity + trademark.
[Open] [Pin as @google]
```

### 11.3 Ambiguous bare name (C4, high-risk)

```txt id="xg0d90"
web://bank

High-risk name.
No single owner.

Choose:
- Bank of America · ...
- Chase · ...
- Local Credit Union · ...
- Search instead
```

### 11.4 Resolver explanation ("why this?")

Every resolution can be expanded to its ranking basis — the same signals the
first-contact ceremony surfaces (plan §21.4), so the result is auditable:

```txt id="gbkqey"
Resolved by:
- local history
- DNS attestation
- legal entity attestation
- 5-year claim age
- default resolver policy v12
```

---

## 12. Security invariants

```txt id="byblfh"
1. Logs prove inclusion, not legitimacy.
2. Resolvers rank candidates, not ownership.
3. Attestors sign evidence, not protocol truth.
4. Bare names never grant dangerous authority.
5. name~keytag is the portable human-safe form.
6. @petname is local and strongest for the user.
7. ed:/b3: always bypass social naming.
8. Child delegation requires parent key signature.
9. Disputes affect presentation only.
10. Resolver nodes are replaceable caches.
```

These extend the `01-WEB-SECURITY.md` adversary matrix (bare-name squatter,
search-ranks-a-clone rows) with the naming-specific defenses.

---

## 13. Minimal APIs

### Claim log

```txt id="1rg98m"
POST /claim
GET  /claim/{hash}
GET  /label/{label}/claims
GET  /head
GET  /proof/{claim_hash}
```

### Resolver node

```txt id="thf5aw"
GET /resolve/{label}
GET /resolve/{label}/{keytag}
GET /claims/{label}
GET /attestations/{ed_key}
GET /explain/{label}/{ed_key}
GET /policy
GET /head
```

### Browser host API (host-mediated, plan §11 capability model)

```txt id="y1xyfk"
host.name.resolve(authority) -> ResolutionResult
host.name.pin(label, ed_key) -> PetnameRecord
host.name.explain(label, ed_key) -> Explanation
```

Naming resolution is a **host capability**, not something an app performs — an
app never sees raw keys or draws the chooser (Constitution rule 4; trusted
chrome, plan §10.6).

---

## 14. Strong short summary

```txt id="dol63y"
Social names are distributed like Certificate Transparency, not DNS.

Claim logs record who claimed what.
Resolver nodes aggregate and rank claims.
Attestors provide evidence.
Policies decide default presentation.
Users pin what they trust.
Keys delegate child segments.

No one owns the bare word.
The stable address is name~keytag.
The trusted local address is @petname.
The absolute identities are ed: and b3:.
```

This gives a distributed, scalable social name system without recreating ICANN,
ENS, or domain parking.

---

## P0 acceptance (what makes this doc real — §19 contract)

```txt id="p0-naming"
- CDDL for all 11 objects in SCHEMAS/, each with a valid + INVALID vector
- reference Claim Log Node + Resolver Node (the 2 API surfaces above)
- the native-first ranking demonstrably beats a squatter for `bank`/`google`
- the P0 identity-UX study (plan §21.5): impersonation attack-success ≤ 5%,
  ≥90% treat a bare name as a search — THIS gate governs the whole naming bet
```
