# GoWebBrowser

A **wasm-first** browser platform. Any language compiles to wasm and drives the
page through a fast binary DOM ABI — **zero JavaScript anywhere**, none of the
per-call / string-marshaling / wrapper-object overhead of the JS DOM. The
rendering engine (Blitz: Stylo + Taffy + Vello, no JS) is a commodity behind a
seam; **the ABI is the product** (`docs/ABI.md`).

## Architecture (v2 — single process)

```
app.wasm (Go / Rust / C / ...)          renderer.exe (Rust)
  gwb.submit(batch) ────────────────▶   wasmtime ─▶ GWB decoder ─▶ blitz-dom
  gwb_events(records) ◀────────────     hit-test/IME/keyboard events
                                        Stylo style ─▶ Taffy layout ─▶ Vello paint
```

One process, no pipes: guest→DOM is a function call. Writes are batched binary
ops (16-byte records + string heap, names as u32 atoms); everything host→guest
is an event record; reads are observations, never sync interrogation.

## Benchmarks (docs/BENCHMARKS.md, 2026-07-06, Snapdragon X2)

Identical DOM workloads: C guest on the GWB ABI vs **vanilla JS** in Chromium.

- **The boundary:** 55,002 ops encode + cross + decode = 1.6 ms (**~29 ns/op**,
  ~6× cheaper per op than the JS binding layer; 1 crossing vs 45,000 calls).
- **Interaction latency:** full click→DOM-applied round trip = **25 µs** —
  below what Chromium's own coarsened clock can measure.
- **Frame-realistic (mutations + layout):** GWB wins every workload by
  **64–93%** (e.g. create-5k: 14 ms vs 63 ms).
- **Mutation-only** (layout excluded — the JS engine's home game): GWB wins
  update-heavy workloads by 27–32%; vanilla JS still leads bulk create/clear
  by 12–33% (Blitz's per-node construction; three of our perf patches already
  live on the local `gwb-perf` engine branch, more upstreamable).
- The JS column is *vanilla* DOM — the floor no framework reaches. **React
  (vdom diffing on top of vanilla) loses every category to GWB outright.**

## Layout

- `docs/` — **ABI.md** (the wire contract), SDK.md (two-tier design),
  DEVX-LANGUAGES.md (Go/Rust/C, humans vs agents), BENCHMARKS.md.
- `renderer/` — the browser: Blitz window + wasmtime host + GWB ABI +
  in-window console + system/crash logging + `--script` test driver
  (click/type/dump golden testing without screenshots).
- `sdk/gwb` (Go) · `sdk-rust/` · `sdk-c/gwb.h` — low-level bindings
  (~200 lines each; byte-identical wire traffic, proven).
- `sdk-c/gwbc.h` — **GoWebComponents shorthand for C**: components, hooks,
  context, keyed lists, utility classes with hover — React-like authoring in
  freestanding C, no build step beyond clang.
- `examples/` — `hello` (console), `click` (events/anim), `todo-go|rs|c`
  (tri-language DevX proof), `starter-c` + `task-dashboard-c` (component
  model), `bench-c` (the benchmark guest).
- Legacy Go spine (`protocol/`, `engine/`, `host/`, `tab/`, `window/`,
  `session/`, `cmd/`) — the original wazero reference host of ABI v0; kept as
  spec + tests, superseded at runtime by the renderer.

## Build

```powershell
# Renderer — NOTE: builds against path-deps on C:\src\blitz, branch gwb-perf
# (local Blitz checkout carrying our performance patches).
cargo build --release --manifest-path renderer/Cargo.toml

# Guests
scripts\build-c.cmd examples\starter-c\starter.c renderer\starter-c.wasm   # C
cargo build --release --target wasm32-wasip1 --manifest-path examples/todo-rs/Cargo.toml
$env:GOOS="wasip1"; $env:GOARCH="wasm"; go build -buildmode=c-shared -o renderer\todo-go.wasm ./examples/todo-go

# Run
cd renderer; .\target\release\renderer.exe starter-c.wasm
# Headless-ish e2e: .\target\release\renderer.exe starter-c.wasm --script starter-test.txt
```

## Status

- [x] GWB ABI v1: full DOM op set, full event surface, preventDefault,
  observations, request_frame (vsync-paced), capability-minimal WASI
- [x] Three language bindings, identical wire traffic (Go 2.7 MB / Rust 98 KB / C 16 KB)
- [x] C component framework (gwbc.h): state/context/events/keyed lists/hover
- [x] Scripted driver + DOM dumps; system log + crash black-box; golden tests
- [x] M5 benchmarks vs Chromium + two optimization rounds (engine patches on gwb-perf)
- [ ] Keyed reconciliation (mapKeyed is API-ready), caret preservation beyond
  end-of-text, wasmtime module cache, workers, gwb_net capability module
