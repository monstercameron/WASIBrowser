Got it. Then the design changes materially:

```text id="bxb1d4"
No application JavaScript.
No hidden JS DOM trampoline as the primary path.
HTML declares a WASM app.
The browser host loads the WASM binary itself.
Go/WASM talks to a custom DOM syscall ABI.
The engine adapter applies DOM patches natively where possible.
```

The important correction: **`syscall/js` is not the ideal foundation for the no-JS version.** Go’s `syscall/js` package is explicitly based on JavaScript semantics and marked experimental/outside the Go compatibility promise. Use it only as an optional compatibility backend for normal browsers. For your custom browser, the cleaner path is **Go/WASI + `//go:wasmimport` host calls** or TinyGo/Component Model imports. Go 1.21 added the `wasip1/wasm` port and `//go:wasmimport`, which lets Go code call host-provided WASM functions directly. ([Go Packages][1])

So the real plan should be:

```text id="5u460y"
Custom browser mode:
  GOOS=wasip1 GOARCH=wasm
  host imports: gobrowser_dom_*
  no syscall/js
  no JS bridge

Normal web compatibility mode:
  GOOS=js GOARCH=wasm
  syscall/js backend
  optional, slower, not the primary runtime
```

---

# 1. Core architecture: Go-first, WASM-first, no JS-first

```text id="4a3eig"
┌──────────────────────────────────────────────────────────────┐
│ Go browser process                                           │
│                                                              │
│ - window/tab manager                                         │
│ - navigation controller                                      │
│ - profile/session manager                                    │
│ - process supervisor                                         │
│ - WASM app loader                                            │
│ - permission/capability broker                               │
│ - DOM patch scheduler                                        │
│ - crash recovery                                             │
└───────────────┬──────────────────────────────────────────────┘
                │
                │ per-tab IPC
                ▼
┌──────────────────────────────────────────────────────────────┐
│ Go WASM app process / worker                                 │
│                                                              │
│ - runs app.wasm                                              │
│ - wazero or Wasmtime backend                                 │
│ - imports gobrowser:dom, gobrowser:events, gobrowser:fetch   │
│ - emits binary DOM patch batches                             │
│ - receives compact event messages                            │
└───────────────┬──────────────────────────────────────────────┘
                │
                │ DOM patch protocol
                ▼
┌──────────────────────────────────────────────────────────────┐
│ Engine adapter                                                │
│                                                              │
│ Windows v0: WebView2 + CDP DOM domain                        │
│ Windows v1: CEF render-process native DOM agent              │
│ Linux future: WebKitGTK/WPE WebProcessExtension              │
│ macOS future: WKWebView + fallback adapter                   │
└───────────────┬──────────────────────────────────────────────┘
                │
                ▼
┌──────────────────────────────────────────────────────────────┐
│ Real web engine DOM                                           │
└──────────────────────────────────────────────────────────────┘
```

The runtime rule becomes:

```text id="gbfb5f"
WASM does not run "inside JavaScript."
WASM runs under your Go-controlled host.
DOM is an imported capability.
```

---

# 2. HTML directly links to WASM

Define your own browser semantic.

Example document:

```html id="g9b2m0"
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Go App</title>

  <link
    rel="gobrowser:wasm-main"
    href="/app/main.wasm"
    type="application/wasm"
    data-world="gobrowser:app/v1"
    data-root="#app">
</head>
<body>
  <go-root id="app"></go-root>
</body>
</html>
```

In your browser, this means:

```text id="o6rq0a"
1. Navigation starts.
2. Go host fetches HTML.
3. Go host tokenizes/scans HTML before or during load.
4. Host finds <link rel="gobrowser:wasm-main">.
5. Host loads /app/main.wasm.
6. Host instantiates WASM with gobrowser imports.
7. Web engine renders the static HTML.
8. WASM mounts into #app through DOM patch batches.
```

This is **not standard browser behavior**, but that is fine. You are defining a custom browser/runtime. The regular web fallback can still use normal WASM loading APIs if you ever care about Chrome/Firefox compatibility; browsers normally instantiate WASM through the WebAssembly JS API such as `WebAssembly.instantiateStreaming()`, which is exactly what you are refusing for your primary runtime. ([MDN Web Docs][2])

---

# 3. Do not make `syscall/js` the primary API

Use this split:

