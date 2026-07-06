# OPERATIONAL-LAYER.md — what turns a protocol into a durable platform

### Status: 🟡 SCAFFOLD (P1–P3). The protocol *core* (naming, bundles, RPC,
### security, permissions, updates, swarm, value, data) is designed and
### adversarially reviewed. THESE are the operational / legal / ecosystem layers
### that a real deployment needs — and they are **mostly undesigned**, not just
### unwritten. This doc is the honest holding pen: each dimension is registered
### with a scaffold and a status, and **graduates to its own numbered
### `docs/NN-WEB-*.md` when it has real normative content** — not before. Listing
### a dimension here is NOT a claim that it is solved.

## The principle threaded through all of them (the one invariant)

> **The protocol preserves addressability. Policy controls presentation,
> warning, and execution permission.**

Every operational layer below is *policy* — governance, moderation, compliance,
abuse response, enterprise controls. None of them may erase `ed:`, `b3:`, or
`name~keytag` access at the protocol layer. They may demote, warn, gate
capabilities, or refuse to *present* — never delete the address. This is the
same rule as naming disputes (`02-WEB-NAMING.md` §7) and censorship resistance
(plan §6), applied to the whole operational surface. It is what keeps "a
platform that can say no" from becoming "a platform that can disappear you."

## Priority order (author these first)

```
1. → 13-WEB-GOVERNANCE.md      who updates defaults/policies/logs and how they fail
2.   12-WEB-DATA.md            ALREADY a core slot — D-ladder (plan §2b/§18); not re-done here
3. → 15-WEB-ABUSE.md          detect→classify→warn→demote→block→dispute→appeal
4. → 25-WEB-CONFORMANCE.md    the test-vector + compatible-implementation definition
5.   11-WEB-PACKAGES.md        ALREADY a core slot — supply-chain lives there; scope expanded
6. → 19-WEB-UX-VALIDATION.md  formalizes the P0 identity-UX study (plan §21.5)
7. → 20-WEB-MARKET.md         WVEP market rules (extends 08-WEB-VALUE.md)
8. → 17-WEB-ENTERPRISE.md     managed policy — a strong adoption wedge
```

Dedup note: **WEB-DATA and WEB-SUPPLY-CHAIN are already core slots** (12 and 11).
**WEB-MARKET / WEB-ENERGY partially exist inside `08-WEB-VALUE.md`** (broker
reputation, truth-in-pricing, failure receipts, estimated-not-attested energy).
**WEB-UX-VALIDATION is scoped in plan §21.5.** Those are cross-referenced, not
duplicated.

---

## 13 · → WEB-GOVERNANCE.md  (P1 — highest priority)

Resolver policies, defaults, attestors, logs, and disputes exist — but *who
updates those systems, and how they fail*, is unspecified.

```txt
- how default policies are proposed / reviewed / signed
- how bad resolver nodes are removed
- how attestors lose trust
- how emergency revocations work
- how communities fork defaults
- how users compare policy sets
```

Key question the doc must answer:

```txt
When the default high-risk name list changes, who signed it, why,
and how does a user/community audit the change?
```

Anchor: every default is an `ed:`-signed, replaceable object
(`02-WEB-NAMING.md` §2.6/§9.2); governance is the *process* around swapping them.

## 14 · → WEB-COMPLIANCE.md  (P2)

A *place* for law to touch the system without law entering the protocol.

```txt
- payment / KYC / AML boundary (lives at the broker, not the wire)
- crypto-workload legality
- data-export rights (ties to D-ladder, slot 12)
- takedown / dispute-record handling
- regulated-sector labels: bank, doctor, lawyer, pharmacy (naming §6)
- child-safety policies
- enterprise / legal-hold modes
```

Principle: *legal pressure may affect ranking, warning, and attestation; it must
not erase `ed:`/`b3:`/`name~keytag` access at the protocol layer.*

## 15 · → WEB-ABUSE.md  (P1)

The safety invariants exist; the *incident-response pipeline* does not.

```txt
reports: malicious app · bad broker · cryptojacking · malicious resolver ·
         malware bundle · phishing-name campaign · spam claims ·
         bandwidth-task DDoS · illegal storage task

pipeline: detect → classify → warn → demote → block capability →
          publish dispute/revocation → allow appeal
```

Anchor: demotion/blocking is capability + presentation (plan §11); the address
survives (neutrality principle).

## 16 · → WEB-CATALOG.md  (P2)

Search stays untrusted (plan §10) — but users still need to *browse* apps. A
catalog is a structured, still-untrusted discovery surface.

```txt
CatalogEntry: app authority · screenshots · category · permissions summary ·
              publisher · latest version · safety reports · WVEP/payment model ·
              data-durability class · review attestations
```

Rule: a catalog is a pointer, never a grant of trust (same as search).

## 17 · → WEB-ENTERPRISE.md  (P2 — adoption wedge)

Internal-tools adopters (persona A, plan §21.1) need admin controls.

```txt
managed resolver policy · managed attestors · allowed brokers ·
blocked WVEP classes · required data-export class · app allow/deny lists ·
audit logs · private gateway mirrors · internal namespace roots
```

## 18 · → WEB-I18N-A11Y.md  (P2.5 — gated before name-claim UI ships)

