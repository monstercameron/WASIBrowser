//go:build !wasip1

// Package dom provides the wasm-side DOM submission layer. In native builds
// (tests, tooling) Commit records the last encoded batch and returns an
// incrementing synthetic revision so callers can track state across renders.
package dom

import "github.com/monstercameron/gowebbrowser/protocol"

// LastBatch holds the most recent encoded batch submitted via Commit. Tests
// read this to inspect what the app emitted without needing a wasm runtime.
var LastBatch []byte

var nativeRev uint32

// Commit encodes b, stores it in LastBatch, and returns an incrementing
// revision number (matching what engine.TestDOM would return after each
// Commit() call).
func Commit(b *protocol.Batch) uint32 {
	LastBatch = b.Encode()
	nativeRev++
	return nativeRev
}

// ResetRevision resets the synthetic revision counter. Call between tests to
// ensure deterministic revision numbering.
func ResetRevision() {
	nativeRev = 0
	LastBatch = nil
}
