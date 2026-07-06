//go:build wasip1

package gwb

import "unsafe"

//go:wasmimport gwb submit
func impSubmit(ptr unsafe.Pointer, length uint32) uint32

//go:wasmimport gwb event_region
func impEventRegion(ptr unsafe.Pointer, length uint32)

//go:wasmimport gwb log
func impLog(level uint32, ptr unsafe.Pointer, length uint32)

func hostSubmit(buf []byte) uint32 {
	if len(buf) == 0 {
		return 0
	}
	return impSubmit(unsafe.Pointer(&buf[0]), uint32(len(buf)))
}

// Log writes a structured line to the host console + system log.
func Log(level uint32, msg string) {
	if len(msg) == 0 {
		return
	}
	b := []byte(msg)
	impLog(level, unsafe.Pointer(&b[0]), uint32(len(b)))
}

//go:wasmexport gwb_abi_version
func gwbAbiVersion() uint32 { return 1 << 16 }

//go:wasmexport gwb_start
func gwbStart(w, h, scale float32, flags uint32) {
	impEventRegion(unsafe.Pointer(&eventBuf[0]), uint32(len(eventBuf)))
	dispatchStart(w, h, scale, flags)
}

//go:wasmexport gwb_events
func gwbEvents(count uint32) uint32 {
	return dispatchEvents(count)
}
