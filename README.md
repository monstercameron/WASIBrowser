# GoWebBrowser

A **wasm-first** browser platform. Any language compiles to wasm and drives the page
through a fast binary DOM ABI — **zero JavaScript anywhere**, none of the per-call /
string-marshaling / wrapper-object overhead of the JS DOM. Go is the first guest
language (`GOOS=wasip1`, no `syscall/js`), not the only one: the ABI is a
language-neutral spec and `sdk/` is just its first binding.

Rendering engine: **Blitz** (DioxusLabs — blitz-dom + Stylo style + Taffy layout +
Vello/wgpu paint, no JS), run **out-of-process** behind the frozen `engine.Engine`
seam. The engine is a yoinked commodity; **the ABI is the product.** See
`plan-blitz.md` for the pinned plan and ABI laws (batched writes, snapshot reads,
language-neutral spec).

## Pipeline

```
Go app  --GOOS=wasip1-->  app.wasm
   |  (//go:wasmimport submit_batch)
   v
Go broker  --GDOM binary frames (protocol/) over stdio/pipe-->  renderer.exe (Rust)
                                                                  decode -> blitz-dom DocumentMutator
                                                                  Stylo style -> Taffy layout -> Vello paint
                                                                  winit events <- hit-test -> broker -> wasm
```

## Layout

- `protocol/` — GDOM binary DOM-patch wire format (encoder + decoder/Visitor). Frozen seam. **Tested.**
- `engine/` — `engine.Engine` seam + `TestDOM` pure-Go reference engine. Frozen seam. **Tested.**
- `host/` — wazero WASI loader: `gobrowser_dom` import, mount, event pump, `Capabilities` sandbox. **Tested.**
- `sdk/dom`, `sdk/app`, `eventmsg`, `examples/counter` — wasm-side SDK + counter app. **Tested.**
- `webkitengine/` — engine adapter over IPC (GWBI framing, loopback transport). **Tested.**
  Kept as the framing donor for `blitzengine/`; renamed/reworked in milestone 3.
- `tab/`, `window/`, `session/` — browser shell: tab lifecycle, window manager, crash supervisor. **Tested.**
- `cmd/run/` — integrated end-to-end demo. `cmd/broker/` — shell/lifecycle demo.
- `renderer/` — (milestone 1) Rust crate: Blitz window + GDOM decoder → `DocumentMutator`.
- `plan-blitz.md` — pinned plan, mission, ABI laws, milestones.

## Status

- [x] GDOM wire protocol + engine seam + TestDOM (tested)
- [x] wazero WASI host + SDK + counter app — **no-JS loop works end-to-end** (`go run ./cmd/run` → Count 0→1→2→3)
- [x] Browser shell: tab lifecycle, window manager, crash supervisor (tested)
- [x] IPC framing + loopback transport (tested)
- [ ] M1: `renderer/` crate — Blitz window renders hardcoded HTML (pin a Blitz rev)
- [ ] M2: Rust GDOM decoder → `DocumentMutator`, fed over stdin frames
- [ ] M3: `blitzengine/` implements `engine.Engine`; swap into `cmd/run`
- [ ] M4: events back (hit-test → eventmsg) — counter increments on a real mouse click
- [ ] M5: benchmark — same DOM-heavy workload, Go-wasm-GDOM vs JS-DOM-in-Chrome

## Quick start

```powershell
go test ./...            # all packages green
go run ./cmd/run         # integrated no-JS demo: counter increments through the full stack
go run ./cmd/broker      # shell/lifecycle + crash-supervision demo
```

Build the counter app to wasm:
```powershell
$env:GOOS="wasip1"; $env:GOARCH="wasm"
go build -buildmode=c-shared -o cmd/run/app.wasm ./examples/counter
```

Renderer (milestone 1; needs rustup + VS Build Tools C++ workload):
```powershell
cargo build --release --manifest-path renderer/Cargo.toml
```
