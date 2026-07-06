//go:build !wasip1

package gwb

// Native stub so the package (and its dependents) compile on the host for
// tooling/tests. A real guest must be built with GOOS=wasip1 GOARCH=wasm.

func hostSubmit(buf []byte) uint32 { _ = buf; return 0 }

// Log is a no-op outside wasip1.
func Log(level uint32, msg string) { _, _ = level, msg }
