# 08-WEB-VALUE.md — WVEP: Browser-Mediated Value Exchange (Proof-of-Value)

### Extracted from 00-WEBNEXT-OVERVIEW.md §13 (the overview/manifesto index). A
### standalone spec because WVEP is both a protocol and an abuse magnet
### (financial fraud, cryptojacking, labor exploitation, bandwidth/storage
### abuse, sponsor tracking, broker collusion, receipt fraud, fingerprinting)
### and deserves its own threat model. Passed a 3-round adversarial review
### (ITERATE 1blk+3maj+3min -> SHIP -> SHIP-confirmed, 2 independent verdicts).

> **The one non-negotiable invariant, repeated everywhere:**
> **A site may REQUEST value. Only the browser may EXTRACT value.**

## WVEP — Browser-Mediated Value Exchange (Proof-of-Value)

Monetization is the plan's most-cited hole (scorecard 3→6; §4a's `payments`
capability alone reached 5 — "narrowed, not closed" — and WVEP closes it
further to 6). WVEP does this by generalizing "payment" into **browser-mediated value
exchange**: an app may *request value*, and the browser — not the app —
decides whether, how, and under what bounds the user contributes it, handing
the app a signed **receipt**, never device control. §4a's `payments` is
exactly the PAYMENT class of WVEP; everything below reuses the plan's own
machinery (signed `ed:`/`b3:` authorities, content-addressed workers,
host-mediated capabilities, trusted chrome, the chunk store, lifecycle).

**The inversion that makes it fit next.0:**
> The current web monetizes users by HIDING the cost. next.0 monetizes access
> by EXPOSING it — explicit, bounded, consented, metered, auditable,
> revocable, settled.

Cost is not only money. WVEP defines **six first-class value classes**, each
producing a common `ValueReceipt`:

```
PAYMENT     money / entitlement            (the §4a capability)
ATTENTION   a browser-framed sponsor msg   (no 3rd-party scripts, no tracking)
COMPUTE     bounded useful work OR crypto   (strictest sandbox in the system)
STORAGE     verified public/encrypted chunk custody   (reuses §2 chunk store)
BANDWIDTH   verified serving/relay of allowed chunks   (never a DDoS/proxy)
JUDGMENT    consented human labeling/rating/eval       (category-disclosed)
```

### 13.1 The flow — offer to session to receipt

Same discipline as the rest of the plan: signed authorities, content-addressed
workers, host-mediated, receipted.

```
1. app presents a signed ValueOffer (binds ONE resource, lists accepted
   classes + settlement recipient, expires, nonce)
2. host validates the offer + checks the user's value policy
3. host shows the REAL cost of each option (§13.3)
4. user picks a path (or the host auto-fulfills within a pre-granted limit)
5. host runs a bounded ValueSession (metered, killable) via an accepted broker
6. broker/provider signs a ValueReceipt over the metered cost (cpu/wall
   fuel-attestable; energy only an estimate — see below)
7. app verifies receipt sig + offer-hash and unlocks — it never sees the
   user's identity or device, only the receipt
```

```json
// ValueOffer (signed by the site authority)
{ "type": "webnext.value.offer", "v": 1,
  "site": "ed:site_pubkey",
  "resource": { "kind": "article", "id": "b3:...", "name": "Read this" },
  "accepted": [
    { "kind": "PAYMENT",   "amount": { "USD": "0.10" } },
    { "kind": "ATTENTION", "max_wall_s": 30 },
    { "kind": "COMPUTE",   "credits": 12, "max_cpu_ms": 90000,
                           "worker_bundle": "b3:...",   // §2b-lifecycle, verified
                           "allow_crypto": true, "allow_useful": true },
    { "kind": "JUDGMENT",  "tasks": 3, "max_wall_s": 120 } ],
  "settlement": { "recipient": "ed:site_revenue_key" },
  "expires": "...", "nonce": "b3:...", "sig": "..." }

// ValueReceipt (signed by an accepted broker/provider; site needs only this)
{ "type": "webnext.value.receipt", "v": 1,
  "offer_hash": "b3:...", "site": "ed:...", "resource": "b3:...",
  "value": { "kind": "COMPUTE", "class": "useful_compute", "credits": 12 },
  "metered": { "cpu_ms": 87312, "wall_ms": 362000 },   // fuel/epoch-attestable
  "estimated": { "energy_wh": 4.1 },                   // best-effort, NOT attested
  "broker": "ed:...", "settled_to": "ed:site_revenue_key",
  "privacy": { "user_identity_disclosed": false, "relay_used": true },
  "expires": "...", "sig": "..." }
```

