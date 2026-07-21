//go:build wasip1

package gwb

import "unsafe"

//go:wasmimport gwb submit
func impSubmit(ptr unsafe.Pointer, length uint32) uint32

//go:wasmimport gwb event_region
func impEventRegion(ptr unsafe.Pointer, length uint32)

//go:wasmimport gwb log
func impLog(level uint32, ptr unsafe.Pointer, length uint32)

//go:wasmimport gwb request_frame
func impRequestFrame()

//go:wasmimport gwb fetch
func impFetch(ptr unsafe.Pointer, length uint32) uint32

//go:wasmimport gwb rpc_call
func impRpcCall(ptr unsafe.Pointer, length uint32) uint32

//go:wasmimport gwb session_set
func impSessionSet(ptr unsafe.Pointer, length uint32)

//go:wasmimport gwb session_clear
func impSessionClear()

//go:wasmimport gwb navigate
func impNavigate(ptr unsafe.Pointer, length uint32, flags uint32) uint32

// RequestFrame schedules exactly one OnFrame call at the next paint.
// Call it again inside OnFrame for continuous animation.
func RequestFrame() { impRequestFrame() }

func hostSubmit(buf []byte) uint32 {
	if len(buf) == 0 {
		return 0
	}
	return impSubmit(unsafe.Pointer(&buf[0]), uint32(len(buf)))
}

func hostFetch(url string) uint32 {
	if len(url) == 0 {
		return 0
	}
	b := []byte(url)
	return impFetch(unsafe.Pointer(&b[0]), uint32(len(b)))
}

func hostRpcCall(buf []byte) uint32 {
	if len(buf) == 0 {
		return 0
	}
	return impRpcCall(unsafe.Pointer(&buf[0]), uint32(len(buf)))
}

func hostSessionSet(token string) {
	if len(token) == 0 {
		return
	}
	b := []byte(token)
	impSessionSet(unsafe.Pointer(&b[0]), uint32(len(b)))
}

func hostSessionClear() { impSessionClear() }

func hostNavigate(target string, flags uint32) uint32 {
	if len(target) == 0 {
		return NavUndeclared
	}
	b := []byte(target)
	return impNavigate(unsafe.Pointer(&b[0]), uint32(len(b)), flags)
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

//go:wasmexport gwb_frame
func gwbFrame(dtMS float32) {
	dispatchFrame(dtMS)
}
