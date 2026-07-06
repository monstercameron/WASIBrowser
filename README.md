# GoWebBrowser

**A wasm-first browser platform — research repo.**

Any language compiles to WebAssembly and drives the page through a fast binary
DOM ABI. **Zero JavaScript anywhere** — no script engine, no JS DOM bindings,
none of the per-call / string-marshaling / wrapper-object overhead that every
JS-hosted framework ultimately pays. This is the "true Java ideal" the applet
era never reached: the browser as a language-neutral runtime where *your*
language is a first-class citizen of the page.

The rendering engine (Blitz: Stylo styling + Taffy layout + Vello GPU paint,
no JS) is deliberately a **commodity behind a seam**. The research artifact is
the **ABI** (`docs/ABI.md`) and the layers communities can build on top of it.

---

## Architecture (v2 — single process)

One process, no pipes, no IPC: a guest→DOM write is a function call into the
renderer's address space. Writes travel as batched binary ops; everything
host→guest is a fixed-size event record; reads are frame-accurate
*observations*, never synchronous interrogation.

```mermaid
flowchart TB
    subgraph GUEST["app.wasm — any language"]
        direction TB
        APP["application code"]
        subgraph SDKS["language bindings (~200 lines each)"]
            GWBC["gwbc.h — 'reactified C'<br/>components · hooks · reconciler"]
            TW["gwbc-tw.h<br/>typed Tailwind plugin"]
            GO["sdk/gwb (Go)"]
            RS["sdk-rust"]
            CH["sdk-c/gwb.h (C, freestanding)"]
        end
        APP --> GWBC
        TW -.plugs into.-> GWBC
        GWBC --> CH
        APP -.or directly.-> GO & RS & CH
    end

    subgraph ABI["GWB ABI v1 — the product (docs/ABI.md)"]
        direction TB
        SUBMIT["gwb.submit(batch)<br/>16-byte op records + string heap<br/>names = u32 atoms"]
        EVENTS["gwb_events(count)<br/>40-byte records in a guest-registered<br/>event region"]
        IMPORTS["imports: log · fetch ·<br/>request_frame · event_region"]
        EXPORTS["exports: gwb_start ·<br/>gwb_events · gwb_frame"]
    end

    subgraph HOST["renderer.exe — Rust host"]
        direction TB
        WT["wasmtime 46<br/>(in-process, WASI-minimal)"]
        DEC["GWB decoder<br/>atom table (flat Vec) · id maps"]
        SHELL["ShellEventHandler<br/>chrome first, then guest routing"]
        CHROME["window chrome<br/>toolbar · drag-resizable console"]
        NET["fetch thread (ureq)<br/>NET_RESULT events"]
        SCRIPT["--script driver<br/>click/type/hover/key/wheel/dump"]
        LOGS["system log · crash black-box"]
        subgraph ENGINE["Blitz (vendored, gwb-perf branch)"]
            DOM["blitz-dom<br/>DocumentMutator"]
            STYLO["Stylo (Firefox CSS)"]
            TAFFY["Taffy (flex/grid)"]
            VELLO["Vello (GPU paint)"]
            DOM --> STYLO --> TAFFY --> VELLO
        end
        WINIT["winit event loop<br/>vsync-paced frame pump"]
    end

    CH -- "one crossing per frame" --> SUBMIT
    SUBMIT --> DEC --> DOM
    WINIT -- "pointer / key / IME" --> SHELL
    SHELL -- "hit-tested, bubbled" --> EVENTS
    EVENTS --> APP
    NET -- "async completions" --> EVENTS
    SCRIPT -- "synthetic DomEvents" --> SHELL
    WT --- CH
    VELLO --> WINIT
```

### Why this shape

- **In-process wasm** (wasmtime lives inside the renderer): the write path is
  `memcpy + decode`, measured at **~29 ns/op** — about 6× cheaper per
  operation than Chromium's JS↔DOM binding layer, and it's *one* boundary
  crossing per batch instead of one per call.