## Custom browser ABI

```go id="d0f5wn"
//go:build wasip1 && wasm

package dom

//go:wasmimport gobrowser_dom submit_batch
func submitBatch(ptr uint32, len uint32) uint32

//go:wasmimport gobrowser_events subscribe
func subscribe(node uint64, eventKind uint32, flags uint32) uint64

//go:wasmimport gobrowser_runtime now_millis
func nowMillis() uint64
```

Then the nice Go SDK wraps that:

```go id="au0oqd"
func Commit(batch *Batch) error {
	ptr, n := batch.Bytes()
	code := submitBatch(ptr, n)
	if code != 0 {
		return Error(code)
	}
	return nil
}
```

## Normal browser compatibility ABI

```go id="rqufl8"
//go:build js && wasm

package dom

import "syscall/js"

func Commit(batch *Batch) error {
	// Optional fallback only.
	js.Global().
		Get("__gobrowser_compat").
		Call("submitBatch", jsBytes(batch.Bytes()))
	return nil
}
```

That way your public API stays:

```go id="5nqh2w"
dom.Commit(batch)
```

not:

```go id="plmeym"
js.Global().Get("document").Call(...)
```

So the phrase becomes:

```text id="jdi66j"
syscall/js is a compatibility backend.
gobrowser_dom is the real ABI.
```

---

# 4. Windows backend reality

There are two different Windows paths.

## Path A: WebView2 now, using CDP DOM

This is the lower-dependency Windows-first path.

```text id="ax64c2"
Go browser process
  → WebView2
  → Chrome DevTools Protocol DOM domain
  → DOM mutations
```

No page JavaScript.

WebView2 follows the Chromium process model, including multiple renderer processes depending on site isolation and origins; Microsoft’s docs note that multiple WebView2 instances with different domains usually start new renderer processes, and that multiple user data folders create separate WebView2 process groups. ([GitHub][3])

The CDP DOM domain gives you native-ish DOM control: remove nodes, set attributes, set node values, set outerHTML, query selectors, and more. ([Chrome DevTools][4])

But the downside is brutal:

```text id="3o15mw"
CDP is not a high-throughput UI renderer API.
It is a browser automation/debugging protocol.
It is JSON-ish, async, and relatively coarse.
```

So WebView2 + CDP is good for:

```text id="jyvy1g"
v0 prototype
document loading
static app shell
coarse subtree replacement
event/navigation/profiling experiments
crash isolation experiments
```

It is not the final answer for “fast DOM API so WASM isn’t bottlenecked.”

## Path B: CEF later, with native render-process DOM agent

This is the better no-JS fast path.

```text id="pkgfap"
Go browser process
  → CEF browser process
  → CEF render process
  → native C++ DOM agent
  → batched DOM mutations
```

CEF is heavier, but it gives you the architectural shape you actually want. CEF has explicit browser/render process separation; render-process callbacks run on the render process main thread, and `CefFrame::VisitDOM()` is only callable from the render process. ([CEF Builds][5])

CEF’s DOM node API is also explicitly render-process-main-thread-only, which matches the design rule: many producers, one DOM writer. ([CEF Builds][6])

So the real recommendation is:

```text id="e3zqqt"
Prototype on WebView2 + CDP.
Design the engine interface as if CEF/WebKit native DOM backends are coming.
Move hot DOM mutation to CEF render-process agent when performance matters.
```

---

# 5. DOM API design: syscall-style, not object-style

Do **not** expose a fake DOM object model.

Bad:

```go id="ba6oaw"
el := dom.Document().CreateElement("div")
el.SetAttribute("class", "card")
root.AppendChild(el)
```

That encourages per-call interop.

Good:

```go id="ijwrm7"
b := dom.NewBatch()
card := b.CreateElement("div")
b.SetAttr(card, "class", "card")
b.Append(root, card)
dom.Commit(b)
```

Even better:

```go id="wi2w05"
ui.Div(
	ui.Class("card"),
	ui.H1(ui.Text("Hello")),
)
```

which lowers to:

```text id="xhl59e"
CREATE_ELEMENT node=42 tag="div"
SET_ATTR       node=42 name="class" value="card"
CREATE_ELEMENT node=43 tag="h1"
CREATE_TEXT    node=44 text="Hello"
APPEND         parent=43 child=44
APPEND         parent=42 child=43
APPEND         parent=root child=42
COMMIT
```

