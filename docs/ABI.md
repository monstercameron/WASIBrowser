# GWB ABI v1 — wasm-first DOM interface (DRAFT 2, 2026-07-05)

The contract between a guest app (any language, compiled to wasm/wasip1) and the
GoWebBrowser renderer. Design goals, in priority order:

1. **Beat the JS DOM decisively on DOM-heavy workloads.** No per-call boundary tax,
   no string-heavy calls, no wrapper-object GC pressure, no accidental sync layout.
2. **Language-neutral.** Plain wasm32 + wasip1; nothing here assumes Go. `sdk/` (Go)
   is binding #1; a Rust or Zig binding should be a ~200-line afternoon.
3. **Impossible to use slowly.** The shape of the API forbids the classic slow paths
   (per-op crossings, sync forced layout) rather than documenting them as mistakes.

## Two tiers, one contract

- **This document is the LOW-LEVEL API (GWB/LL)** — the wire contract. Raw fixed-width
  op records, manual node ids, manual atoms, explicit subscriptions. Everything the
  host implements, and everything a perf-obsessed guest may use directly.
- **The HIGH-LEVEL API (GWB/HL, see `SDK.md`) is a guest-side library** — retained
  element handles, closures, automatic ids/atoms/arenas. It compiles down to LL
  batches with zero host involvement. The host cannot tell which tier produced a batch.

One contract at the boundary keeps the host simple and the fast path unpolluted;
ergonomics are a library problem, per language. Apps mix tiers freely (HL handles
expose their raw node id; hot paths drop to LL on the same nodes).

Everything host→guest is an **event record**; everything guest→host is a **batch op**.
Two channels, both binary, both batched, both zero-copy, both allocation-free in
steady state.

## Memory + threading model

- Host runs the guest inside wasmtime, in-process with the engine. The host
  reads/writes guest linear memory directly; no copies cross the boundary.
- Single-threaded guest. Guest code runs only inside its exports (`gwb_start`,
  `gwb_events`, `gwb_frame`).
- `submit` **decodes during the call**: ops are validated and atom-resolved into the
  host's apply queue while the guest's buffer is live; the buffer is reusable the
  moment `submit` returns. Structural application happens after the active guest
  call returns (re-entrancy rule), then style/layout/paint once per frame.
- Event records are written into the **guest-registered event region** (see Events);
  they are valid only during the `gwb_events` call.

## Latency contract

The pipeline is specified, not best-effort:

1. **Discrete input events dispatch immediately** — a `pointer_down` reaching the
   host is hit-tested and delivered to the guest on the same OS event cycle (µs),
   *not* queued to the next frame boundary.
2. Batches submitted during that dispatch are already decoded when the handler
   returns; the host applies, styles, layouts, and paints on the **same vsync**
   when the budget allows. Target: **input-to-photon ≤ 1 frame.**
3. **Continuous streams coalesce per frame** (`pointer_move`, `wheel`, `scroll`:
   latest record per target wins) — they describe state, not edges; delivering
   every intermediate sample adds work, not information.
4. **Steady state allocates nothing on either side.** Guest reuses its batch arena
   after every `submit`; host writes events into the pre-registered region; atom
   tables are flat arrays (`u32` index, no hashing on the hot path).
5. An idle page costs **zero crossings** — no polling, no ticks, no heartbeat.

## Handshake

1. Guest is a wasip1 **reactor** (`-buildmode=c-shared` in Go): host runs `_initialize`.
2. Host calls `gwb_abi_version() -> u32` (`major u16 << 16 | minor u16`). Major
   mismatch → refuse to mount, console error.
3. Host calls `gwb_start(viewport_w: f32, viewport_h: f32, scale: f32, flags: u32)`
   (`flags` bit0: dark theme). During `gwb_start` the guest MUST call
   `event_region` once, and typically defines atoms + submits its initial DOM.

## Guest imports (module `"gwb"`)

