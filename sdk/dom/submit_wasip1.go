//go:build wasip1

// Package dom provides the wasm-side DOM submission layer. The guest calls
// Commit to send an encoded GDOM batch to the host via the gobrowser_dom
// host import module.
package dom

import (
	"runtime"
	"unsafe"

	"github.com/monstercameron/gowebbrowser/protocol"
)

// submit_batch is the host import that receives a GDOM batch from the guest.
// The host reads [ptr, ptr+len) from linear memory, applies it to the engine,
// and returns the new revision (0 on a revision mismatch).
//
//go:wasmimport gobrowser_dom submit_batch
func submit_batch(ptr uint32, length uint32) uint32

// Commit encodes b and passes the bytes to the host via the submit_batch import.
// It returns the revision returned by the host.
func Commit(b *protocol.Batch) uint32 {
	buf := b.Encode()
	if len(buf) == 0 {
		return 0
	}
	ptr := unsafe.SliceData(buf)
	rev := submit_batch(uint32(uintptr(unsafe.Pointer(ptr))), uint32(len(buf)))
	runtime.KeepAlive(buf)
	return rev
}