The plan admits ASCII/English keytags are an equity problem (§1c, §21.6). This
makes it a workstream.

```txt
locale keytag dictionaries · screen-reader rendering for identity chips ·
non-Latin name support · mixed-script warnings · RTL address display ·
voice dictation for keytags · QR / contact-card sharing ·
colorblind-safe trust states
```

If identity chrome is the product, it must be accessible. Merges plan §21.6.

## 19 · → WEB-UX-VALIDATION.md  (P0 — the study that gates everything)

Formalizes the identity-UX research protocol (plan §21.5). Numeric success
metrics, not vibes.

```txt
tests: phishing detection · first-contact comprehension ·
       @petname vs name~keytag understanding · permission-prompt comprehension ·
       WVEP-cost comprehension · update-risk comprehension ·
       bare-name ambiguity comprehension

pass bar (plan §21.5): impersonation ATTACK-SUCCESS ≤ 5%;
       ≥90% treat a bare name as a search; beats domain+lock-icon control
```

This is P0, not P2 — it can falsify the core naming bet.

## 20 · → WEB-MARKET.md  (P2 — extends 08-WEB-VALUE.md)

WVEP has broker reputation, truth-in-pricing, and failure receipts already;
this adds the *market rules* around them.

```txt
broker competition · receipt fraud · settlement delay · pricing disclosure ·
value-type comparison · site-greed metrics · user budgets · broker reputation ·
buyer categories · refund / failed-session handling
```

## 21 · → WEB-ENERGY.md  (P2 — extends WVEP's estimated-energy model)

Compute/crypto/storage/bandwidth consume real resources.

```txt
energy-estimate model (estimated, not attested — per WVEP) · optional carbon
disclosure · battery-wear policy · thermal policy · device-health limits ·
monthly energy ledger
```

User-facing: `This site used 0.12 kWh of compute value this month.`

## 22 · → WEB-SYNC.md  (P2 — the honest gap in offline-first)

Offline-first (Tier 0) works cleanly for read-mostly apps; collaborative state
needs a conflict/sync pattern.

```txt
local mutation log · CRDT recommendations · signed operation logs ·
conflict-resolution UI · RPC reconnect semantics · offline-queue permissions
```

## 23 · → WEB-ABI-EVOLUTION.md  (P1 — prevents fossilizing early mistakes)

The ABI is versioned (`docs/ABI.md`); its *evolution rules* are not.

```txt
feature detection · ABI negotiation · deprecation windows · polyfill host
imports · minimum host version · extension/native differences ·
compatibility test suite
```

## 24 · → WEB-OBSERVABILITY.md  (P2 — user-mediated, no hidden telemetry)

Production debugging without violating privacy. Ties to the P2.5 devtools list
(plan §21.2).

```txt
local logs · crash reports · opt-in telemetry · privacy-preserving diagnostics ·
RPC tracing · permission-denial logs · WVEP-session failure logs ·
update-failure reports
```

Rule: no hidden telemetry — every diagnostic is user-mediated (Constitution:
no ambient authority).

## 25 · → WEB-CONFORMANCE.md  (P0/P1 — critical)

Turns "nothing is real until its INVALID vector exists" (naming §2, plan §19)
into a suite + a definition of *what a compatible implementation is*.

```txt
vectors: valid bundle · invalid hash · invalid signature · name-collision ·
         delegation-chain · update-diff · WVEP receipt · permission-denial

defines: "WASIBrowser-compatible host" · "WebNext resolver-compatible node" ·
         "WVEP broker-compatible node"
```

## 26 · → WEB-MODERATION.md  (P2)

The "what about illegal/harmful apps?" question, answered by the neutrality
principle rather than dodged.

```txt
protocol access vs catalog visibility · warning lists · local policy blocks ·
enterprise blocks · child profiles · dispute records · appeal records
```

Core: *the protocol preserves addressability; policies control presentation,
warning, and execution permission.* (The invariant at the top of this doc.)

## 27 · → WEB-PROFILES.md  (P1 — matters a lot for real use)

Identity is master-key-based (plan §10.5); real users have many devices.

```txt
profile master key · device keys · recovery keys · petname sync ·
permission sync · resolver-policy sync · value-ledger sync · lost-device revoke
```

## 28 · → WEB-BUSINESS-RECIPES.md  (P2 — makes the platform legible to builders)

WVEP gives the mechanism; builders need patterns.

```txt
free · paid · pay-per-use · subscription · sponsor-supported · compute-supported ·
enterprise-internal · offline-first paid · public-good (funded by
storage/bandwidth contribution)
```

Each recipe maps to a WVEP configuration (`08-WEB-VALUE.md`) + a manifest
`value_exchange` capability (plan §11.2/§13).

---

## Honest bottom line

```txt
The protocol core is strong and reviewed.
The missing dimensions are governance, abuse ops, data durability, conformance,
supply chain, UX validation, market design, and enterprise policy.
These are the layers that make WebNext survivable in the real world,
not just elegant on paper — and they are P1–P3 authoring, not done.
```

Do not read this doc's existence as "these are handled." It is the *map of what
is not yet handled*, kept honest so the gap can't hide.