| import | signature | semantics |
|---|---|---|
| `submit` | `(ptr: u32, len: u32) -> u32` | Decode + queue one write batch. Returns 0, or the 1-based index of the first invalid op (dev-mode diagnostics via `log`). Buffer reusable on return. |
| `event_region` | `(ptr: u32, len: u32)` | Register the buffer event batches are written into. Min 4 KiB. Re-registration allowed; takes effect next delivery. |
| `log` | `(level: u32, ptr: u32, len: u32)` | Structured console line (0=debug 1=info 2=warn 3=error) → in-window console + system log. |
| `request_frame` | `()` | One-shot: schedule `gwb_frame` at the next paint. The rAF equivalent. |

## Guest exports

| export | signature | semantics |
|---|---|---|
| `_initialize` | `()` | wasip1 reactor init. |
| `gwb_abi_version` | `() -> u32` | ABI handshake. |
| `gwb_start` | `(f32, f32, f32, u32)` | Mount: register event region, build initial DOM. |
| `gwb_events` | `(count: u32) -> u32` | `count` records are in the event region. Return flags read only for a solo PREVENTABLE delivery: bit0 `prevent_default`, bit1 `stop_propagation`. |
| `gwb_frame` | `(dt_ms: f32)` | Animation tick; called only after `request_frame`. |

(No `gwb_alloc`: the registered event region replaces per-delivery allocation —
one crossing per delivery, not two.)

## Node IDs

- Guest-allocated `u32`, dense from 1. Host keeps a flat `guest_id → engine node` table.
- `id 1` is pre-defined: the **mount root** (host-provided container). The guest
  cannot address anything outside its mount subtree — host chrome (console, future
  tab UI) is physically unreachable. Isolation falls out of the id space.
- Ids are never reused until `Remove`d; SDKs may recycle after removal.

## Atoms — the string killer

Names (tags, attributes, style properties) travel as `u32` atoms:

- **0–1023: well-known atoms**, fixed in the spec appendix (`div`, `span`, `p`,
  `class`, `id`, `style`, `href`, `value`, `display`, `color`, …). Common DOM
  traffic carries **zero string bytes**.
- **1024+: dynamic atoms**, guest-defined once via `DefineAtom`, reused forever.
  Host table is a flat `Vec` indexed by atom — no hashing at apply time.

Attribute/text *values* are inline UTF-8 in the batch string heap — values are
usually unique; names are Zipf-distributed. Intern what repeats, ship what doesn't.

## Write batch format (guest → host, via `submit`)

Fixed-width op records + separate string heap. Decode is a branch-light array walk;
no varints, no per-op length parsing, records are cache-line friendly.

```
header (16 bytes): magic "GWB1" u32 | op_count u32 | str_heap_off u32 | str_heap_len u32
ops:    op_count × 16-byte records
strs:   string heap — concatenated (len u32, utf8 bytes, pad-to-4) blobs
```

Op record: `op u8 | flags u8 | a u16 | b u32 | c u32 | d u32`.
`str` fields are byte offsets into the string heap (`d = 0xFFFFFFFF` = none).

| op | record fields | maps to (Blitz DocumentMutator) |
|---|---|---|
| `CreateElement` | b=id, c=tag_atom | `create_element` |
| `CreateText` | b=id, d=str | `create_text_node` |
| `SetAttr` | b=id, c=name_atom, d=str | `set_attribute` |
| `RemoveAttr` | b=id, c=name_atom | `clear_attribute` |
| `SetText` | b=id, d=str | `set_node_text` |
| `SetStyle` | b=id, c=prop_atom, d=str | `set_style_property` |
| `RemoveStyle` | b=id, c=prop_atom | `remove_style_property` |
| `AppendChild` | b=parent, c=child | `append_children` |
| `InsertBefore` | b=parent, c=child, d=before | `insert_nodes_before` |
| `Remove` | b=id (subtree) | `remove_and_drop_node` |
| `ReplaceWith` | b=old, c=new | `replace_node_with` |
| `Clear` | b=id (children only) | `remove_and_drop_all_children` |
| `SetInnerHTML` | b=id, d=str | `set_inner_html` — escape hatch; children get no guest ids |
| `DefineAtom` | b=atom, d=str | host atom table |
| `Listen` | b=id, a=event_kind, c=listen_flags | subscription (DOM state, like addEventListener) |
| `Unlisten` | b=id, a=event_kind | |
| `Observe` | b=id, c=what_flags | layout/visibility observation (see Reads) |
| `Unobserve` | b=id, c=what_flags | |
| `Focus` | b=id | focus control |
| `ScrollIntoView` | b=id | v1.1 candidate |

