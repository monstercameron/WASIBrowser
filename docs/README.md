# WebNext spec family — index & status

`00-WEBNEXT-OVERVIEW.md` is the **overview / manifesto / index** (constitution,
tiers, North Star, roadmap, glossary, ladders). The normative specs split out
of it as the **P0 deliverable** (§8/§20). This tracker is the split in
progress — status is honest, not aspirational.

| Doc | Scope | Current home | Status |
|---|---|---|---|
| `00-WEBNEXT-OVERVIEW` | constitution, tiers, roadmap, glossary | 00-WEBNEXT-OVERVIEW.md | ✅ is the index |
| `01-WEB-SECURITY.md` | adversary matrix, C-ladder, trusted chrome, must-never | §10 (draft) + stub | 🟡 stub (matrix P0) |
| `02-WEB-NAMING.md` | b3/ed/@petname/name~keytag/bare/delegation/dns ramp | §1 (draft) + **written** | 🟡 social-resolution + resolver-node half authored; address-forms/native-first/DNS-ramp still P0 extract from §1 |
| `03-WEB-BUNDLE.md` | bundle format, chunks, manifests, lifecycle, cache, GC, compiled | §2 (draft) | ⬜ P0 extract |
| `04-WEB-RPC.md` | service identity, interface schema, wire, RPC events, canonical ifaces | §4 (draft) | ⬜ P0 extract |
| `05-WEB-PERMISSIONS.md` | capability vocab, wasmtime gates, task manager, safe mode | §11 (draft) | ⬜ P0 extract |
| `06-WEB-UPDATES.md` | U-ladder diff, rollback, key rotation, storage migration | §11.6 + §18 | ⬜ P0 extract |
| `07-WEB-SWARM.md` | gateways, LAN, DHT, Wirehair, relays, privacy modes | §3 (draft) | ⬜ P0 extract |
| `08-WEB-VALUE.md` | WVEP: offers, sessions, receipts, V-ladder, threat model | **08-WEB-VALUE.md** | ✅ written (3-round SHIP) |
| `09-WEB-MIGRATION.md` | extension/native split, dns legacy surface, shim, onboarding | §0c (draft) | ⬜ P0 extract |
| `10-WEB-UX.md` | identity chrome, first-contact ceremony, prompts, value UI | §10.3/§11.7 | ⬜ P0 + the UX study |
| `11-WEB-PACKAGES.md` | deps as ed:/b3: refs | §17.2 | ⬜ P0 |
| `12-WEB-DATA.md` | D-ladder durability + export ABI | §2b + §18 | ⬜ P0 |
| `SCHEMAS/` | CDDL: Bundle/Publisher/Service/Interface/Capability/NameClaim/Delegation/Defaults/ValueOffer/ValueReceipt/Recovery/UpdateDiff **+ naming: TopLabelClaim/ClaimInclusionProof/QuorumClaim/KeytagAssignment/NameAttestation/ResolverPolicy/AliasBinding/DisputeRecord/ClaimRenewal/ClaimSuccession/ResolverHead** (02-WEB-NAMING §2) | §15 index | ⬜ P0 normative CDDL |
| `TEST-VECTORS/` | valid + INVALID examples per object (nothing is real until its invalid case exists) | — | ⬜ P0 |

**Legend:** ✅ done · 🟡 stub/first-draft · ⬜ not yet (P0). The gap from "draft in
§X" to "✅ written normative spec" is P0 execution, not more design — the design
passed 8 adversarial rounds; the specs now need authoring + CDDL + test vectors.

## Tier-2 — the operational / ecosystem layer (P1–P3, mostly undesigned)

The core above is the *protocol*. These are the governance / legal / abuse /
ecosystem layers that turn a protocol into a durable platform. They are held —
honestly, as scaffolds, NOT as finished specs — in **`OPERATIONAL-LAYER.md`**,
and each graduates to its own numbered doc when it earns normative content. The
principle threaded through all: *the protocol preserves addressability; policy
controls presentation, warning, and execution permission.*

| Doc (graduates to) | Scope | Status |
|---|---|---|
| `13-WEB-GOVERNANCE.md` | who updates defaults/policies/logs; how they fail; forks; audit | ⬜ P1 (scaffold) |
| `14-WEB-COMPLIANCE.md` | KYC/AML boundary, takedown, regulated labels, legal-hold | ⬜ P2 (scaffold) |
| `15-WEB-ABUSE.md` | detect→classify→warn→demote→block→dispute→appeal pipeline | ⬜ P1 (scaffold) |
| `16-WEB-CATALOG.md` | structured *untrusted* app-discovery surface | ⬜ P2 (scaffold) |
| `17-WEB-ENTERPRISE.md` | managed policy/attestors/brokers, allow-lists, audit — adoption wedge | ⬜ P2 (scaffold) |
| `18-WEB-I18N-A11Y.md` | locale keytags, screen-reader chips, RTL, QR — merges §21.6 | ⬜ P2.5 (scaffold) |
| `19-WEB-UX-VALIDATION.md` | the identity-UX study protocol (plan §21.5) — **P0, gates the naming bet** | ⬜ P0 (scaffold) |
| `20-WEB-MARKET.md` | WVEP market rules — extends `08-WEB-VALUE.md` | ⬜ P2 (scaffold) |
| `21-WEB-ENERGY.md` | energy ledger — extends WVEP estimated-energy | ⬜ P2 (scaffold) |
| `22-WEB-SYNC.md` | offline conflict/CRDT/signed-op-log — the honest offline-first gap | ⬜ P2 (scaffold) |
| `23-WEB-ABI-EVOLUTION.md` | ABI negotiation/deprecation/feature-detect (extends `ABI.md`) | ⬜ P1 (scaffold) |
| `24-WEB-OBSERVABILITY.md` | user-mediated diagnostics, no hidden telemetry | ⬜ P2 (scaffold) |
| `25-WEB-CONFORMANCE.md` | test-vector suite + "compatible implementation" definitions | ⬜ P0/P1 (scaffold) |
| `26-WEB-MODERATION.md` | access vs visibility; warning/block/appeal | ⬜ P2 (scaffold) |
| `27-WEB-PROFILES.md` | master/device/recovery keys, multi-device sync | ⬜ P1 (scaffold) |
| `28-WEB-BUSINESS-RECIPES.md` | free/paid/sub/sponsor/compute recipes over WVEP | ⬜ P2 (scaffold) |

Already covered by the core, NOT re-authored here: **data durability** =
`12-WEB-DATA.md`, **supply chain** = `11-WEB-PACKAGES.md` (scope expanded to
SBOM/reproducible-builds/provenance).
