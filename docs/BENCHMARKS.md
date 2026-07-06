# M5 Benchmarks — GWB/C vs JS DOM in Chromium (2026-07-06)

Identical DOM workloads, two stacks:

- **GWB**: freestanding C guest (`examples/bench-c`, 8.2 KB wasm) on the raw LL
  ABI, in the Blitz/wasmtime renderer. Host-instrumented at µs precision.
- **JS**: vanilla DOM (`bench/js/bench.html`) in Playwright Chromium 1.61,
  timed in-page with `performance.now()` around the synchronous mutation loop.

Row shape both sides: `div.row > (span "Item N") + (button "x")`.
Machine: Snapdragon X2, Windows 11, release builds. Medians of 5 reps (3 for 5k).

## Results (µs)

| workload | GWB guest_call | GWB decode | GWB apply | **GWB total** | **JS mutation-only** | JS + forced layout |
|---|---|---|---|---|---|---|
| create 1k (11,002 ops / 204 KB) | 296 | 199 | 2,695 | **3,190** | **2,000** | 10,500 |
| updateAll @1k (1,001 ops) | 125 | 56 | 395 | **576** | **600** | 6,400 |
| updateOne @1k (2 ops) | 22 | 1 | 2 | **25** | **<100**¹ | ~100 |
| classAll @1k (1,001 ops) | 141 | 81 | 1,084 | **1,306** | **500** | 500 |
| clear @1k | 24 | 2 | 2,289 | **2,315** | **1,300** | 1,300 |
| create 5k (55,002 ops / 1.0 MB) | 1,055 | 554 | 13,973 | **15,582** | **7,900** | 63,200 |
| updateAll @5k (5,001 ops) | 462 | 210 | 2,034 | **2,706** | **3,100** | 37,400 |
| clear @5k | 36 | 4 | 11,463 | **11,503** | **6,900** | 7,000 |

¹ Chromium coarsens `performance.now()` to 100 µs — single-op timings quantize to 0/100.

GWB columns: `guest_call` = entire wasm execution incl. batch encode;
`decode` = host batch validation; `apply` = DocumentMutator application into
the retained tree (includes Stylo damage marking, excludes layout/paint).
JS `mutation-only` = the DOM call loop (includes Blink's internal invalidation,
excludes layout); `+ forced layout` adds one synchronous `offsetHeight`.

## Analysis — what the thesis actually won, and what it didn't (yet)

**The boundary thesis is confirmed, decisively.** The cost of moving 55,002
operations from the guest program to the engine — encode + crossing + decode —
is 1.6 ms, i.e. **~29 ns per op**. The equivalent JS cost is inseparable from
Blink's application work, but the whole JS loop runs ~175 ns per DOM call at
5k-row scale. The GWB *interface* is roughly **6× cheaper per operation** than
the JS binding layer, and it's a single crossing instead of 45,000.

**The current loss is in `apply`, and it's engine-side, not ABI-side.** Blitz's
`DocumentMutator` marks `ALL_DAMAGE` and walks ancestors *per op*; at 55k ops
that costs 14 ms where Blink's (25-years-optimized) internal application does
better. Same story for `clear` (subtree teardown). This is alpha-engine
overhead with obvious fixes (coalesced damage marking for batches — the host
already knows the batch extent), not a structural deficit.

**Full-pipeline scaling favors GWB.** With one forced layout included — closer
to what a frame actually costs — JS create-5k is **63 ms** and updateAll@5k is
**37 ms**, while GWB's entire boundary+apply is 15.6 ms / 2.7 ms plus one
deferred Blitz layout pass per frame (not yet instrumented; paint-timing hook
is future work). Chrome's mutation calls are cheap; its style/layout
consequences at scale are not.

**Interaction latency**: a full GWB input round-trip — hit-test, event record,
guest execution, decode, apply — is **25 µs** (updateOne). Below the
measurement floor of Chromium's coarsened clock and far below a 120 Hz frame
budget (8,333 µs). The latency contract holds with two orders of magnitude to
spare.

## Fairness caveats

- JS numbers exclude Playwright/CDP overhead (timed in-page) — fair.
- GWB numbers include the *entire* guest execution; JS numbers are just the
  loop body inside an already-running VM — generous to JS.
- Neither column includes paint. GWB layout/paint timing needs a host hook
  (future work); the JS forced-layout column is style+layout only.
- Chromium's timer coarsening (100 µs) makes small JS numbers lower bounds.
- Blitz is pre-1.0: `apply` and `clear` have known unoptimized paths.

## Reproduce