Authorities are the plan's own — `site/broker/buyer = ed:` or `name~keytag`,
`worker_bundle = b3:`. Bare names never suffice (§10 R1): `settle_to:
"ed:8f2a..."` or `dailyledger~cedar-finch`, never `settle_to: "news-site"`.
**What's attestable vs estimated (honesty):** the receipt separates `metered`
values the fuel/epoch limiter (§11.4) can actually attest (`cpu_ms`,
`wall_ms`, bytes, storage) from `estimated` ones it can't — notably
**`energy_wh` is a best-effort estimate, not a cryptographic fact** (there is
no portable per-workload power API; thermal/voltage/shared-rail variance make
kWh unattestable), shown with an "est." qualifier everywhere and never
presented as metered truth.

### 13.2 Declared capability, strictest worker sandbox

An app can request value only if its manifest declares `value_exchange`
(§11.2) with accepted classes + per-class caps — no declaration, no ask. The
COMPUTE worker is the **strictest sandbox in the system**, tighter than a
normal app (§10 R3, §11.3): no DOM, no app storage, no user files / identity /
clipboard / camera, **no arbitrary network (broker-only)**, scratch space
only, one input object in → one output object out. It runs under the §11.4
limiter + fuel/epoch budgets, shows up in the Task Manager as a live session,
pauses on battery/thermal pressure, and is force-killable. Worker bundles are
`b3:` objects under the §2b lifecycle (revocation lists, no silent permission
expansion on update — §10 R4, mechanism in §11.6).

### 13.3 Cost transparency + the greed ledger

No "free" label unless the real cost is zero. Every option discloses its true
price — money amount + recipient; attention duration + sponsor + tracking
status; compute CPU-time + estimated energy/heat + broker + category;
storage/bandwidth caps + serving obligations; judgment task-count + category +
sensitivity. The host keeps a per-site **Value Ledger** so users can see who
is greedy — the honesty mechanism that makes site-set prices safe:

```
Daily Ledger · cedar-finch — this month: 12 unlocks · 18m CPU · 0.12 kWh est.
  avg 91s gentle compute/article ·  (!) asks 5.8x more compute than similar sites
```

### 13.4 Crypto generation — allowed, never cryptojacking

The spiciest capability gets the tightest rules. Crypto generation is a
**subclass of COMPUTE, not special authority** — a compute workload with
stricter disclosure. Hard invariants: opt-in only; **battery = disabled by
default**, background = disabled; must show coin/network + recipient + pool +
intensity + duration in **trusted chrome** (§10.6); instantly stoppable;
budget-bounded (CPU fuel/epoch, §11.4); category is declared, and a mislabeled
workload (crypto dressed as "science") is caught the same way over-declared
permissions are (§11.5): **runs-and-observes + broker reputation**, not
real-time semantic verification — a black-box WASM worker's actual computation
can't be proven against its label by the limiter alone, and the doc says so
rather than asserting flat enforcement. Plus a greed disclosure the ad web never
gives: *"this crypto ask is worth ~$0.002 to unlock — direct payment is
cheaper."* This is how it stays honest instead of recreating browser
cryptojacking. **CPU-only, and honestly so:** value workers are CPU compute
under the fuel/epoch limiter (§11.4); **GPU compute is out of scope** — wasmtime
has no GPU-sandboxing primitive, so exposing GPU to a guest worker would be an
un-bounded, un-metered capability with no enforcement story, and the plan does
not hand-wave one. (The renderer's own Vello/wgpu GPU use is host-controlled,
never guest-exposed.) A GPU value class waits on a real sandbox design.