The WASM import is one hot call:

```text id="6v2rnj"
gobrowser_dom.submit_batch(ptr, len)
```

not thousands of calls.

---

# 6. Binary DOM syscall protocol

Use a compact binary format from day one after JSON debugging.

```text id="hygmu0"
BatchHeader
  magic          u32 = "GDOM"
  version        u16
  flags          u16
  document_id    u64
  frame_id       u64
  app_id         u64
  batch_id       u64
  base_revision  u64
  op_count       u32
  string_count   u32
  string_bytes   u32

StringTable
  offsets[]      u32
  utf8_blob[]    byte

Ops
  packed op records
```

Opcode set:

```go id="zr2uah"
const (
	OpCreateElement uint16 = iota + 1
	OpCreateElementNS
	OpCreateText
	OpSetText
	OpSetAttr
	OpRemoveAttr
	OpSetClass
	OpSetStyle
	OpAppend
	OpInsertBefore
	OpRemove
	OpReplace
	OpSetOuterHTML
	OpCloneTemplate
	OpReplaceChildren
	OpSubscribeEvent
	OpUnsubscribeEvent
	OpFocus
	OpBlur
	OpSetValue
	OpSetChecked
	OpRequestMeasure
	OpCommit
)
```

For WebView2/CDP, lower this to coarse CDP calls:

```text id="nkoilv"
OpSetText       → DOM.setNodeValue
OpSetAttr       → DOM.setAttributeValue
OpRemove        → DOM.removeNode
OpSetOuterHTML  → DOM.setOuterHTML
```

For CEF/WebKit native agents, lower it to direct DOM operations on the render-process DOM lane.

---

# 7. Fast path strategy

With no JS, your performance strategy is:

```text id="gkc4ew"
Do not make the host call the engine once per DOM op.
Do not make CDP call per node when using WebView2.
Use coarse native batches.
Use static templates.
Use subtree replacement aggressively.
Use native render-process agent for fine-grained hot paths.
```

## v0 WebView2/CDP fast-ish mode

For WebView2, use fewer, bigger DOM changes:

```text id="6bko8x"
initial render:
  Page.setDocumentContent or root.outerHTML replacement

large subtree change:
  DOM.setOuterHTML on a component root

small text update:
  DOM.setNodeValue

attribute change:
  DOM.setAttributeValue

remove:
  DOM.removeNode
```

Do not try to create 20,000 nodes one by one through CDP. That will be ugly.

## v1 CEF/WebKit fast mode

For CEF/WebKit native render-process agents:

```text id="ijyfsv"
batch enters render process
validate node ownership
decode string table
apply all writes on render main thread
flush once
return ack/revision
```

That is the real fast DOM API.

---

# 8. Process model for tabs

Build process isolation yourself above the engine.

```text id="eq9br8"
BrowserBroker.exe / gobrowser.exe
  - owns windows
  - owns profile manager
  - owns tab registry
  - owns permission broker
  - supervises children

TabHost.exe
  - one per tab or per site-instance
  - runs app WASM
  - owns app state
  - has no direct file/network power
  - talks to BrowserBroker through IPC

EngineRenderer
  - WebView2/Chromium renderer or CEF renderer
  - owns actual DOM/rendering
```

Policies:

```text id="8puwd6"
Normal mode:
  one TabHost per tab

Memory saver:
  one TabHost per same-origin app group

High security:
  one TabHost per site-instance

Background frozen:
  suspend WASM ticks
  keep rendered page
  persist state snapshot
```

Modern Chromium’s site isolation model aims to restrict renderer processes to one site as a security boundary; your browser should copy that idea at the **WASM app process** layer even if the embedded engine has its own process model. ([Chromium][7])

---

# 9. Crash isolation APIs

Expose crash isolation as a first-class Go API.

```go id="x05khh"
type ProcessKind string

const (
	ProcessBrowser ProcessKind = "browser"
	ProcessTab     ProcessKind = "tab"
	ProcessWASM    ProcessKind = "wasm"
	ProcessRender  ProcessKind = "render"
	ProcessGPU     ProcessKind = "gpu"
)

type CrashEvent struct {
	Kind        ProcessKind
	TabID       TabID
	ProcessID   int
	Reason      string
	ExitCode    int
	Recoverable bool
	LastURL      string
	LastApp      string
}

type CrashPolicy struct {
	RestartWASM       bool
	ReloadRenderer    bool
	RestoreDOMSnapshot bool
	MaxRestarts        int
	RestartWindow     time.Duration
}
```

