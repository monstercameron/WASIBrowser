# GoWebBrowser

A Go-first, **zero-JavaScript** browser runtime. Apps are Go→WASM; the engine is a
**forked WinCairo WebKit2** whose web process runs a native DOM agent that applies
binary DOM patches by calling WebCore directly. No `syscall/js`, no JS transport.

Engine: **WebKit2 / WinCairo** (open source). Built x64 first (runs emulated on the
X2) to validate the architecture; native Windows-ARM64 port is a later phase.

## Pipeline

```
Go app  --GOOS=wasip1-->  app.wasm
   |  (//go:wasmimport submit_batch)
   v
Go broker  --GDOM binary batch (protocol/)-->  WebKit2 web-process DOM agent (webkit/)
                                                  decodes -> WebCore Document/Node/Element
                                                  events <-- back to broker -> wasm
```

## Layout

- `protocol/` — GDOM binary DOM-patch wire format (encoder + decoder/Visitor). **Tested.**
- `engine/` — `engine.Engine` seam + `TestDOM` pure-Go reference engine. **Tested.**
- `host/` — wazero WASI loader: `gobrowser_dom` import, mount, event pump, `Capabilities` sandbox. **Tested.**
- `sdk/dom`, `sdk/app`, `eventmsg`, `examples/counter` — wasm-side SDK + counter app. **Tested.**
- `webkitengine/` — `engine.Engine` over WebKit2 IPC; loopback transport tested, process transport stubbed until WinCairo is built. **Tested.**
- `tab/`, `window/`, `session/` — browser shell: tab lifecycle, window manager, crash supervisor. **Tested.**
- `webkit/` — C++ WebKit2 injected-bundle DOM agent (`dom_agent.cpp`, `bundle.cpp`, `form_values.cpp`). Write-only until the WebKit tree exists.
- `cmd/run/` — integrated end-to-end demo. `cmd/broker/` — shell/lifecycle demo.
- `build/build-wincairo.ps1` — builds WinCairo WebKit (Release/x64). `plan.md` — full design spec.

## Status

- [x] GDOM wire protocol + engine seam + TestDOM (tested)
- [x] wazero WASI host + SDK + counter app — **no-JS loop works end-to-end** (`go run ./cmd/run` → Count 0→1→2→3)
- [x] Browser shell: tab lifecycle, window manager, crash supervisor (tested)
- [x] WebKit2 engine adapter (Go) + IPC framing + loopback transport (tested)
- [x] C++ DOM agent + bundle wiring (written; uncompiled — needs WebKit tree)
- [ ] Build WinCairo locally; compile the C++ bundle against it
- [ ] Wire `processTransport` to MiniBrowser+bundle over named pipe
- [ ] Swap `engine.TestDOM` → `webkitengine.Engine` in `cmd/run` (same seam)
- [ ] Windows-ARM64 port of WinCairo (deferred)

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

Build the engine (later; hours, needs VS2022 + C++ workload):
```powershell
.\build\build-wincairo.ps1
```