- **The engine is swappable.** Everything engine-specific sits behind the
  decoder's `DocumentMutator` calls. Blitz was *yoinked*, not built; if a
  better no-JS engine appears, the ABI (and every SDK above it) survives.
- **The Go spine** (`protocol/`, `engine/`, `host/` — wazero) predates v2 and
  is kept as the reference host + spec tests for the wire format.

---

## The API interconnect (GWB ABI v1)

The contract is small enough to hold in your head, and that is the point —
it's what makes new language bindings a weekend project, not a platform team.

**Writes — `gwb.submit(ptr, len)`.** A batch is a `"GWB1"` header + N fixed
16-byte op records + a string heap. The op set covers the whole DOM surface:
create element/text, set/remove attr & style, text, append/insert/replace/
remove, inner-html, `Listen`/`Unlisten`, `Observe`, `Focus`, `DefineAtom`.
Ops are validated and atom-resolved **during the call**; the buffer is
reusable on return. The ABI law: batches submitted while an event is being
dispatched are applied **after** the dispatch completes (mutating mid-dispatch
invalidates the engine's node chains — learned the hard way).

**Atoms — the string killer.** Tag/attr/style names travel as `u32` atoms.
0–1023 are well-known (elements 1–88 cover *every* renderable HTML tag; attrs
from 100; style props from 200); 1024+ are guest-defined once via `DefineAtom`
and reused forever. Common DOM traffic carries **zero string bytes**; values
are inline UTF-8 in the heap.

**Node identity.** Guest-allocated dense `u32` ids; id 1 is the mount root —
the only host node a guest can address. The host keeps a flat
`guest id → engine node` table. Guests own `#mount` and nothing else; there is
no `document`, no global scripting surface.

**Events — `gwb_events(count)`.** The host writes 40-byte records (kind,
flags, target, listener, timestamp, 16-byte kind-specific payload, optional
trailing string) into a buffer the guest registered once. The full browser
interaction surface is forwarded: pointer down/up/move/cancel, click,
dblclick, contextmenu, enter/leave (hover), wheel, key down/up, text input,
input, focus/blur, scroll — plus window-level `window_resize`,
`theme_change`, and **`page_load`** (delivered exactly once, right after the
initial batches apply — the "document loaded" moment). `PREVENTABLE` events
honor guest return flags for `preventDefault`/`stopPropagation`.

**Reads — observations, never round-trips.** `Observe(id)` subscribes to
frame-accurate layout rects delivered as events. A sync read across a process
or even a call boundary is a latency lie; the ABI simply doesn't have one.

**Async — the host owns the event loop.** `gwb.fetch(url)` returns a request
id; completion arrives later as a `NET_RESULT` event (status + body). A
freestanding wasm guest has no sockets and no threads — and doesn't need them.

**Frames.** `request_frame` → one `gwb_frame(dt)` callback, paced by the
vsync redraw stream (a naive poll-loop self-feed spins at 0 ms; don't).

---

## "Reactified C" — the community-language thesis

The research question: *if the DOM boundary is cheap and language-neutral,
can any language community build its own first-class web idiom on top?* We
answered it with the least ergonomic language we had: **freestanding C**
(no libc, 16 KB of low-level binding).

`sdk-c/gwbc.h` — one header, no build step beyond clang — turns the C
preprocessor into a component model:

```c
#include "gwbc.h"
#include "gwbc-tw.h"   /* optional plugin: typed Tailwind + Preflight */

typedef struct { const char *title; } CounterProps;

component(Counter, props, CounterProps) {
    stateI32(count, 0);

    event(increment) {
        logf("[counter] %d -> %d", count, count + 1);
        set(count, count + 1);
    }
    eventKey(keys, k) {                      /* typed browser-event payloads */
        if (strEq(k.key, "Enter")) set(count, 0);
    }

    return main(
        class(U(twP(6), TwFlex, TwFlexCol, twGap(3))),
        h1(class(U(twTextSize(TwText2xl), TwFontBold)), props.title),
        p(text("Count: %d", count)),
        button(
            onClick(increment), onKeyDown(keys),
            class(U(twBg(TwSlate, 900), TwTextWhite, twPx(4), twPy(2),
                    twRounded(TwRoundedXl), TwTransition, Hover(BgSlate700))),
            "Increment"
        )
    );
}

app(Counter, { .title = "Hello from C" });
```

What that header actually delivers (all verified by scripted golden tests):

- **Components & props** — plain functions + compound literals; children as
  props; context (`context`/`provider`/`useContext`) without prop drilling.
- **Hooks** — `stateI32/Str/Bool/Enum/Struct`, `previousI32`, `useEffect`
  (after-commit, keyed, deps fingerprints, cleanups), `useQuery` with
  **stale-while-revalidate** over `gwb.fetch`, `memoI32/Str`, atoms
  (`useAtom`/`setAtom` shared state), `keyedId` refs.
- **An identity reconciler** — `keyed()` / auto-keyed `mapKeyed` rows reuse
  host nodes across full re-renders, so a focused `<input>` keeps its caret
  through unrelated updates. No fiber, no diffing — identity only, and it's
  enough.
- **The full event surface** — `onClick/onDblClick/onContextMenu/onKeyDown/
  onFocus/onWheel/onHover(...)/onScroll` plus window-level
  `onLoad/onWindowResize/onThemeChange`, with typed payload views
  (`eventKey`, `eventPointer`, `eventWheel`, `eventResize`).
- **Every renderable HTML tag** (88 of them, lowercase like JSX host
  elements) and **utility-class styling** — `U(...)` tokens compile to
  deduplicated classes in one generated `<style>` element, `Hover()` variants
  included. `gwbc-tw.h` adds a typed Tailwind layer: the full 22-hue × 11-shade
  palette, spacing/typography/shadow scales as enums and functions, and a
  zero-specificity Preflight.

The flagship demo, `examples/task-dashboard-c` (~66 KB of wasm), is a
10-component dashboard: semantic HTML (`dl`/`dt`/`dd`, `form`/`label`/
`output`, `nav`, `mark`, `kbd`), keyed filtered lists, payload-bound handlers,
remote data with refetch, hover highlighting, Enter-to-submit, wheel-adjusted
priority, and a live debug panel — in C, with business logic in a separate
pure-C translation unit.

**The point is not that people should write webpages in C.** The point is
that if *C* can have a pleasant, React-shaped authoring layer in one header,
then every language community — OCaml, Zig, Swift, Kotlin, Lua — can build
their own native idiom directly on the ABI, with their own type system doing
the safety work, and pay no JS tax on the way to the screen. The Go SDK
(`sdk/gwb`) and Rust SDK (`sdk-rust`) prove the polyglot claim at the wire
level: all three bindings emit **byte-identical** traffic for the same UI
(the tri-language todo in `examples/todo-go|rs|c` — Go 2.7 MB, Rust 98 KB,
C 16 KB).

---

## The renderer as a tool

The host is a real (small) browser shell, built for research iteration:

- **Window chrome** — a toolbar with view options (clear console, hide/show
  console) and a drag-resizable, fixed-height console panel that
  overflow-scrolls (it never grows with message count). Host chrome events are
  intercepted ahead of guest routing by the same dispatch pipeline.
- **Scripted driver** — `renderer.exe app.wasm --script steps.txt` with
  `click/type/focus/hover/unhover/rclick/key/wheel/dump/quit`. `dump` writes
  the mount subtree as pseudo-HTML for golden-file assertions: **all DSL and
  event verification is text-based, no screenshots needed.**
- **Observability** — every guest `log` line lands in the in-window console
  *and* a live-tailable `logs/system.log` (timestamped, flushed per line);
  panics produce a crash report with backtrace + a 400-line black-box ring.
- **Instrumentation** — decode/apply/guest-call microsecond timings on every
  event and batch, parsed straight out of the system log by the benchmark
  scripts.

---

## Benchmarks (`docs/BENCHMARKS.md`, Snapdragon X2)

Identical DOM workloads: C guest on the GWB ABI vs **vanilla JS** in Chromium.

- **The boundary:** 55,002 ops encode + cross + decode = 1.6 ms (**~29 ns/op**,
  ~6× cheaper per op than the JS binding layer; 1 crossing vs 45,000 calls).
- **Interaction latency:** full click→DOM-applied round trip = **25 µs** —
  below what Chromium's own coarsened clock can measure.
- **Frame-realistic (mutations + layout):** GWB wins every workload by
  **64–93%** (e.g. create-5k: 14 ms vs 63 ms).
- **Mutation-only** (layout excluded — the JS engine's home game): GWB wins
  update-heavy workloads by 27–32%; vanilla JS still leads bulk create/clear
  by 12–33% (Blitz's per-node construction; three perf patches already live on
  the local `gwb-perf` engine branch, more upstreamable).
- The JS column is *vanilla* DOM — the floor no framework reaches. **React
  (vdom diffing on top of vanilla) loses every category to GWB outright.**

---

## Layout

- `docs/` — **ABI.md** (the wire contract — start here), SDK.md (two-tier
  design), DEVX-LANGUAGES.md (Go/Rust/C authoring compared, for humans and
  agents), BENCHMARKS.md.
- `renderer/` — the browser: Blitz window + wasmtime host + GWB ABI + window
  chrome + console + logging + `--script` test driver. Golden test scripts
  (`*-test.txt`) live here too.
- `sdk-c/gwb.h` — low-level C binding (freestanding, `-fno-builtin`).
- `sdk-c/gwbc.h` — the reactified-C component layer; `sdk-c/gwbc-tw.h` — the
  typed Tailwind plugin.
- `sdk/gwb` (Go) · `sdk-rust/` — the other two low-level bindings.
- `examples/` — `hello`, `click` (raw ABI events/anim), `todo-go|rs|c`
  (tri-language wire-identity proof), `starter-c`, `task-dashboard-c`
  (the component-model flagship), `bench-c`.
- Legacy Go spine (`protocol/`, `engine/`, `host/`, `tab/`, `window/`,
  `session/`, `cmd/`) — the original wazero reference host of ABI v0; kept as
  spec + tests, superseded at runtime by the renderer.

## Build

```powershell
# Renderer — NOTE: builds against path-deps on C:\src\blitz, branch gwb-perf
# (local Blitz checkout carrying our performance + robustness patches).
cargo build --release --manifest-path renderer/Cargo.toml

# Guests
scripts\build-c.cmd examples\task-dashboard-c renderer\dashboard-c.wasm      # C
cargo build --release --target wasm32-wasip1 --manifest-path examples/todo-rs/Cargo.toml
$env:GOOS="wasip1"; $env:GOARCH="wasm"; go build -buildmode=c-shared -o renderer\todo-go.wasm ./examples/todo-go

# Run
cd renderer; .\target\release\renderer.exe dashboard-c.wasm
# Headless-ish e2e (golden dumps + log assertions):
#   .\target\release\renderer.exe dashboard-c.wasm --script dashboard-test.txt
```

## Status

- [x] GWB ABI v1: full DOM op set, **full browser event surface** (pointer/
  key/wheel/focus/scroll/context-menu + `page_load`/resize/theme), atoms for
  every renderable HTML tag, preventDefault, observations, vsync-paced frames
- [x] Three language bindings, byte-identical wire traffic
- [x] gwbc.h: components, hooks (state/effects/context/query-SWR/atoms/memo),
  keyed identity reconciliation with caret preservation, typed event payloads
- [x] gwbc-tw.h: typed Tailwind (palette/spacing/typography/shadows) + Preflight
- [x] Window chrome: toolbar view options, drag-resizable overflow-scrolling console
- [x] Scripted driver (click/type/hover/key/wheel/dump) + golden tests + crash black-box
- [x] Benchmarks vs Chromium + engine perf/robustness patches on `gwb-perf`
- [ ] Go/Rust SDK parity for fetch + the extended event surface
- [ ] Nested keyed scopes, stale-attr removal on reuse, portals/error boundaries
- [ ] wasmtime module cache, workers, capability-gated net module