Public browser strategy:

```go id="2c7t26"
browser.OnProcessCrash(func(ev CrashEvent) CrashPolicy {
	if ev.Kind == ProcessWASM {
		return CrashPolicy{
			RestartWASM:        true,
			RestoreDOMSnapshot: true,
			MaxRestarts:        3,
			RestartWindow:      30 * time.Second,
		}
	}

	if ev.Kind == ProcessRender {
		return CrashPolicy{
			ReloadRenderer:     true,
			RestoreDOMSnapshot: true,
			MaxRestarts:       2,
			RestartWindow:     30 * time.Second,
		}
	}

	return CrashPolicy{}
})
```

WebView2 gives you real hooks here: `ProcessFailed` reports unexpected or unresponsive WebView process failures, and `BrowserProcessExited` reports browser-process exit; Microsoft notes that a main browser-process crash produces both events and requires recreating affected controls. ([Microsoft Learn][8])

CEF has similar render-process hooks: `OnRenderProcessTerminated` and `OnRenderProcessUnresponsive` exist on the browser process UI thread. ([CEF Builds][9])

---

# 10. Tab lifecycle model

Each tab should have explicit lifecycle state.

```go id="m8czq3"
type TabState int

const (
	TabCreated TabState = iota
	TabLoading
	TabActive
	TabBackground
	TabFrozen
	TabDiscarded
	TabCrashed
	TabRestoring
	TabClosed
)
```

Lifecycle rules:

```text id="7ac8vw"
Active:
  WASM running
  event delivery live
  animation frame delivery live

Background:
  timers throttled
  network allowed by policy
  events queued/coalesced

Frozen:
  WASM suspended
  DOM remains visible or snapshot kept
  no timers
  no event delivery except visibility restore

Discarded:
  renderer released
  WASM state snapshot saved
  tab thumbnail retained

Crashed:
  show crash UI
  offer restart/reload
```

Go API:

```go id="v12w4h"
type Tab interface {
	ID() TabID
	URL() string
	State() TabState

	Freeze() error
	Resume() error
	Discard() error
	Reload() error
	KillWASM() error
	KillRenderer() error
	Snapshot() (*TabSnapshot, error)
	Restore(*TabSnapshot) error
}
```

---

# 11. WASM app ABI

Define the runtime ABI as imports/exports.

## Imports provided by the browser

```wit id="o1bsrt"
package gobrowser:runtime;

interface dom {
  submit-batch: func(ptr: u32, len: u32) -> u32;
  request-measure: func(ptr: u32, len: u32) -> u64;
}

interface events {
  subscribe: func(node: u64, kind: u32, flags: u32) -> u64;
  unsubscribe: func(event-id: u64);
  poll-event: func(ptr: u32, cap: u32) -> u32;
}

interface storage {
  get: func(key-ptr: u32, key-len: u32, out-ptr: u32, out-cap: u32) -> u32;
  set: func(key-ptr: u32, key-len: u32, val-ptr: u32, val-len: u32) -> u32;
}

interface fetch {
  request: func(req-ptr: u32, req-len: u32) -> u64;
  poll-response: func(handle: u64, out-ptr: u32, out-cap: u32) -> u32;
}
```

## Exports expected from app

```text id="2ykxss"
gobrowser_init()
gobrowser_mount(root_id u64)
gobrowser_handle_event(ptr u32, len u32)
gobrowser_tick(now_ms u64)
gobrowser_snapshot(ptr u32, cap u32) -> u32
gobrowser_restore(ptr u32, len u32) -> u32
gobrowser_shutdown(reason u32)
```

Long-term, model this with WIT/Component Model, but keep the hot DOM batch as raw bytes. The Component Model’s canonical ABI is meant to define consistent value passing across components, but the DOM patch stream should stay compact and custom. ([GitHub][10])

---

# 12. App code example

This is what writing apps should feel like:

