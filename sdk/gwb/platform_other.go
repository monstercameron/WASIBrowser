//go:build !wasip1

package gwb

// Native stub so the package (and its dependents) compile on the host for
// tooling/tests. A real guest must be built with GOOS=wasip1 GOARCH=wasm.

func hostSubmit(buf []byte) uint32 { _ = buf; return 0 }

// Log is a no-op outside wasip1.
func Log(level uint32, msg string) { _, _ = level, msg }

// RequestFrame is a no-op outside wasip1.
func RequestFrame() {}

// Fetch/Rpc/SetSession/ClearSession are no-ops outside wasip1 (tooling/test
// builds on the host) — see hostFetch/hostRpcCall/hostSessionSet/hostSessionClear.
func hostFetch(url string) uint32                     { _ = url; return 0 }
func hostRpcCall(buf []byte) uint32                   { _ = buf; return 0 }
func hostSessionSet(token string)                     { _ = token }
func hostSessionClear()                               {}
func hostNavigate(target string, flags uint32) uint32 { _, _ = target, flags; return 0 }
