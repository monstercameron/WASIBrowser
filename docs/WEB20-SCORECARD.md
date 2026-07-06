# Web scorecard — Web 2.0 and next.0 against an ideal

Companion analysis to `00-WEBNEXT-OVERVIEW.md`. **Part I** scores the current
mainstream web across every dimension that makes up a web platform, against an
ideal (10/10) spec — with full credit where it genuinely excels. **Part II**
scores next.0 against the *same* rubric, honestly (design-ceiling scores,
heavily discounted for being mostly unbuilt and unadopted). The point of
next.0 is only as good as the honest measure of what it's replacing — and of
itself. A rubric that lowballs the incumbent to flatter the challenger is
useless; the web is the most successful software platform in history for real
reasons, and the *pattern* of where it wins and loses is more persuasive than
any single verdict.

**Scope.** "Web 2.0" here = the current web-app era: JS/SPA frameworks,
REST/GraphQL over cloud origins, DNS + TLS + CDNs, cookies/OAuth, platforms
and app stores. (Web 1.0 = static hypertext; "web3" = the crypto-token
detour; next.0 = `00-WEBNEXT-OVERVIEW.md`.)

**Scale.** `0–2` broken or actively harmful · `3–4` works but structurally
deficient · `5–6` adequate with real limits · `7–8` strong, minor gaps ·
`9–10` near-ideal. Scores are the author's calibrated judgment, not measured
constants — argue with them.

---

## 1. Access & reach  — avg **7.5** (the crown jewel)

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Zero-install access | Click a link, it runs — no install, no store, no gatekeeper | Exactly this; the single best property any platform has ever had | **10** |
| Cross-platform reach | One artifact runs on every OS/device/form-factor | Near-universal via the browser; real quirks but astonishing reach | **9** |
| Addressability | Every resource has a durable, shareable, deep-linkable address | URLs are a great idea, eroded by SPAs, walled gardens, link rot | **6** |
| Discoverability | Anyone can find anything relevant, un-gamed | Search works, but SEO gaming + walled gardens hide much of it | **7** |
| Accessibility | Every user, every ability, first-class by default | Best *model* of any platform (ARIA/semantics), poor real compliance | **6** |
| Internationalization | Every script/locale equal, first-class | Unicode/IDN/i18n solid; Latin-centric defaults linger | **7** |

## 2. Performance & efficiency — avg **3.6** (the weakest technical area)

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Interaction latency | Native-feeling, sub-frame response | Can be great, routinely isn't — JS main-thread, ad-tech jank | **5** |
| Payload / data efficiency | Minimum bytes, dedup'd, delta'd | Median page is multi-MB; frameworks + trackers + uncompressed bloat | **3** |
| Resource use (CPU/mem/battery) | Frugal, backgrounds cost nothing | Heavy; Electron-ification, runaway tabs, wakeful trackers | **3** |
| Offline capability | Works offline by default, network optional | Offline is an exception bolted on via service workers | **3** |
| Cold-start cost | Instant relaunch from local cache | Re-fetch + re-parse + re-compile most visits | **4** |

## 3. Developer experience — avg **6.2**

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Authoring floor | Trivial to start, scales smoothly | Low floor (HTML), but the modern SPA stack is enormous | **7** |
| Language freedom | Any language, first-class | JavaScript monopoly; wasm nascent + DOM-starved | **3** |
| Backend / API model | Minimal ceremony, direct intent | REST/GraphQL workable but MVC/middleware-heavy | **5** |
| Tooling & ecosystem | Deep, stable, trustworthy | Unmatched breadth (npm/devtools/CI) — also churn + supply-chain risk | **8** |
| Debuggability | Deterministic, transparent, easy | Great devtools; flaky async + framework indirection fight back | **6** |
| Deploy / distribute | Push once, reaches everyone | Genuinely easy; static hosts + free tiers | **8** |

## 4. Security — avg **4.3**

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Transport security | Confidential + authentic by default | HTTPS near-universal — a real, hard-won win (CA-tree caveats) | **8** |
| Sandbox / isolation | Hostile code can't escape or peek | Origin + iframe model decent but leaky (XSS, Spectre-class) | **6** |
| Least privilege | No ambient authority; explicit capabilities | Cookies + ambient credentials + CSRF = ambient authority by design | **3** |
| Supply-chain integrity | Every dependency verified, sandboxed | npm + CDN tags + third-party scripts run with full page authority | **2** |
| Content / code integrity | Bytes cryptographically verified before run | You trust the server's *current* bytes; SRI exists, unused | **3** |
| Authentication | Un-phishable, un-breachable, user-owned | Passwords + OAuth; phishing- and breach-prone, provider-dependent | **4** |

