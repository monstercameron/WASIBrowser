# GWB/HL — the high-level guest API (Go-first sketch, DRAFT 2026-07-05)

The high-level tier is a **guest-side library** over the GWB/LL wire ABI
(`ABI.md`). The host never sees it; it compiles down to LL batches. Per-language;
Go is binding #1. Design rule: **HL is sugar, never a wall** — every HL object
exposes its raw node id, and raw batch access works on HL-created nodes, so a
hot path can drop a tier without rewriting the screen around it.

## Package layout (Go)

- `sdk/gwb` — **low-level binding.** Zero-alloc batch builder, well-known atom
  constants, typed event decoding, `//go:wasmimport` shims. What a benchmark or a
  framework author uses.
- `sdk/ui` — **high-level.** Retained element handles, closures for events,
  automatic ids/atoms, automatic batch flushing. What an app author uses.

## Low-level tier (`sdk/gwb`) — optimize

```go
var b gwb.Batch                      // reusable; zero allocations after warmup

func start(w, h, scale float32, flags uint32) {
    row := gwb.NewID()
    b.CreateElement(row, gwb.Div)
    b.SetAttr(row, gwb.Class, "row")
    b.AppendChild(gwb.Root, row)
    b.Listen(row, gwb.Click, 0)
    b.Submit()                       // one crossing; buffer reusable immediately
}

func onEvent(e *gwb.Event) (flags uint32) {
    switch e.Kind {
    case gwb.Click:
        // e.Listener, e.Target, e.X, e.Y — decoded views over the event region
    }
    return 0
}
```

Characteristics: explicit ids, explicit flush points, `string`-free paths available
(`SetAttrBytes`), atoms as constants. The counter benchmark and any framework
(a future Go reconciler, a Rust dioxus-like) target this tier.

## High-level tier (`sdk/ui`) — write quickly

Retained **direct-mode** handles — objects you keep and mutate, like a sane
version of `document.createElement` + live references. Deliberately NOT a virtual
DOM: no diffing tax, no reconciliation surprises; mutations are explicit and
translate 1:1 to LL ops. (A declarative/diffing layer can sit on top later —
it belongs above HL, not inside it.)

```go
func main() {} // wasip1 reactor; setup runs in init()

func init() {
    ui.Main(func(root ui.El) {
        count := 0
        label := root.Div(ui.Class("count")).Text("Count 0")
        root.Button("Increment").OnClick(func(e ui.Event) {
            count++
            label.SetText(fmt.Sprintf("Count %d", count))
        })
    })
}
```

What HL automates (and what it costs):

| concern | HL behavior | cost over LL |
|---|---|---|
| node ids | allocated + recycled internally | ~zero |
| atoms | strings interned on first use, cached | one map hit per *new* name |
| batching | ops appended to a hidden frame batch, auto-`Submit` on handler return | zero — same crossings |
| events | closures in a `(node,kind) → func` registry; `Listen` emitted on `On*` | one map hit per event |
| strings | convenience `string` params | Go allocs the app chose to make |

Auto-flush rule: HL submits its pending batch when the guest export that invoked
user code returns (`gwb_start` / `gwb_events` / `gwb_frame`). Manual `ui.Flush()`
exists but is almost never needed — the host applies post-call anyway, so early
submission buys nothing.

## Mixing tiers

```go
list := root.Div(ui.ID("feed"))          // HL handle
raw := list.NodeID()                      // escape hatch: raw u32
var b gwb.Batch                           // hot path: build 10k rows LL-style
for i := range rows {
    id := gwb.NewID()
    b.CreateElement(id, gwb.Div)
    b.SetTextRef(id, rows[i])             // no per-row string copies
    b.AppendChild(raw, id)
}
b.Submit()
```

HL never wraps or hides LL state; both tiers share the same id space and the same
hidden frame batch boundary, so interleaving is safe by construction.

## Porting order

1. `sdk/gwb` (LL binding) + rebuild `examples/counter` on it — this is what the
   Rust decoder gets tested against.
2. `sdk/ui` (HL) + the same counter in HL form — two counters, one ABI, proving
   the tier claim.
3. `examples/bench` — the M5 DOM-write benchmark, LL tier, vs JS-in-Chrome.
