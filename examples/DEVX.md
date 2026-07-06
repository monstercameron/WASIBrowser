# Tri-language DevX report — GWB ABI v1 (2026-07-06)

One identical Todo app (TODO_SPEC.md) in Go, Rust, and C, each on a ~200-line
LL binding. All three verified live in the renderer: identical DOM, identical
styles, identical behavior — and **identical wire traffic** (same batch
byte-for-byte sizes for every interaction). The ABI is language-neutral in
practice, not just on paper.

## Measured

| | Go | Rust | C (freestanding) |
|---|---|---|---|
| wasm size | **2,680 KB** | **98 KB** | **16 KB** |
| app LOC (non-blank) | 162 | 191 | 174 |
| binding LOC | 358 | 240 | 202 |
| `+100` guest encode | 0.34 ms | 0.10 ms | n/a (no clock) |
| `+100` wire | 1601 ops / 32,056 B | identical | identical |
| host decode+apply (+100) | ~1 ms | ~1 ms | ~1 ms |
| toggle / delete / add | 3 op / 2 op / 18 op | identical | identical |
| build | `GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared` | `cargo build --target wasm32-wasip1 --release` | `clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin -Wl,--no-entry -Wl,--export-memory` |

## DevX notes

**Go** — least ceremony: maps and closures make the listener→todo routing
trivial; `fmt.Sprintf` everywhere; `//go:wasmexport` is clean. Costs: 2.7 MB
binary (Go runtime + GC on board), slowest encode (3.4× Rust — GC/alloc
pressure), and the platform split (`platform_wasip1.go` + native stub) needed
to keep host tooling green.

**Rust** — best size/perf balance with real ergonomics: `HashMap` + `format!`
read like the Go version; the binding is the cleanest of the three (enums,
borrow-checked `Batch`). Costs: single-threaded-guest global state fights the
language (`thread_local!` + `RefCell` boilerplate), edition-2024 banned
`static mut` for the event region (needed an `UnsafeCell` wrapper), and
`no_mangle extern "C"` exports are noisier than Go's directive.

**C** — startlingly viable and the smallest by far, since the ABI is just
"write little-endian bytes into a buffer": the binding is a single header.
Costs: everything is manual — fixed-capacity arrays with tombstones instead
of maps, hand-rolled `append_u32` instead of sprintf, linear scans for
listener lookup, no clock without WASI. One real toolchain trap:
`-fno-builtin` is mandatory (clang pattern-matches your strlen loop into a
call to libc strlen, which doesn't exist).

## Findings against the platform

- Blitz alpha.6 **text input works end-to-end**: focus by click, keystrokes,
  per-keystroke `input` events with the control value, value-clear via
  `SetAttr(value, "")`.
- Found+fixed under stress: applying guest batches **mid-event-dispatch**
  crashed blitz-dom (`node.rs:1119`) when a `Remove` invalidated the driver's
  chain. Apply is now strictly post-dispatch, as the ABI spec always said.
  The crash logger black-boxed it perfectly.
- 100-component mount = 1 crossing, 32 KB, ~1 ms end-to-end on the X2. The
  same operation through the JS DOM is 700+ calls; this is the M5 benchmark's
  opening argument.