```go id="zqm7di"
package main

import (
	"github.com/monstercameron/gobrowser/sdk/app"
	"github.com/monstercameron/gobrowser/sdk/ui"
)

type Counter struct {
	Count ui.State[int]
}

func (c *Counter) Render() ui.Node {
	return ui.Div(
		ui.Class("card"),
		ui.H1(ui.Text("Go-first WASM Browser")),
		ui.P(ui.Textf("Count: %d", c.Count.Get())),
		ui.Button(
			ui.OnClick(func(ctx ui.EventContext) {
				c.Count.Set(c.Count.Get() + 1)
			}),
			ui.Text("Increment"),
		),
	)
}

func main() {
	app.Main(&Counter{})
}
```

Build target:

```bash id="allst5"
GOOS=wasip1 GOARCH=wasm go build -o app.wasm ./cmd/app
```

Then HTML links it:

```html id="8osncs"
<link rel="gobrowser:wasm-main" href="/app.wasm" type="application/wasm" data-root="#app">
<go-root id="app"></go-root>
```

No authored JavaScript. No app JS runtime.

---

# 13. Event system without JS

Events are captured by the engine adapter, not JS.

## WebView2/CDP path

Use browser/engine-level event observation where available, and for app-controlled DOM, prefer controlled widgets:

```text id="8xd4ea"
native input focus
navigation events
permission events
document lifecycle
CDP input/page/runtime/dom events where practical
```

This is the weakest part of WebView2 for a no-JS app framework. DOM event capture is naturally a page-world feature. Without JS, WebView2 gives you fewer ergonomic hooks.

## CEF/WebKit path

Native render-process agent captures DOM events and converts them to compact messages:

```text id="zazomf"
click:
  event_kind = CLICK
  target_node = 9281
  button = LEFT
  modifiers = CTRL|SHIFT

input:
  event_kind = INPUT
  target_node = 518
  value = "abc"
  composing = false
```

Then:

```text id="wqgr4f"
render process DOM event
  → native DOM agent
  → binary EventMessage
  → Go BrowserBroker
  → TabHost WASM
  → app handler
  → PatchBatch
```

---

# 14. Security and permissions

Every WASM app gets a capability table.

```go id="gz8eic"
type CapabilitySet struct {
	DOM       DOMCapability
	Network   NetworkCapability
	Storage   StorageCapability
	Clipboard ClipboardCapability
	File      FileCapability
	Process   ProcessCapability
}

type DOMCapability struct {
	RootNode      NodeID
	AllowedRange  NodeRange
	AllowedEvents []EventKind
	CanMeasure    bool
	CanFocus      bool
}

type NetworkCapability struct {
	AllowedOrigins []Origin
	Methods        []string
	MaxBodyBytes    int64
}
```

WASM cannot:

```text id="vwbetl"
open arbitrary files
spawn processes
touch arbitrary DOM roots
inspect cookies from other origins
send native IPC directly
eval JS
access CDP
```

WASM can only call imported capabilities granted by the broker.

---

# 15. Multitab isolation

Use four levels.

## Level 0: single process dev mode

```text id="jsrny4"
gobrowser.exe
  window
  webview
  wasm runtime
```

Fast to debug. Not secure.

## Level 1: one WASM runtime per tab

```text id="xpgy6p"
gobrowser.exe
  tab 1 wasm instance
  tab 2 wasm instance
```

Better isolation, still same OS process.

## Level 2: one Go TabHost process per tab

```text id="60tw5m"
gobrowser.exe
tabhost.exe --tab=1
tabhost.exe --tab=2
```

This is the default serious mode.

## Level 3: one TabHost per site-instance

```text id="xwiy0a"
same origin + related opener group may share
cross-origin always separate
private/incognito always separate
untrusted app always separate
```

This mirrors modern browser strategy better.

---

# 16. IPC protocol

Use a single envelope for all internal messages.

```go id="5b27zy"
type MessageKind uint16

const (
	MsgDOMBatch MessageKind = iota + 1
	MsgDOMAck
	MsgEvent
	MsgNavigation
	MsgPermissionRequest
	MsgPermissionResponse
	MsgCrash
	MsgLifecycle
	MsgSnapshot
	MsgDevtools
)

type Envelope struct {
	Version   uint16
	Kind      MessageKind
	Flags     uint16
	TabID     uint64
	FrameID   uint64
	AppID     uint64
	Seq       uint64
	ReplyTo   uint64
	BodyLen   uint32
}
```