## 5. Privacy — avg **2.3** (the moral failure)

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Surveillance resistance | Fetching leaks nothing to third parties | The web IS the surveillance economy's apparatus; recent anti-tracking helps marginally | **2** |
| Data minimization & control | User grants the minimum, sees all flows | Consent theater (cookie banners) over opaque data brokers | **2** |
| Identity unlinkability | No cross-site correlation possible | Global identifiers, fingerprinting, login-with-X correlate everything | **2** |
| Metadata protection | Who-fetched-what is hidden | DNS + SNI + IP + referer leak intent continuously | **3** |

## 6. Decentralization & resilience — avg **2.6**

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Censorship resistance | No single party can remove content | DNS seizure, host takedown, deplatforming, CA revocation all work | **3** |
| Bus-proofing (SPOF) | No single death breaks an app | Origin dies → app dies; one CDN outage → half the web down | **3** |
| Content permanence | Addresses never rot | ~half of older links die; no content addressing at all | **2** |
| Operator independence | Apps aren't bound to a host | Apps welded to specific servers/clouds/domains | **3** |
| Data & identity sovereignty | User owns data, identity, audience | The platform owns all three; you rent your presence | **2** |

## 7. Economics & governance — avg **4.5**

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| Cost to publish | ~Free, no gatekeeper | Low and falling — static hosts, free tiers; a genuine strength | **8** |
| Cost to consume (equity) | Cheap everywhere, esp. on poor links | Bloat costs most where data costs most; deeply inequitable | **4** |
| Monetization health | Incentives aligned with users | Ad-surveillance capture; incentives point away from users | **2** |
| Naming economics | Free/fair human names, no rent | Registrar rent, squatting, scarcity, ICANN politics | **4** |
| Lock-in / monopoly dynamics | Low switching cost, no aggregators | Winner-take-all platforms, app-store taxes, aggregation | **3** |
| Standards / governance | Open, plural, vendor-neutral | Open process (W3C/WHATWG/IETF) undercut by Chromium monoculture | **6** |

## 8. User experience & trust — avg **4.6**

| Dimension | Ideal (10/10) | Web 2.0 | Score |
|---|---|---|---|
| UI capability / richness | Build literally anything | Near-anything is buildable; a real triumph | **9** |
| Consistency / predictability | Coherent, learnable, honest patterns | Every site reinvents UX; dark patterns rampant | **4** |
| Trust legibility | Users can tell safe from hostile | The lock icon means little; phishing thrives | **3** |
| Cross-app interoperability | Data & actions portable between apps | Walled gardens; near-zero portability | **3** |
| Durability of artifacts | What you built still runs in 10 years | Framework churn + API deprecation rot apps fast | **4** |

---

## Aggregate

```
1. Access & reach            ███████▌   7.5   ← what you notice in 5 seconds
8. UX & trust                ████▌      4.6
7. Economics & governance    ████▌      4.5
4. Security                  ████▎      4.3
2. Performance & efficiency  ███▌       3.6
6. Decentralization/resil.   ██▌        2.6
5. Privacy                   ██▎        2.3   ← what you notice in 5 years
   (Developer experience     ██████▏    6.2)

Unweighted overall ≈ 5.3 / 10
```

**Do not trust the 5.3.** An unweighted mean is the wrong summary, and the
*shape* is the real finding: the web scores highest on the virtues you feel
immediately — it opens instantly, runs anywhere, renders anything, is trivial
to publish to — and lowest on the virtues that only surface over time or under
an adversary: privacy, permanence, sovereignty, resilience, efficiency. **Web
2.0 is optimized for the demo, not the decade.** That is exactly why it feels
wonderful and ages badly: the failures are latent, adversarial, and
long-horizon, so they never lose the first impression that wins adoption.

A second pattern: its two genuine near-perfect scores — zero-install reach
(10) and UI richness (9) — are the two things next.0 must *not* regress, or
none of the rest matters. Any successor that is more private, permanent, and
sovereign but harder to open or weaker to build in will simply lose, the way
every "better web" has. The incumbent's crown jewels are the bar, not the
target.

