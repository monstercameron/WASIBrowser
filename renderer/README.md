# renderer

The browser process: Blitz window + wasmtime host + GWB ABI decoder + `web://`
manifest resolver + signed-RPC thread + window chrome + console + logging.

```powershell
cargo build --release
cargo test            # decoder, conformance vectors, wasm limits, CSS scoping,
                      # network policy, manifest validation, apply_batches
```

## Source map

| file | what |
|---|---|
| `src/abi.rs` | GWB ABI host: batch decode, `apply_batches`, host imports (`submit`, `fetch`, `rpc_call`, `navigate`, ...), guest runtime + limits |
| `src/abi_tests.rs` | tests for the above, incl. Go-SDK conformance vectors from `../testdata/abi` |
| `src/css_scope.rs` | rewrites guest stylesheets so they only match inside the tab's mount |
| `src/net.rs` | network policy for guest `fetch`/`rpc_call` (origin allowlist, private-address guard, caps) |
| `src/manifest.rs` | `web://name` -> bundle + capabilities (`services`, `links`, `fetch`) |
| `src/ui.rs` | window chrome, tabs, event routing, guest lifecycle |
| `src/guest.rs` | legacy console-mode guests (non-GWB wasip1 modules) |
| `src/script.rs` | `--script` driver for the golden tests in `scripts/` |
| `src/logger.rs` | `logs/system.log` + crash black-box |

## Golden test scripts

`scripts/*.txt` drive the real UI (`click`, `type`, `hover`, `key`, `wheel`,
`dump`). Run one with, for example:

```powershell
.\target\release\renderer.exe web://shop.local --manifest-root ..\manifests --script scripts\shop-e2e.txt
```

## Blitz dependency

`Cargo.toml` pins Blitz to upstream `DioxusLabs/blitz` at `f550417`, so a fresh
clone builds with no local setup. The benchmark numbers in
`docs/BENCHMARKS.md` ("round 2") were measured against the `gwb-perf` branch:
the same commit plus three `blitz-dom` performance patches (lazy stylo
element-data init, skipping style-attribute flush for attribute-less elements,
`FxHashSet` for `changed_nodes`). Behavior is identical; only speed differs.

### Using a local Blitz checkout

To build against a local checkout (for example the `gwb-perf` branch) without
editing `Cargo.toml`, create `renderer/.cargo/config.toml` (git-ignored):

```toml
[patch."https://github.com/DioxusLabs/blitz"]
blitz-dom    = { path = "C:/src/blitz/packages/blitz-dom" }
blitz-html   = { path = "C:/src/blitz/packages/blitz-html" }
blitz-shell  = { path = "C:/src/blitz/packages/blitz-shell" }
blitz-traits = { path = "C:/src/blitz/packages/blitz-traits" }
blitz-paint  = { path = "C:/src/blitz/packages/blitz-paint" }
```

Publishing `gwb-perf` as a fork (or upstreaming the patches) would let
`Cargo.toml` point at it directly.

## TLS

`ureq` uses `native-tls` (SChannel on Windows, Security.framework on macOS,
OpenSSL on Linux) rather than rustls: rustls pulls in `ring`, which needs a C
toolchain (clang on Windows ARM64) just to build.
