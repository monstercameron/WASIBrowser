# Guest-language DevX on the GWB ABI — humans and agents

Research write-up, 2026-07-06. Based on building the identical Todo app
(`examples/TODO_SPEC.md`) three times against the same LL ABI: Go (`sdk/gwb`),
Rust (`sdk-rust`), freestanding C (`sdk-c/gwb.h`). Raw numbers in
`DEVX-RESULTS.md`. Every claim below was observed in this repo, not assumed.

The two audiences are deliberately separated: what makes a language pleasant
for a **human** (ergonomics, mental load, ecosystem) is not what makes it
productive for an **agent** (feedback-loop quality, failure legibility,
one-shot correctness, token cost). They overlap less than you'd think.

---

## Go

**Human pros**
- Least ceremony of the three: maps + closures made listener→todo routing a
  non-problem; `fmt.Sprintf` for every string; `//go:wasmexport` is one line.
- Fastest edit-compile-run loop (~1s builds) and the flattest learning curve.
- The GC means UI state is just… state. No ownership choreography.

**Human cons**
- 2.68 MB wasm — the Go runtime and GC ride along. Wasmtime spends ~250 ms
  compiling it at every launch (fixable with a module cache, but the cost is real).
- Slowest guest-side encode (0.34 ms vs Rust's 0.10 ms for +100) — allocation
  pressure in the hot path is a language property, not a binding bug.
- Platform split friction: `//go:wasmimport`/`//go:wasmexport` only compile
  under wasip1, so every package needs a native stub twin to keep host tooling
  (`go build ./...`, gopls) green.

**Agent pros**
- Cheapest in tokens: least boilerplate, least code to hold in context, and
  the least surprising language semantics — an agent's prior knowledge of Go
  transfers essentially loss-free.
- Compile errors are short, located, and almost always right about the fix.
- Runtime failures are *loud*: a nil map write or index-out-of-range panics
  with a message that reaches the console/system log. Loud failures are gold
  for agents — silent corruption is how agent sessions go off the rails.

**Agent cons**
- The wasip1/native build-tag split is exactly the kind of environmental
  gotcha agents trip on (editing the wrong twin, diagnostics from the wrong
  build lens — both happened in this session, harmlessly).
- Weak compile-time modeling: wrong node-id handed to the wrong op compiles
  fine. The type system verifies little of the ABI contract.

## Rust

**Human pros**
- Best size/perf balance with real ergonomics: 98 KB, fastest encode, and the
  app reads nearly like the Go version (`HashMap`, `format!`, pattern matching).
- The binding itself came out cleanest: enums, exhaustive matches, a `Batch`
  the borrow checker keeps honest.
- The natural choice if a guest framework (HL tier, reconciler) is ever built
  to be shared across apps: library discipline is where Rust pays off.

**Human cons**
- The single-threaded-guest reality fights the language: globals need
  `thread_local!` + `RefCell` (or `UnsafeCell` for the event region — the
  2024 edition's `static_mut_refs` denial forced exactly that dance).
- `#[unsafe(no_mangle)] extern "C"` export blocks are noisy next to Go's
  one-line directive.
- Slower iteration than Go (cargo + LTO release builds), two toolchains if
  your host work is elsewhere.

**Agent pros**
- **The compiler is a free verification harness.** rustc's diagnostics are
  detailed enough that an agent can usually repair from the error text alone —
  both Rust failures in this session (static-mut ref, type mismatch) were
  one-edit fixes driven purely by the compiler's own suggestion.
- What compiles is very likely correct: ownership and exhaustive matching
  catch the reference-after-remove and forgotten-case bugs that in other
  languages surface as runtime mysteries the agent must debug blind.
- Panics trap loudly at the wasm boundary; the host logs them. No silent state.

**Agent cons**
- Highest token cost per feature: lifetimes, wrapper types, and the
  global-state boilerplate all consume context and iterations.
- Borrow-checker fights can loop a weaker agent (clone-to-appease spirals).
  The escape hatches (`RefCell`, cloning) are fine here — UI state is small —
  but the agent has to know that.

## C (freestanding)

**Human pros**
- 16 KB. Effectively instant wasmtime compile, trivially distributable, and
  the whole "runtime" is ~200 lines of header you can read in one sitting.
- Zero toolchain beyond clang: no target installs, no package manager, no
  sysroot — because the ABI is just little-endian bytes into a buffer.
- Honest about what the machine does. As the systems-floor option (and the
  compile target for *other* languages), it proves the ABI has no hidden
  runtime requirements at all.

**Human cons**
- Everything is manual: fixed-capacity arrays with tombstones instead of
  maps, `append_u32` instead of sprintf, linear scans for listener lookup,
  no clock without WASI.
- The failure mode is corruption, not a message. No bounds checks, no panics.
- Toolchain traps: `-fno-builtin` is mandatory (clang pattern-matches your
  strlen loop into a call to the libc that doesn't exist) — a genuinely
  obscure link error the first time you hit it.

**Agent pros**
- Smallest possible surface: one header, no build system, no dependency
  resolution. An agent can hold the *entire* stack in context.
- Deterministic and fast to build; nothing environmental to drift.

**Agent cons — and these are serious**
- **Degenerate-but-passing is the default failure mode.** The demo binding
  silently drops ops when buffers fill and silently truncates strings; an
  off-by-one just corrupts memory that fails somewhere else, later. Agents
  are *worse* than humans at noticing silent wrongness — they see "it ran"
  and move on. A production C binding for agent use would need assert-and-
  trap hardening (bounds traps, capacity traps) before it's trustworthy.
- No compiler verification of anything that matters here: atom vs node id
  vs event kind are all `u32`/`u16`. Every mistake is a runtime mystery.
- The `-fno-builtin` class of trap costs agents a research loop that Go and
  Rust simply don't have.

---

## Recommendations

**For humans:** prototype in **Go** (velocity, and it's the project's native
tongue), ship in **Rust** when size/startup matter or a shared guest framework
emerges. Use **C** as the conformance floor — its real value is proving the
ABI needs nothing — and as the pattern for future compiled-to-wasm languages.

**For agents:** **Rust first**, slightly ahead of Go. The compiler-as-verifier
property matters more for agents than for humans, because agents lean on
externalized checking (their "did I get it right?" signal must come from the
environment, not intuition). Go is a close second and wins when token budget
or iteration speed dominates. C only with a hardened binding that converts
every silent failure into a trap — the current demo header is explicitly not
that.

**For the platform:** the tier strategy is validated — one wire ABI, per-
language thin bindings, ergonomics pushed guest-side. Next DevX investments,
in value order: wasmtime module cache (kills Go's 250 ms launch tax), an
assert-hardened C binding, and the HL tier where humans and agents both stop
caring about node ids entirely.