## Where the biggest gaps map to next.0 (honest, not a clean sweep)

The lowest-scoring dimensions are the plan's explicit targets:

- **Privacy 2.3 / sovereignty 2 / unlinkability 2** → app-scoped derived
  identities, no cookies, no global id, oblivious fetch (`00-WEBNEXT-OVERVIEW` §1,
  §5).
- **Permanence 2 / bus-proofing 3 / operator independence 3** → content
  addressing, swarm + gateway-agnostic fetch, offline-first (§2, §3).
- **Supply-chain 2 / code integrity 3 / least privilege 3** → verified
  bundles, host-mediated capabilities, no ambient authority (§10, §11).
- **Data efficiency 3 / cold-start 4 / offline 3** → chunk dedup + delta
  updates + local + compile cache + binary wire (§2, §4b).
- **Language freedom 3** → wasm-first, any-language, binary DOM ABI (the
  runtime layer, already built).
- **Naming economics 4 / lock-in 3** → non-exclusive names, namespaces as
  commons, no registrars (§1b–§1d).
- **Trust legibility 3** → the identity chip + certainty ladder (§10.3, §11.7).

**What next.0 does NOT fix, and shouldn't pretend to** (reality-anchor): the
monetization-model failure (2) is an economic/social problem, not a protocol
one; accessibility compliance (6) and dark-pattern UX (4) are cultural; and
its own hardest risk is regressing the incumbent's 10-and-9 (open, easy) while
chasing the 2s and 3s. The scorecard motivates the plan; it doesn't absolve
it — the plan has its own honest-risks section (§7) for a reason.

---

# Part II — next.0 against the same rubric

Same 37 dimensions, same ideal. **The honesty problem, stated first:** Web 2.0
was scored as a *shipped, planetary-scale, battle-tested* system. next.0 is a
*design doc plus a partial runtime.* Scoring a plan against a deployed system
is structurally generous, so every score below is a **design ceiling** — what
the dimension becomes *if the plan is built as specified AND reaches
meaningful adoption.* Neither "if" is given, and two discounts apply to every
number below. They are the whole story:

1. **Maturity discount.** Most of this is unbuilt. As it stands *today*, the
   only dimensions with a real non-zero score are the runtime-layer ones that
   actually exist and were measured (latency, language freedom, code
   integrity in part). Everything else is ~0–2 today.
2. **Adoption discount.** The biggest wins are network-effect-gated. A swarm
   with no peers is a gateway; a non-exclusive name system with no log quorum
   is a chooser dialog; censorship resistance with no mirrors is a single
   point of failure. "Decentralization: 8" is a *ceiling* worth its realized
   fraction — today, near zero.

Tags: **[S]** structural (guaranteed by architecture if built) · **[P]** proven
(built/measured) · **[A]** adoption-gated · **[X]** execution risk · **[O]**
open/unsolved (plan flags it) · **[R]** regression vs web · **[—]** unaddressed.

### 1. Access & reach — web 7.5 → next.0 **7.2** (−0.3: the crown-jewel dip)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Zero-install access | 10 | **8** | [R][A] runs like a link, offline relaunch — but needs *its own runtime* installed; the web needs none |
| Cross-platform reach | 9 | **8** | [S][X] wasm is portable; the renderer must be ported per platform; mobile unproven |
| Addressability | 6 | **9** | [S] `b3:`/`ed:` addresses never rot — structural; delegation-chain names depend on native-first resolution + the log quorum (slightly less absolute than a raw hash) |
| Discoverability | 7 | **6** | [A][O] no crawl base; indexing a content-addressed web is genuinely harder |
| Accessibility | 6 | **6** | [X] AccessKit hooks exist, but decades behind ARIA maturity |
| Internationalization | 7 | **6** | [O] ascii-only names v1; IDN explicitly deferred |

### 2. Performance & efficiency — web 3.6 → next.0 **8.4** (+4.8)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Interaction latency | 5 | **9** | [P] measured 29 ns/op, 25 µs round-trip, no JS |
| Payload / data efficiency | 3 | **9** | [S] chunk dedup + delta updates + binary wire |
| Resource use | 3 | **7** | [S][X] no per-tab JS engine; GPU renderer isn't free |
| Offline capability | 3 | **9** | [S] offline-first is a Tier-0 invariant |
| Cold-start | 4 | **8** | [S] compile-cache + mmap relaunch; first-ever load still compiles wasm |

