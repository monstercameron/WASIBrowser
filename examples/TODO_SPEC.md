# Tri-language Todo spec (DevX comparison)

One app, three guests: `todo-go`, `todo-rs`, `todo-c`. All three MUST produce the
same DOM, styles, and behavior — only the h2 title differs ("Todos — Go" /
"Todos — Rust" / "Todos — C"). Purpose: stress the GWB ABI and compare DevX.

## DOM (inside mount root, guest id 1)

```
card div          padding:20px background:#26282c border-radius:10px width:420px
├ h2              margin:0 0 12px 0                text "Todos — {LANG}"
├ row div         display:flex gap:8px margin:0 0 14px 0
│ ├ input         type=text placeholder="What needs doing?" width:240px
│ ├ button        "Add"
│ └ button        "+100"
├ list div        (container for item rows)
└ status p        margin:10px 0 0 0 font-size:12px color:#9a9fa6
                  text "N items, M done"
```

Item row (appended to list div):

```
item div          display:flex gap:8px margin:0 0 6px 0
├ label span      text = todo text; cursor:pointer
└ button          "x"
```

## Behavior

- `Listen(input, EvInput)` → remember latest control value.
- `Add` click → append one todo: text = remembered value, or `Item N` (N =
  1-based creation counter) if empty; clear remembered value and set the
  input's `value` attribute to "".
- `+100` click → append 100 todos `Item N..N+99` in **one batch**; log the
  guest-side elapsed ms via `gwb.log` (C may skip timing — no clock).
- label click → toggle done. Done style: `color:#9a9fa6` +
  `text-decoration:line-through`. Undone: `color` reset to `#e8e8e8`,
  `text-decoration` removed (RemoveStyle).
- `x` click → remove the item row (subtree), update status.
- status text always `"{total} items, {done} done"`.

## Atoms

Everything from the well-known table except `text-decoration`, which each guest
must register with `DefineAtom(1024, "text-decoration")` — deliberately
exercising dynamic atoms in all three languages.

## Non-goals

No persistence, no editing, no filters. Input focus/typing depends on Blitz
alpha.6 form widgets — if typing doesn't reach EvInput, all three fall back to
auto-named items and the finding goes in the DevX report.
