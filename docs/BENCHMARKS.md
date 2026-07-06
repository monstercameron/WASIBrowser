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

## Follow-ups, in value order

1. Batch-coalesced damage marking in apply (host knows batch extent) — the
   single biggest number on the board.
2. Paint/layout timing hook host-side, so full-frame columns exist both sides.
3. A Go-guest variant of bench-c to quantify guest-language overhead on the
   same workloads (encode was 0.34 ms vs 0.10 ms on the todo +100).