## Events (host → guest)

**Subscription is DOM state** (`Listen`), not callbacks. The host's event driver
(Blitz hit-testing) resolves the target, walks ancestors, and delivers to the
nearest subscribed listener — **bubbling lives host-side**, like a real browser.
Unsubscribed events never cross.

Event record (40-byte fixed header + optional trailing string, in the event region):

```
kind u16 | flags u16 | target u32 | listener u32 | timestamp_ms f64 |
payload [16 bytes, kind-specific] | str_len u32 (0 if none) | str bytes, pad 4
```

- `target` = deepest guest node hit; `listener` = the subscribed ancestor that matched.
- Payloads: pointer `{x f32, y f32, buttons u16, mods u16, detail u16}` · wheel
  `{dx f32, dy f32, mods u16}` · key `{code u16, mods u16, state u8}` · text-input /
  input / change: new value in trailing string · resize `{w f32, h f32, scale f32}` ·
  theme `{dark u32}`.
- v1 kinds: `pointer_down/up/move`, `click`, `dblclick`, `pointer_enter/leave`,
  `wheel`, `key_down/up`, `text_input`, `input`, `change`, `submit`, `focus`, `blur`,
  `scroll`, `window_resize`, `theme_change` + synthetic observation kinds (below).

**Delivery timing** (see Latency contract): discrete events immediately, one
`gwb_events` call per OS event; continuous streams coalesced per frame into one
call with `count > 1`. **Preventable** events (link click, form submit,
key-into-editable — the short list where the host must wait on a verdict) are
always delivered solo, `PREVENTABLE` flag set, return flags honored. Sync is fine —
the guest is in-process and already on the event path.

**Default actions stay native.** Text editing, scrolling, form-control state, focus
traversal run in the engine at full speed; the guest is *informed* (`input`,
`change`, `scroll`), not made responsible for reimplementing them.

## Reads — observation, never interrogation

There is deliberately **no** `getBoundingClientRect`-shaped import: in-process FFI
makes a sync read *look* cheap while forcing style/layout flushes — the JS DOM's
biggest self-inflicted wound. Instead:

- `Observe(id, LAYOUT)` → whenever an observed node's geometry changes, the next
  event delivery includes a synthetic `observed_layout` record (payload
  `x, y, w, h` f32, border-box, viewport-relative). Frame-accurate at delivery.
- `Observe(id, VISIBILITY)` → IntersectionObserver-flavored enter/leave records.

Viewport/theme arrive as window events; input values ride `input`/`change`.
If a genuine pull need emerges, it will be `read_snapshot()` over **last-flushed
frame** data — never a forced flush. (Reserved, not in v1.)

## Scheduling

Fully event-driven. `request_frame()` opts into exactly one `gwb_frame(dt)`; call it
again inside `gwb_frame` for continuous animation. Batches submitted anywhere apply
at the same post-call point; one style/layout/paint per frame regardless of batch count.

## Capabilities (reserved)

WASI grants stay minimal (stdio today). Future host modules (`gwb_net`,
`gwb_storage`, `gwb_gfx`) are separate import modules gated per-app by host policy —
the wasm import list *is* the permission manifest.

## Explicitly not in v1

Multi-window/multi-doc, shared-memory threads, streaming HTML parse into the mount,
guest custom paint (`gwb_gfx` later), synchronous pull reads.

## Supersedes

The v0 spine ABI (`gobrowser_dom`: `submit_batch`/`gobrowser_alloc`/
`gobrowser_handle_event`, u64 ids, full-replace renders). Go `sdk/`, wazero `host/`
(reference host), and `examples/counter` get ported; `protocol/` GDOM encoding is
replaced by the batch format above.