### 3. Developer experience — web 6.2 → next.0 **6.3** (+0.1: a wash)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Authoring floor | 7 | **5** | [R] needs a toolchain; no view-source-and-edit-in-notepad |
| Language freedom | 3 | **9** | [P] any-language wasm; Go/Rust/C proven byte-identical |
| Backend / API model | 5 | **8** | [S] RPC-first, capability-scoped, no MVC |
| Tooling & ecosystem | 8 | **3** | [R][A] ~nothing exists; years to approach npm's universe |
| Debuggability | 6 | **5** | [X] golden driver + crash logs, but no mature devtools |
| Deploy / distribute | 8 | **8** | [S] push bytes to any gateway, or seed |

### 4. Security — web 4.3 → next.0 **8.5** (+4.2)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Transport security | 8 | **9** | [S] QUIC/Noise, key-verified endpoints, no CA tree |
| Sandbox / isolation | 6 | **8** | [S] wasm + capability model, host is kernel |
| Least privilege | 3 | **9** | [S] no ambient authority, manifest capabilities |
| Supply-chain integrity | 2 | **7** | [S] signed bundle + cap-limited blast radius (not immune to abused grants) |
| Content / code integrity | 3 | **10** | [S] mandatory, non-bypassable verification — the core thesis |
| Authentication | 4 | **8** | [S][O] key-based, one-signature; key-loss recovery unsolved |

### 5. Privacy — web 2.3 → next.0 **8.3** (+6.0: the largest win)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Surveillance resistance | 2 | **9** | [S] no cookies, no global id, encrypted wire |
| Data minimization & control | 2 | **8** | [S] capability-gated, explicit grants |
| Identity unlinkability | 2 | **9** | [S] HKDF per-app keys, unlinkable by construction |
| Metadata protection | 3 | **7** | [S][A] herd effect + oblivious mode (opt-in; gateways still see hashes) |

### 6. Decentralization & resilience — web 2.6 → next.0 **8.2** (+5.6, most adoption-gated)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Censorship resistance | 3 | **7** | [A][O] DNS-independent, but not censorship-*proof*; needs mirrors to exist |
| Bus-proofing (SPOF) | 3 | **8** | [S][A] any cache serves verified bytes — *if* someone serves them |
| Content permanence | 2 | **8** | [S][A] immortal bytes if seeded; §2b is honest that unpinned decays |
| Operator independence | 3 | **9** | [S] apps aren't host-bound |
| Data & identity sovereignty | 2 | **9** | [S] user owns the master key |

### 7. Economics & governance — web 4.5 → next.0 **6.5** (+2.0)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| Cost to publish | 8 | **8** | [S] gateway or seed |
| Cost to consume (equity) | 4 | **8** | [S] binary + dedup + delta + cache + LAN: cheap on bad/expensive links |
| Monetization health | 2 | **6** | [S][O] §13 WVEP: a full browser-mediated value-exchange protocol (money/attention/compute/storage/bandwidth/judgment → signed receipts), adversarially reviewed to SHIP — structurally better than surveillance ads *and* than a bare payments capability; still rails/broker-market-bootstrap gated (honestly scoped), so not 10 |
| Naming economics | 4 | **7** | [S][A][O] no registrars/rent, word-keytags + native-first resolution now ergonomic (adversarially validated ≥ HTTPS steady-state), but log quorum + keyed-name UX still adoption-unproven |
| Lock-in / monopoly | 3 | **7** | [S][A] portable; but discovery could re-centralize onto gateways/search |
| Standards / governance | 6 | **6** | [O] specs-first, but currently one author's draft, no standards body |

### 8. UX & trust — web 4.6 → next.0 **7.2** (+2.6)

| Dimension | Web | next.0 | Note |
|---|---|---|---|
| UI capability / richness | 9 | **7** | [X][R] Blitz is capable but years behind Blink (forms/IME/CSS/media gaps) |
| Consistency / predictability | 4 | **6** | [S][—] uniform trust chrome; app-level dark patterns remain |
| Trust legibility | 3 | **7 → 3 (unverified)** | [S][O] identity chip + certainty ladder — but the §10.7 usability study hasn't run and label-phishing surfaces keep appearing; the 7 is a design *ceiling*, the defensible score today is 3–4 |
| Cross-app interoperability | 3 | **7** | [S] capability RPC + shared identity substrate |
| Durability of artifacts | 4 | **9** | [S] content-addressed immutable bundles + frozen ABI run unchanged for decades |

