//go:build !wasip1

package app

import "github.com/monstercameron/gowebbrowser/protocol"

// NativeMount performs the initial mount with the given root NodeID. Call
// after Run in native tests to trigger the first render without a wasm runtime.
func NativeMount(hostRootID uint64) {
	mount(hostRootID)
}

// NativeHandleEvent decodes an eventmsg-encoded event buffer and dispatches it,
// then triggers a re-render. Use eventmsg.Encode to build the buffer.
func NativeHandleEvent(buf []byte) {
	handleEvent(buf)
}

// NativeClickHandlerIDs returns the NodeIDs that have registered click handlers
// in the most recent render. Tests use this to find which NodeID to inject
// click events on.
func NativeClickHandlerIDs() []protocol.NodeID {
	ids := make([]protocol.NodeID, 0, len(clickHandlers))
	for id := range clickHandlers {
		ids = append(ids, id)
	}
	return ids
}

// NativeReset resets all runtime state. Call between tests for isolation.
func NativeReset() {
	rootFn = nil
	rootNodeID = 0
	nextNodeID = 1
	batchCounter = 0
	currentRevision = 0
	prevRootChild = protocol.NullNode
	clickHandlers = nil
}