Transports:

```text id="cgqupz"
Windows:
  named pipes first
  shared memory ring buffer later

Linux:
  Unix domain sockets
  memfd/shared memory later

macOS:
  XPC or Unix domain sockets

in-process dev:
  Go channels
```

---

# 17. Shared memory fast path

For DOM batches:

```text id="lwn2x4"
TabHost WASM process
  writes PatchBatch into shared ring
  notifies BrowserBroker
  BrowserBroker forwards descriptor to engine adapter
  engine adapter applies batch
```

Ring buffer:

```text id="t7zb0z"
SharedRegion
  header
    write_seq atomic u64
    read_seq  atomic u64
    capacity  u64
  slots[]
    seq       u64
    kind      u16
    len       u32
    checksum  u32
    payload   bytes
```

Do not require shared memory for correctness. Require it for performance mode.

---

# 18. DOM snapshots and crash recovery

Keep app state and DOM state separately.

```text id="cx5cy8"
WASM state snapshot:
  produced by app export gobrowser_snapshot

DOM snapshot:
  browser-maintained last committed virtual tree
  plus latest real DOM revision

Renderer crash:
  recreate WebView
  reload static HTML
  restore DOM from virtual tree
  reconnect WASM app

WASM crash:
  keep DOM frozen
  restart WASM
  call gobrowser_restore
  diff restored tree against current DOM
```

Crash UI:

```text id="7vbs7v"
"This tab's app process crashed."
[Restart app] [Reload page] [Show diagnostics]
```

Diagnostics should include:

```text id="p6bylj"
last DOM batch id
last event id
WASM module hash
tab URL
origin
process id
exit code
panic text if available
```

---

# 19. Scheduler

One DOM writer per document/frame.

```go id="nm06lr"
type DOMScheduler struct {
	rev       uint64
	pending   []domproto.Batch
	events    []EventMessage
	transport EngineDOMTransport
}

func (s *DOMScheduler) Submit(batch domproto.Batch) error {
	if batch.BaseRevision != s.rev {
		return ErrRevisionMismatch
	}
	if err := ValidateOwnership(batch); err != nil {
		return err
	}
	s.pending = append(s.pending, batch)
	return nil
}

func (s *DOMScheduler) CommitFrame() error {
	merged := Coalesce(s.pending)
	s.pending = nil

	if err := s.transport.Apply(merged); err != nil {
		return err
	}

	s.rev++
	return nil
}
```

Coalesce:

```text id="1stfd0"
SET_TEXT same node → keep latest
SET_ATTR same name → keep latest
ADD_CLASS then REMOVE_CLASS → cancel if same frame
CREATE then REMOVE → drop whole subtree
multiple style writes → merge style record
```

---

# 20. Engine abstraction

Keep this strict:

```go id="kn9cmp"
type Engine interface {
	CreateTab(ctx context.Context, opts TabOptions) (EngineTab, error)
	Shutdown(ctx context.Context) error
}

type EngineTab interface {
	LoadHTML(ctx context.Context, html []byte, baseURL string) error
	ApplyDOMBatch(ctx context.Context, batch domproto.Batch) (DOMAck, error)
	QueryDOM(ctx context.Context, q DOMQuery) (DOMResult, error)
	CaptureSnapshot(ctx context.Context) (*Bitmap, error)

	OnEvent(func(EngineEvent))
	OnCrash(func(RenderCrash))
	Close(ctx context.Context) error
}
```

Implementations:

```text id="ymyktb"
engine/webview2cdp
engine/cefdom
engine/webkit2dom
engine/testdom
```

`testdom` is important: a pure-Go fake DOM for unit tests.

---

# 21. Best build order under your constraints

## Phase 1: no-JS WASM loader

* Parse `<link rel="gobrowser:wasm-main">`.
* Build `GOOS=wasip1 GOARCH=wasm`.
* Run module in wazero from Go.
* Provide fake `gobrowser_dom.submit_batch`.
* Render into pure-Go test DOM.

## Phase 2: WebView2/CDP prototype

* Create Win32 + WebView2 shell.
* Load static HTML.
* Use CDP DOM domain to find `#app`.
* Apply coarse `SetOuterHTML` batches.
* Handle crashes through WebView2 `ProcessFailed` / `BrowserProcessExited`.

## Phase 3: Go UI SDK