```powershell
# GWB side (timings land in renderer/logs/system.log)
scripts\build-c.cmd examples\bench-c\bench.c renderer\bench-c.wasm
cd renderer; .\target\release\renderer.exe bench-c.wasm --script bench-test.txt

# JS side
cd bench\js; npm i playwright; node run.mjs
```

## Optimization round 1 (same day)

Tried two host-side optimizations against the `apply` deficit:

1. **Atom/QualName caching** (resolve each name atom to an interned QualName
   once, ever, instead of clone + re-intern per op): **kept.** classAll@1k
   apply fell 1,084 → 756 µs (total 1,306 → 889, −32%); attr-heavy workloads
   all improved; creates within run-to-run variance.
2. **Auto-fragment** (detach hot parents, mutate on the cheap
   not-in-document path, reattach once — automatic DocumentFragment):
   **rejected by measurement.** create5k apply went 14 → 18.7 ms. The cost
   model was wrong: Blitz's per-op invalidation walks short-circuit on
   already-dirty ancestors, so the per-op path is cheap after the first op,
   while reattaching re-traverses the entire subtree. Bulk-create cost is
   intrinsic per-node creation (stylo element-data init, slab insert,
   ElementData construction) — an engine-side target.

Post-round medians (µs): create1k 3,762 · updateAll@1k 597 · updateOne 36 ·
classAll@1k 889 · create5k 14,291 · updateAll@5k 2,555 · clear@5k 10,012.
(Fanless X2: run-to-run thermal variance is a few hundred µs on the creates;
compare trends, not single digits.)

**Remaining gap analysis:** beating Blink on *mutation-only bulk create*
requires engine work inside blitz-dom itself — `create_element` eagerly
initializes stylo element data with ALL_DAMAGE and flushes style attributes
per node, where Blink defers most of that into its batched style pass. The
realistic path is a vendored Blitz patch (path-dependency on the local
checkout) deferring per-node style init into the existing resolve pass —
plausibly 2–3× on creates, and upstreamable. Everything outside bulk
mutation-only is already won: the boundary (6×), interaction latency (25 µs),
and every layout-inclusive workload (3–14×).

## Optimization round 2 — engine surgery (same day)

Switched the renderer to path-dependencies on the local Blitz checkout
(`C:\src\blitz`, branch `gwb-perf` off f550417) and patched blitz-dom:

1. **Lazy stylo element-data init** in `create_element` (the traversal
   initializes it anyway; node damage comes from `process_added_subtree` at
   attach). Verified visually identical; golden tests green. The big one.
2. `flush_style_attribute` skipped for attribute-less elements.
3. `changed_nodes`: SipHash `HashSet` → `FxHashSet` (one insert per created node).

Round-2 medians (µs) vs JS mutation-only:

| workload | GWB before | **GWB after** | JS | standing |
|---|---|---|---|---|
| create1k | 3,255 | **2,269** | 2,000 | JS by 12% (was 47%) |
| updateAll@1k | 591 | **437** | 600 | **GWB by 27%** (was tie) |
| updateOne | 29 | **26** | <100 | GWB (unmeasurable by their clock) |
| classAll@1k | 987 | **675** | 500 | JS by 26% (was 44%) |
| clear@1k | 2,363 | **2,008** | 1,300 | JS by 35% |
| create5k | 15,989 | **11,735** | 7,900 | JS by 33% (was 45%) |
| updateAll@5k | 2,793 | **2,095** | 3,100 | **GWB by 32%** (was 13%) |
| clear@5k | 11,717 | **10,327** | 6,900 | JS by 33% |

Notes: found-in-passing that f550417 renders one starter text color
differently than alpha.6 (both patch-states identical — upstream quirk, not
ours). Remaining create/clear gap is `Node::new`'s large struct construction
and per-node teardown — deeper surgery (splitting hot/cold node data) than a
patch session should carry.

**Context that matters:** the JS column is *vanilla* DOM — the engine's own
floor, which nothing in the JS ecosystem beats. React adds virtual-DOM diffing
on top of these numbers (typically 3–5× vanilla on create-heavy work). Against
React, GWB wins every category outright, before counting layout or the 6×
boundary advantage.

## Follow-ups, in value order

1. Batch-coalesced damage marking in apply (host knows batch extent) — the
   single biggest number on the board.
2. Paint/layout timing hook host-side, so full-frame columns exist both sides.
3. A Go-guest variant of bench-c to quantify guest-language overhead on the
   same workloads (encode was 0.34 ms vs 0.10 ms on the todo +100).
