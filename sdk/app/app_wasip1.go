//go:build wasip1

package app

import "unsafe"

// gobrowser_mount is called by the host after loading and instantiating the
// wasm module. rootID is the NodeID of the mount-root element (resolved by the
// host from the selector passed to Mount).
//
//go:wasmexport gobrowser_mount
func gobrowser_mount(rootID uint64) {
	mount(rootID)
}

// gobrowser_handle_event is called by the host to deliver an input event. The
// host has already written the eventmsg-encoded bytes into the buffer that was
// returned by gobrowser_alloc(length).
//
//go:wasmexport gobrowser_handle_event
func gobrowser_handle_event(ptr uint32, length uint32) {
	if length == 0 {
		return
	}
	buf := unsafe.Slice((*byte)(unsafe.Pointer(uintptr(ptr))), int(length))
	// Copy to a Go-managed slice so handleEvent can hold a reference safely.
	data := make([]byte, length)
	copy(data, buf)
	handleEvent(data)
}

// gobrowser_alloc allocates n bytes of wasm linear memory for the host to
// write an event message into. The host calls this immediately before
// gobrowser_handle_event to obtain a valid write target.
//
//go:wasmexport gobrowser_alloc
func gobrowser_alloc(n uint32) uint32 {
	if n == 0 {
		return 0
	}
	buf := make([]byte, n)
	// Store in the package-level eventBuf so the GC does not collect it
	// before the host writes into it.
	eventBuf = buf
	return uint32(uintptr(unsafe.Pointer(unsafe.SliceData(buf))))
}

// eventBuf holds the most recently allocated event buffer to prevent GC.
var eventBuf []byte
