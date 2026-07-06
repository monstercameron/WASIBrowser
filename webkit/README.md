# webkit/ — WinCairo WebKit2 DOM agent bundle

This directory contains the C++ injected-bundle code that runs inside the
WebKit2 web process and applies GDOM batches to the live WebCore DOM. No
JavaScript or JavaScriptCore is involved.

## Files

| File | Purpose |
|------|---------|
| `dom_agent.cpp` | `gdom::Agent` — GDOM batch decoder; calls WebCore DOM methods directly |
| `bundle.cpp` | `WKBundleInitialize` — bundle entry point; wires page-load and IPC callbacks |
| `form_values.cpp` | `gdom::Agent::applyFormValue` — sets input/textarea/select values |

## Building against the WinCairo WebKit tree

**Prerequisites**

- WinCairo WebKit build at `$WEBKIT_BUILD` (e.g. `C:\webkit\WinCairo-Release`)
- MSVC 2022 (ARM64) or Clang-cl targeting `arm64-pc-windows-msvc`
- CMake ≥ 3.28

**CMake snippet** (add to your WebKit build overlay or a standalone bundle CMakeLists):

```cmake
add_library(GoBrowserBundle MODULE
    webkit/dom_agent.cpp    # included by bundle.cpp; list separately for IDEs
    webkit/bundle.cpp
    webkit/form_values.cpp
)

target_include_directories(GoBrowserBundle PRIVATE
    ${WEBKIT_BUILD}/include          # WKBundle*, WKData*, WKString*, …
    ${WEBKIT_BUILD}/include/WebCore  # Document, Element, HTMLInputElement, …
    ${WEBKIT_BUILD}/include/WTF      # HashMap, AtomString, …
)

target_link_libraries(GoBrowserBundle PRIVATE
    ${WEBKIT_BUILD}/lib/WebKit.lib
    ${WEBKIT_BUILD}/lib/WTF.lib
    ${WEBKIT_BUILD}/lib/JavaScriptCore.lib   # needed by WebCore internals
)

set_target_properties(GoBrowserBundle PROPERTIES
    OUTPUT_NAME "GoBrowserBundle"
    SUFFIX ".dll"
)
```

Build output: `GoBrowserBundle.dll`

## Loading the bundle

Pass the bundle path to the WinCairo MiniBrowser host executable when launching
it from the Go broker:

```
MiniBrowser.exe --injected-bundle-path=C:\path\to\GoBrowserBundle.dll --ipc-pipe=gdom-1
```

The `--ipc-pipe` argument names the Windows named pipe the Go broker connects to
(see `webkitengine/transport.go` → `processTransport`).

WebKit loads the bundle by calling `WKBundleInitialize(bundle, initData)` during
web-process startup, before any page loads.

## IPC contract

All communication between the Go broker (UI process) and this bundle (web
process) uses the "GWBI" WKBundlePage message channel. Messages are raw byte
arrays (`WKDataRef`) carrying IPC frames.

### Frame layout (all little-endian)

```
[4]byte  magic    = "GWBI"
uint16   kind                // MessageKind (see below)
uint16   flags               // reserved = 0
uint32   bodyLen
[bodyLen]byte body
```

### MessageKind table

| Value | Name | Direction | Body |
|-------|------|-----------|------|
| 1 | `KindDOMBatch` | Go → bundle | Raw GDOM batch bytes (see `protocol/`) |
| 2 | `KindDOMAck` | bundle → Go | `uint64` newRevision; **0 = NACK** (stale base revision or decode error) |
| 3 | `KindEvent` | bundle → Go | `uint32` kind, `uint64` nodeID, `uint32` valueLen, `[valueLen]byte` value (UTF-8) |
| 4 | `KindLoadHTML` | Go → bundle | UTF-8 HTML string (length = bodyLen) |
| 5 | `KindMount` | Go → bundle | UTF-8 CSS selector string |
| 6 | `KindMountAck` | bundle → Go | `uint64` nodeID; **0 = not found** |

### Event kind values (KindEvent body field `kind`)

| Value | Name |
|-------|------|
| 1 | Click |
| 2 | Input |
| 3 | Key |

These match `engine.EventClick / EventInput / EventKey` in `engine/engine.go`.

### Revision protocol

The agent maintains a monotonic revision counter that starts at 0 and
increments on each successful `OpCommit`. A `KindDOMBatch` is accepted only
if its `baseRevision` field matches the current revision; otherwise the agent
sends `KindDOMAck` with `newRevision = 0` (NACK) and the Go broker
retransmits after resyncing.

### Go → UI process flow

```
Go broker                          MiniBrowser (UI proc)        Web process (bundle)
   │                                      │                            │
   │── KindLoadHTML ──────────────────────►                            │
   │                           WKPageLoadURL / LoadHTML ───────────────►
   │                                                         didFinishDocumentLoad
   │                                                         gdom::Agent created
   │── KindMount ──────────────────────────────────────────────────────►
   │◄── KindMountAck (nodeID) ──────────────────────────────────────────
   │── KindDOMBatch ─────────────────────────────────────────────────── ►
   │◄── KindDOMAck (newRevision) ─────────────────────────────────────── 
   │                                                         DOM event fires
   │◄── KindEvent (kind, nodeID, value) ──────────────────────────────── 
```

## Why C++ is uncompiled here

This directory contains write-only C++ because:

1. No WinCairo WebKit tree is present in the repo (it is a multi-gigabyte
   external build).
2. The WebCore and WTF headers required to compile `dom_agent.cpp` and
   `bundle.cpp` do not exist without that tree.
3. The Go IPC layer (`webkitengine/`) is fully tested via `loopbackTransport`
   without any C++ compilation. The Go tests exercise the identical framing
   logic and verify end-to-end batch+event plumbing.

The C++ is kept consistent with the Go IPC contract (magic bytes, MessageKind
values, frame layout, event encoding) by construction; all constants appear in
both `webkitengine/ipc.go` and the `bundle.cpp` comment header and code.
