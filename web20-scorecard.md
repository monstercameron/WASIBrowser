# Web 2.0 — a full-dimension scorecard against an ideal

Companion analysis to `plan-webnext.md`. The point of next.0 is only as good
as the honest measure of what it's replacing. So this scores the **current
mainstream web** across every dimension that makes up a web platform, against
an ideal (10/10) spec on each — with full credit where the web genuinely
excels. A rubric that lowballs the incumbent to flatter the challenger is
useless; the web is the most successful software platform in history for real
reasons, and the *pattern* of where it wins and loses is more persuasive than
any single verdict.

**Scope.** "Web 2.0" here = the current web-app era: JS/SPA frameworks,
REST/GraphQL over cloud origins, DNS + TLS + CDNs, cookies/OAuth, platforms
and app stores. (Web 1.0 = static hypertext; "web3" = the crypto-token
detour; next.0 = `plan-webnext.md`.)

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
  identities, no cookies, no global id, oblivious fetch (`plan-webnext` §1,
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