## Verdict

```
                    web 2.0   next.0 ceiling   Δ
1 Access & reach      7.5         7.2        −0.3   crown-jewel dip
2 Performance         3.6         8.4        +4.8
3 Developer exp.      6.2         6.3        +0.1   wash
4 Security            4.3         8.5        +4.2
5 Privacy             2.3         8.3        +6.0   largest gain
6 Decentralization    2.6         8.2        +5.6   most adoption-gated
7 Economics/gov       4.5         6.5        +2.0
8 UX & trust          4.6         7.2        +2.6
                    ─────       ─────
overall (unweighted)  5.3         8.7 (ceiling)
```

**Do not trust the 8.7 either** — it is a ceiling, and it multiplies by two
discounts that are, today, brutal (realized ≈ 2). The finding is again the
*shape*, and it is a near-perfect mirror image of Web 2.0's:

- **next.0's gains sit almost entirely in the web's latent-failure quadrants**
  — privacy (+6.0), decentralization (+5.6), performance (+4.8), security
  (+4.2): the things you feel in five *years*.
- **Its non-gains and regressions sit almost entirely in the web's crown
  jewels** — access (−0.3), developer experience (wash), UI richness (9→7),
  tooling (8→3): the things you feel in five *seconds*.

That is the exact fault line the Web 2.0 scorecard warned about, now
quantified: challengers die by regressing the immediate virtues while chasing
the latent ones. The path from next.0's realized ≈ 2 to its ceiling 8.7 runs
*directly through* the dimensions where it currently loses — reach needs its
runtime everywhere; UI needs Blink-class coverage; the ecosystem needs years;
discovery needs a crawl base that doesn't exist.

Two honest holes the plan should own louder:
- **Monetization** *(was the least-examined risk; draft-9 update: now 6, not
  3 — WVEP, §13/08-WEB-VALUE.md, is a full browser-mediated value-exchange
  protocol, adversarially reviewed to SHIP. Still rails/broker-bootstrap
  gated, so not closed — but no longer a void.)*
- **Trust legibility (7)** is a *conditional* score — it collapses to ~3 if
  the "keys must become human" UX problem (§10.7, the plan's own highest-risk
  item) isn't solved.

**Draft-9 refresh (post-8-round adversarial review).** The refinement moved
*confidence* far more than the ceiling: the design earned **2 independent SHIP
verdicts**, and every previously-flagged hole is closed or honestly scoped.
Per-dimension **ceiling** deltas are small — the ceilings were always the
aspiration; refinement made them *defensible*, not higher — with one real
raise: **Monetization 3→6** (Economics/gov category 6.5→**7.0**). Naming
economics (7) is now adversarially validated as ≥ HTTPS steady-state;
sovereignty/dynamic-data gained the declared `data_durability` capability
(the formal fix for the mutable-SaaS weakness). Overall design ceiling
**~8.7 → ~8.8**, essentially flat. **The two discounts are UNCHANGED** —
realized-today still ≈ 2 (mostly unbuilt), and the crown-jewel regressions
(reach, tooling 3, UI-richness 7, authoring-floor 5) are exactly what the P0
spec-family + identity-UX study must attack. The refinement's product is a
*defensible* platform thesis, not a shipped one.

**Draft-11 refresh (§21: personas, dev-ergonomics-proven, C-ladder chips,
attack-success UX gate, privacy tests).** Like draft 9, this moved
*defensibility, not the ceiling*. No Part II category number changed — instead:
Trust legibility's conditional 7 is now backed by concrete render states
(C0–C5 chips) + an auditable first-contact ceremony, but stays gated on the P0
study (still 7→3 defensible). The P0 UX gate got a **better ruler** —
impersonation **attack-success ≤5%** replaces "80% distinguish," so the study
can now fail honestly. Privacy (8.3) became *falsifiable* via five MUST-be-NO
acceptance tests. Language-freedom's 9 now rests on the real byte-identical
`todo-go|rs|c` examples, not a sketch, and devtools/i18n became P2.5
release-blockers. Where it *did* move is Part III (design-maturity): Adoption
realism 8→9, Document structure 8→8.5. Naming-UX held at 7 — the ceremony is
designed, not proven. Overall design ceiling **~8.8, flat**; the two discounts
and the P0 identity-UX gate are unchanged.

