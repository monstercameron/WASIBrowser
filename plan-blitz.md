# GoWebBrowser — Blitz renderer plan (PINNED 2026-07-05)

## Mission (clarified 2026-07-05)

A **wasm-first browser/platform**: any language compiles to wasm and drives the page through a
fast binary DOM ABI — no JS anywhere, none of the per-call/string-marshaling/wrapper-object
overhead of the JS DOM ("the true Java ideal they never achieved"). Go is the first guest
language, not the only one. **The rendering engine is a yoinked commodity behind the frozen
`engine.Engine` seam; the ABI is the product.** Blitz is selected because it is the only
existing engine with a native no-JS DOM-mutation API (DocumentMutator) — not out of engine
preference. Engine fidelity work is deprioritized: apps target a documented CSS subset;
coverage arrives upstream.

**ABI laws:** (1) writes are batched binary ops, one boundary crossing per frame;
(2) **reads are answered from a per-frame layout snapshot pushed to the guest — NEVER a sync
round-trip per call** (the process hop makes sync reads slower than JS; snapshot reads make
them faster); (3) the ABI is a language-neutral spec (wasip1 + `submit_batch` + event pump) —
`sdk/` is just the first binding.

Engine pivot: the WinCairo-WebKit2 fork path (plan.md) is superseded. The renderer is
**Blitz** (DioxusLabs — blitz-dom + Stylo style + Taffy layout + Vello/WGPU paint, NO JS),
run **out-of-process** and driven by our existing GDOM protocol. Cam chose this 2026-07-05
(reverses the 2026-06-27 WebKit decision; the WinCairo build env stays on disk as fallback).

## Shape

```
Go app.wasm → wazero host → broker → [stdio/pipe, GDOM frames] → renderer.exe (Rust)
                                                                   ├ decode GDOM → blitz-dom DocumentMutator
                                                                   ├ Stylo style → Taffy layout → Vello/WGPU paint
                                                                   └ winit events → hit-test → frames back → broker → wasm
```

Why out-of-process, not cgo:
- No cross-toolchain linking; cargo/MSVC and go build stay independent.
- winit must own the Rust process's main thread — clean in its own process, painful under cgo.
- One boundary crossing per GDOM **batch**, not per op — the wire format was built for this.
- Reuses the exact process-transport shape `webkitengine/` stubbed (GWBI framing, spawn, supervise).

## What survives untouched (all tested)

`protocol/` (GDOM), `host/` (wazero), `sdk/dom` + `sdk/app` + `eventmsg`, `examples/counter`,
`engine/` seam + TestDOM, `tab/`/`window/`/`session/` shell. New code = one Go package
(`blitzengine/`, mostly lifted from `webkitengine` framing) + one Rust crate (`renderer/`).

## Milestones (de-risking order)

1. `cargo new renderer` — Blitz window rendering a hardcoded HTML string. Proves
   Blitz+Stylo+wgpu build & paint on this machine. **Start by cloning Blitz and reading
   `examples/` — API knowledge is stale; pin a git rev.**
2. Stdin frame reader + Rust GDOM decoder (port `protocol/` op set) → `DocumentMutator`.
   Test with a throwaway Go feeder piping raw batches.
3. `blitzengine/` implementing `engine.Engine`; swap into `cmd/run` — counter renders in a
   real window through the full no-JS stack.
4. Events back: Blitz hit-testing → stdout frames → existing eventmsg path — counter
   increments on a real mouse click. That's the demo.
5. **The benchmark** (the pitch artifact): same DOM-heavy workload as Go-wasm-GDOM vs
   JS-DOM-in-Chrome. The write-path speedup number is the entire thesis — measure it.

## Plugged plan-holes (carried from research)

events (winit→hit-test→EventDriver) · measure (read Taffy layout) · a11y (AccessKit) ·
networking stays in the Go broker under the capability model · host HTML declares the app
(`<link rel="gobrowser:wasm-main">`, parsed by blitz-html/html5ever) · node-id ownership:
guest-allocated u64 → host map · Win ARM64: `aarch64-pc-windows-msvc` + wgpu/Vulkan
(Adreno X2-90 Vulkan already proven in dynamicfighters).

## Risks

- Blitz is pre-1.0/pre-alpha: form controls, IME, CSS coverage immature; expect upstream
  patches. Pin a rev.
- `DocumentMutator` may not cover every GDOM op (node moves are the usual gap).
- Two toolchains (Rust + Go). First Stylo build is long but is pure `cargo build`.

## Toolchain check before milestone 1

rustup (`x86_64-pc-windows-msvc` first, emulated on the X2 like the WebKit plan assumed —
or go straight `aarch64-pc-windows-msvc`) + VS Build Tools C++ workload (already installed
for WinCairo).
