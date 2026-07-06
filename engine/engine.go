// Package engine defines the rendering-engine seam. The browser drives an Engine
// by applying GDOM batches and receives input Events back. There are two
// implementations: TestDOM (pure Go, for tests and headless runs) and the WebKit
// adapter (WebKit2/WinCairo web-process DOM agent), which share this contract.
package engine

import (
	"errors"

	"github.com/monstercameron/gowebbrowser/protocol"
)

// ErrRevisionMismatch is returned by Apply when a batch's BaseRevision does not
// match the engine's current revision (the broker must resync and resend).
var ErrRevisionMismatch = errors.New("engine: batch revision mismatch")

// Event is an input event delivered from the engine to the app.
type Event struct {
	Kind   uint32
	Target protocol.NodeID
	Value  string
}

// Event kinds. Keep in sync with the C++ agent and the SDK.
const (
	EventClick uint32 = 1
	EventInput uint32 = 2
	EventKey   uint32 = 3
)

// Engine is the rendering backend the browser talks to. Implementations apply
// DOM patches with no JavaScript involved.
type Engine interface {
	// LoadHTML loads the host document that declares the wasm app.
	LoadHTML(html string) error

	// RootNode resolves a selector (e.g. "#app") to the NodeID the app mounts
	// under. The same id is handed to the app via the mount export.
	RootNode(selector string) (protocol.NodeID, error)

	// Apply applies one GDOM batch and returns the new committed revision.
	// Returns ErrRevisionMismatch if the batch is stale.
	Apply(batch []byte) (revision uint64, err error)

	// Revision is the current committed revision.
	Revision() uint64

	// Events is the stream of input events from the engine.
	Events() <-chan Event

	// Close releases engine resources.
	Close() error
}