**"Declared pool" is the broker's disclosure, not a second network
destination.** The sandboxed worker's network stays **broker-only** (§13.2),
full stop — it never reaches a raw Stratum pool (which has no `ed:` identity,
so trusting one directly would violate R1). The "pool" shown in trusted chrome
is what the accountable `ed:` **broker** discloses and bridges to, exactly as
payments settlement bridges to cards/bank rails (§13.5): the worker→host→broker
relationship is unchanged; the broker does the legacy-pool bridging.

### 13.5 Brokers, settlement, scale — no blockchain required

A **broker** matches demand to work, verifies contribution, and signs
receipts; it is an accountable `ed:` service (its manifest declares supported
classes / categories / audit-log / policy), allow/blockable in user policy,
and **federated** — multiple brokers may satisfy one offer and the host picks
on price / privacy / energy / trust / rejection-rate. Settlement is pluggable
(broker ledger, fiat, crypto payout, subscription credit, org entitlement) and
**no chain is required** — the simplest model is buyer-funds-broker → session
completes → broker signs receipt → credits the site. Scale comes from
**stateless receipt verification**: a site verifies a receipt's signature +
offer-hash + expiry and unlocks, never re-contacting the user — horizontally
scalable, and storage/bandwidth reuse the §2 chunk store + §2b lifecycle
wholesale.

**Brokers are NOT a new tracker — the unlinkability invariant (§1, §5) is
enforced here too.** A worker presents to a broker the *same* app-scoped
derived key as everything else — `HKDF(master, site_authority)` — so a broker
sees a **per-site pseudonym, never a per-device/per-user identity**, and
cannot correlate "this worker did jobs for site A, B, and C" across sites, by
construction (exactly the guarantee §4a states for payment payees). Any
routing signal a broker uses is **anonymous/aggregate hardware class only**
(e.g. "8-core laptop, plugged in") with **no persistent worker id, even in
Fast mode**; there is deliberately no cross-session per-worker reputation by
default (a stronger continuity signal is an explicit opt-in tier, never the
default — it would trade away the invariant). (That "no persistent id" is
about the cryptographic *identity* axis; the broker-visible *IP* in Fast mode
is a separate correlation channel governed by §5's privacy tiers — Private
mode relays it away — not the identity guarantee, and the two shouldn't be
conflated.) Notably there is **no broker→worker payout leg at all**: the
user's contribution *is* the payment, redeemed as access, and settlement flows
only broker→site (`settled_to: ed:site_revenue_key`) — so the classic
"paying the contributor needs a correlatable identity" problem simply does not
arise, crypto included (the *broker*, with its own accountable `ed:`, bridges
to the pool). Privacy modes then layer onto
§5: *Fast* (direct broker connection, per-site pseudonym), *Private* (relay
the broker traffic too, so the broker sees neither IP nor a stable class),
*Strict* (money/attention only — hardware can't be fingerprinted by work it
never does).

### 13.6 Safety invariants (extend §10, don't replace it)

WVEP's non-negotiables are §10's rules applied to value: no value extraction
without a browser-mediated, signed-offer session; no execution without a
verified worker bundle; no compute worker with DOM or ambient network; no
unbounded CPU/mem/disk/net (GPU not exposed, §13.4); no battery-compute or
crypto by default; no third-party attention tracking; no contribution of
private user data; no receipt without a provider signature; no silent
permission expansion on worker update; the user can kill any session; the host
records per-site actual cost. Value prompts, the live-session indicator, and
kill switches are **browser-native only** (§10.6) — an app can't draw a fake
pay/mine dialog.