* `ui.Div`, `ui.Button`, `ui.State`.
* Render to virtual tree.
* Diff to DOM patches.
* Compile app to WASI WASM.

## Phase 4: event model

* Start with native browser events you can capture cleanly.
* For WebView2/CDP, accept limited event support.
* Build the event protocol anyway.

## Phase 5: process isolation

* Split `tabhost.exe`.
* One WASM app process per tab.
* Named-pipe IPC.
* Crash/restart/snapshot APIs.

## Phase 6: CEF native DOM backend

* Add CEF engine backend.
* Implement C++ render-process DOM agent.
* Receive binary batches.
* Apply on render process main thread.
* Send native DOM events back.
* This becomes the real fast no-JS renderer.

## Phase 7: Linux WebKit/WPE backend

* Implement WebKitGTK/WPE.
* Use WebProcessExtension for DOM agent.
* Same batch protocol.
* Same WASM app ABI.

---

# 22. The hard truth

With your new constraint, the architecture becomes cleaner but more ambitious:

```text id="hdh8fa"
No JS + WebView2 + fast fine-grained DOM
```

is not the strongest combination.

The honest matrix:

| Backend                  | No JS |    Fast DOM | Low dependency | Cross-platform future | Verdict           |
| ------------------------ | ----: | ----------: | -------------: | --------------------: | ----------------- |
| WebView2 + CDP           |   Yes |  Medium/low |           High |          Windows only | Good v0           |
| WebView2 + injected JS   |    No | Medium/high |           High |          Windows only | Rejected by you   |
| CEF render-process agent |   Yes |        High |     Low/medium |                  High | Best serious path |
| WebKitGTK/WPE extension  |   Yes |        High |         Medium |        Linux/embedded | Best Linux path   |
| Custom engine            |   Yes |     Unknown |       Terrible |              Terrible | Don’t             |

So I’d do this:

```text id="6xwhv8"
Windows now:
  WebView2 + CDP + coarse DOM patches

Serious runtime:
  CEF + native render-process DOM agent

Future Linux:
  WebKitGTK/WPE + WebProcessExtension

Always:
  Go browser broker
  Go WASM host
  WASI/go:wasmimport ABI
  binary DOM syscall protocol
  per-tab process isolation
```

---

# 23. Final architecture slogan

```text id="7dfkqa"
HTML declares the WASM.
Go loads the WASM.
WASM calls Go-owned host imports.
Go schedules DOM syscalls.
Native engine adapters mutate the DOM.
JavaScript is not part of the app model.
```

And the final brutal version:

```text id="w7f9ha"
Do not build a Go wrapper around JavaScript.
Build a Go browser runtime where DOM is a WASM syscall surface.
```

That is the version of this idea that is internally consistent.

[1]: https://pkg.go.dev/syscall/js "js package - syscall/js - Go Packages"
[2]: https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/instantiateStreaming_static?utm_source=chatgpt.com "WebAssembly.instantiateStreaming() - MDN Web Docs"
[3]: https://github.com/MicrosoftDocs/edge-developer/blob/main/microsoft-edge/webview2/concepts/process-model.md "edge-developer/microsoft-edge/webview2/concepts/process-model.md at main · MicrosoftDocs/edge-developer · GitHub"
[4]: https://chromedevtools.github.io/devtools-protocol/tot/DOM/ "Chrome DevTools Protocol - DOM domain"
[5]: https://cef-builds.spotifycdn.com/docs/112.3/classCefRenderProcessHandler.html "Chromium Embedded Framework (CEF): CefRenderProcessHandler Class Reference"
[6]: https://cef-builds.spotifycdn.com/docs/145.0/classCefDOMNode.html "Chromium Embedded Framework (CEF): CefDOMNode Class Reference"
[7]: https://www.chromium.org/developers/design-documents/site-isolation/?utm_source=chatgpt.com "Site Isolation Design Document"
[8]: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-related-events "Handling process-related events in WebView2 - Microsoft Edge Developer documentation | Microsoft Learn"
[9]: https://cef-builds.spotifycdn.com/docs/143.0/classCefRequestHandler.html "Chromium Embedded Framework (CEF): CefRequestHandler Class Reference"
[10]: https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md?utm_source=chatgpt.com "component-model/design/mvp/CanonicalABI.md at main"