**Bottom line:** on this rubric next.0 is not "a better web." It is a
*different bet* — trade the incumbent's latent failures for the challenger's
adoption risk. If built and adopted, it dominates the dimensions that matter
most under an adversary and over a decade. Neither "if" is close to settled,
and an honest score is the ceiling times two discounts. The scorecard says the
bet is worth making; it does not say it is won.

## Postscript — the spec's design maturity (2026-07-06)

`00-WEBNEXT-OVERVIEW.md` was hardened through **six rounds of adversarial review**
(hostile senior-engineer critics, findings 8 blockers → 3+9 → 3+6 → 1+1 →
SHIP → SHIP-confirmed). Two independent final reviewers returned **SHIP**:
easy dev sell, URL/name ergonomics judged **≥ HTTPS in steady state**, and no
remaining blocker or major *design* hole. What that does and does not mean, honestly:

- **It means the DESIGN is at its achievable ceiling** on every dimension a
  design (not deployment) can move: naming ergonomics, verification/integrity,
  privacy structure, capability security, offline/efficiency, the RPC + payment
  + update models, and the honesty of its own scoping.
- **It does NOT mean "10/10 in all dimensions" literally.** An unbuilt spec
  cannot honestly score 10 on tooling ecosystem (4), UI-richness parity (7),
  monetization *rails* (5), or the adoption-gated decentralization dimensions —
  and it doesn't pretend to. The residual sub-ceilings are **adoption-gated or
  open-research** (the §10.7 keys-must-be-human usability study, ecosystem
  size, single implementation, payment-rail partnerships), which both final
  reviewers explicitly ruled *non-blocking* for a ship-quality design but which
  the reality-anchor here refuses to paper over with inflated numbers.

So: the *plan* is done and defensible. The *proof* is in P0–P5 (§8) — and the
honest scorecard still multiplies the 8.7 design ceiling by the maturity and
adoption discounts, which only building collapses.

---

# Part III — the design-maturity meta-rubric (a DIFFERENT rubric, honestly)

Part I/II score **deployed reality** (37 dims), where 10-across is a vanity
target (adoption/maturity discounts). This third rubric scores **spec/design
quality** on the 8 axes an architecture reviewer used — and here 10/10 IS a
legitimate goal, because these measure the *document*, not the deployment.
After the formalization pass (constitution, C/D/V/U ladders, WVEP threat model
+ tiers + reputation + truth-in-pricing + failure receipts, phase acceptance
contracts, prior-art sharpening, spec-family scaffold):

| Axis | pre-review | draft-10 | Gap to 10 (what's LEFT — honest) |
|---|---|---|---|
| Vision | 9 | **10** | constitution added; nothing left |
| Security posture | 8 | **9** | C-ladder/must-never/gates in doc; the normative adversary matrix + test vectors are P0 authoring (`docs/01-WEB-SECURITY.md` stub) |
| Architecture | 8.5 | **9** | objects indexed (§15) + ladders formal; normative CDDL schemas are P0 |
| Adoption realism | 7 | **8** | personas + acceptance contracts added; reach is a structural cap (needs the native runtime / extension traction) |
| Naming UX | 6.5 | **7** | ladders + first-contact ceremony designed; the number is *gated on the P0 usability study* — unproven, honestly not 10 |
| Monetization / WVEP | 8 | **9** | threat model + V-tiers + broker reputation + truth-in-pricing + failure receipts + no-moral-laundering added; real settlement rails are P3 build |
| Implementation path | 7.5 | **9** | phases are now acceptance-tested release contracts; the tests must actually be *run* (build) |
| Document structure | 5.5 | **8** | split scaffolded (index + 08-WEB-VALUE.md written + WEB-SECURITY stub) + glossary + schema index + diagrams; the remaining ~10 specs are P0 extraction |

**The honest last mile.** These refinements took the design-maturity axes from
a ~7.5 mean to a **~8.6 mean** — and the gap from there to a clean 10 on each
is **execution, not more design**: author the P0 spec family (`docs/`), write
the normative CDDL + test vectors, and — the one that gates everything —
**run the P0 identity-UX study**. I will not score Naming-UX, the schemas, or
the split a 10 while they are designed-but-unbuilt; that would be the exact
vanity metric Part II's postscript warns against. The design is at its ceiling;
the proof is P0.
