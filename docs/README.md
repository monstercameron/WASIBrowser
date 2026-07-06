# WebNext spec family — index & status

`../plan-webnext.md` is the **overview / manifesto / index** (constitution,
tiers, North Star, roadmap, glossary, ladders). The normative specs split out
of it as the **P0 deliverable** (§8/§20). This tracker is the split in
progress — status is honest, not aspirational.

| Doc | Scope | Current home | Status |
|---|---|---|---|
| `00-WEBNEXT-OVERVIEW` | constitution, tiers, roadmap, glossary | plan-webnext.md | ✅ is the index |
| `01-WEB-SECURITY.md` | adversary matrix, C-ladder, trusted chrome, must-never | §10 (draft) + stub | 🟡 stub (matrix P0) |
| `02-WEB-NAMING.md` | b3/ed/@petname/name~keytag/bare/delegation/dns ramp | §1 (draft) | ⬜ P0 extract |
| `03-WEB-BUNDLE.md` | bundle format, chunks, manifests, lifecycle, cache, GC, compiled | §2 (draft) | ⬜ P0 extract |
| `04-WEB-RPC.md` | service identity, interface schema, wire, RPC events, canonical ifaces | §4 (draft) | ⬜ P0 extract |
| `05-WEB-PERMISSIONS.md` | capability vocab, wasmtime gates, task manager, safe mode | §11 (draft) | ⬜ P0 extract |
| `06-WEB-UPDATES.md` | U-ladder diff, rollback, key rotation, storage migration | §11.6 + §18 | ⬜ P0 extract |
| `07-WEB-SWARM.md` | gateways, LAN, DHT, Wirehair, relays, privacy modes | §3 (draft) | ⬜ P0 extract |
| `08-WEB-VALUE.md` | WVEP: offers, sessions, receipts, V-ladder, threat model | **WEB-VALUE.md** | ✅ written (3-round SHIP) |
| `09-WEB-MIGRATION.md` | extension/native split, dns legacy surface, shim, onboarding | §0c (draft) | ⬜ P0 extract |
| `10-WEB-UX.md` | identity chrome, first-contact ceremony, prompts, value UI | §10.3/§11.7 | ⬜ P0 + the UX study |
| `11-WEB-PACKAGES.md` | deps as ed:/b3: refs | §17.2 | ⬜ P0 |
| `12-WEB-DATA.md` | D-ladder durability + export ABI | §2b + §18 | ⬜ P0 |
| `SCHEMAS/` | CDDL: Bundle/Publisher/Service/Interface/Capability/NameClaim/Delegation/Defaults/ValueOffer/ValueReceipt/Recovery/UpdateDiff | §15 index | ⬜ P0 normative CDDL |
| `TEST-VECTORS/` | valid + INVALID examples per object (nothing is real until its invalid case exists) | — | ⬜ P0 |

**Legend:** ✅ done · 🟡 stub started · ⬜ not yet (P0). The gap from "draft in
§X" to "✅ written normative spec" is P0 execution, not more design — the design
passed 8 adversarial rounds; the specs now need authoring + CDDL + test vectors.