**Honest open risk — JUDGMENT content the protocol can't police.** The
runtime enforces *technical* bounds on human-judgment tasks (count,
wall-clock, disclosed category, sensitivity flag) — but it has **no way to
vet the moral/legal fitness of the task content itself**: a "3 quick tasks to
unlock" flow could be labeling CSAM, self-harm, or disinformation dressed as
benign eval. This is named, not hidden (same discipline as §4a's monetization
scope and §10.4's "the protocol verifies *who* answered, not that the answer
is *fair*"): category-disclosure + bounds are enforced; content fitness is
**unsolved by the protocol**, mitigated only by broker accountability +
reputation + user-policy category blocks, and that limit is stated plainly.

### 13.7 Why it fits, and why it beats the ad model

Today the site embeds third-party scripts, tracks the user, and hides the
attention/privacy cost. WVEP: the app *asks*, the browser *shows the choices
and the real cost*, the runtime *enforces the bounds*, the receipt *proves the
contribution*, the ledger *shows what it actually cost*. It doesn't ban ads —
it makes attention **compete openly** against payment, compute, storage,
bandwidth, and labor, every option's cost on the table. That is the same move
as all of next.0: replace hidden ambient cost with explicit, bounded,
receipted exchange. WVEP's job is not to decide which cost is morally best —
it's to make every cost explicit, bounded, consented, metered, auditable,
revocable, and settled.

---

## 13.8 Value-risk tiers (V-ladder) and default policy

Every value class carries a risk tier that drives default policy:

```
V0 payment/entitlement         V4 bandwidth, public chunks
V1 sponsor, no tracking        V5 human judgment, low-sensitivity
V2 storage, public/encrypted   V6 crypto / GPU-N/A / networked / sensitive — HIGH
V3 useful CPU compute
```

Policy: **V0–V2 low** (allow within caps), **V3–V4 medium** (allow, metered,
visible), **V6 high** (opt-in + trusted-chrome disclosure + battery-off +
instantly killable). Categories `credential-attack / DDoS / proxying /
surveillance / malware / unknown-networked` are **blocked by default**, never
merely "asked." A capability needing trust runs only at C1/C2+ (§10.3), never
under a C4 bare name.

## 13.9 Broker reputation (accountability without a chain)

A broker is not trusted; it is *accountable*. The browser keeps a local broker
score and shows it before use:

```
Broker score keys:  invalid-receipt rate · rejected-work rate · user complaints
                    · hidden-category violations · over-budget attempts
                    · worker revocations · settlement-failure rate

  Broker: OpenCompute · trusted · settlement reliability 99.8% · violations: none
```

A broker that fakes receipts, over-runs budgets, or mislabels categories drops
out of the user's accepted set — the "slashing" is loss of accepted-broker
status, no token required.

## 13.10 Truth-in-pricing (a named WVEP invariant)

Every offer MUST disclose, before the user chooses: site recipient · broker ·
buyer/sponsor (if any) · estimated cost · (after) actual metered cost ·
whether tracking occurs · whether identity is disclosed · whether crypto is
involved · whether the network is used. No option may render "free" unless the
real cost is genuinely zero. This is the WVEP counterpart to §10 R1.

## 13.11 Failure receipts (greed is auditable even when nothing unlocks)

Even an aborted/failed session produces a **local** `ValueSessionFailureReceipt`
`{ site, broker, reason, cost_already_consumed, attempted_overrun? }`. It never
unlocks anything, but it lets the greed ledger (§13.3) surface sites that
burn user resources without delivering, and sites that repeatedly try to
exceed their declared budget.

## 13.12 No moral laundering (the honest boundary)

> **WVEP bounds and meters value exchange. It does NOT certify the buyer's
> purpose is good.** Brokers are accountable for their *category* claims and
> browsers enforce *user policy* — but the protocol cannot vouch that a
> compute job, a crypto network, a judgment task, or a sponsor is ethical or
> legal. That fitness is a reputation/policy problem (mirrors §10.4: "the
> protocol verifies *who* answered, not that the answer is *fair*"), stated
> plainly rather than laundered behind "it's just compute."
