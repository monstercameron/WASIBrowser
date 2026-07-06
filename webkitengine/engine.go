package webkitengine

import (
	"errors"
	"fmt"
	"sync"
	"sync/atomic"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/protocol"
)

// Options configures a new Engine backed by the real WebKit host process.
type Options struct {
	// BinaryPath is the path to the WinCairo MiniBrowser host executable.
	// Leave empty to get ErrNoBinaryConfigured. Use NewLoopback for tests.
	BinaryPath string

	// PipeName is the Windows named-pipe identifier (without the \\.\pipe\
	// prefix). If empty, a unique name is generated.
	PipeName string

	// EventBufSize is the capacity of the Events() channel. Default: 64.
	EventBufSize int
}

// Engine implements engine.Engine via the WebKit2 web-process DOM agent.
//
// Only one Apply / RootNode / LoadHTML call may be in flight at a time; the
// callMu mutex enforces this. The single-inflight invariant lets ackCh and
// mountCh be size-1 buffers without risk of cross-contamination.
type Engine struct {
	tr       Transport
	rev      uint64 // accessed via atomic where possible, guarded by revMu for RMW
	revMu    sync.Mutex
	events   chan engine.Event
	ackCh    chan Message // DOMAck frames for in-flight Apply
	mountCh  chan Message // MountAck frames for in-flight RootNode
	done     chan struct{}
	closeOnce sync.Once
	callMu   sync.Mutex // serialises Apply / RootNode / LoadHTML
}

// compile-time interface check
var _ engine.Engine = (*Engine)(nil)

// New creates an Engine connected to a real WinCairo WebKit host process.
// Returns ErrNoBinaryConfigured if opts.BinaryPath is empty.
func New(opts Options) (*Engine, error) {
	pipeName := opts.PipeName
	if pipeName == "" {
		pipeName = fmt.Sprintf("gdom-%d", nextUID())
	}
	tr, err := newProcessTransport(opts.BinaryPath, pipeName)
	if err != nil {
		return nil, err
	}
	return newEngine(tr, opts.EventBufSize), nil
}

// NewLoopback creates an Engine whose transport drives dom directly, with no
// real WebKit process. Batches applied to the Engine update dom immediately;
// events injected via dom.InjectEvent arrive on Engine.Events(). Intended for
// tests and headless integration.
func NewLoopback(dom *engine.TestDOM) (*Engine, error) {
	return newEngine(newLoopbackTransport(dom), 0), nil
}

func newEngine(tr Transport, eventBufSize int) *Engine {
	if eventBufSize <= 0 {
		eventBufSize = 64
	}
	e := &Engine{
		tr:      tr,
		events:  make(chan engine.Event, eventBufSize),
		ackCh:   make(chan Message, 1),
		mountCh: make(chan Message, 1),
		done:    make(chan struct{}),
	}
	go e.recvLoop()
	return e
}

// recvLoop runs in a background goroutine and dispatches inbound IPC frames:
//   - KindDOMAck  → ackCh  (Apply's waiter)
//   - KindMountAck → mountCh (RootNode's waiter)
//   - KindEvent   → events  (surfaced to callers via Events())
func (e *Engine) recvLoop() {
	for {
		msg, err := e.tr.Recv()
		if err != nil {
			return // transport closed; Engine.Close() will clean up
		}
		switch msg.Kind {
		case KindDOMAck:
			select {
			case e.ackCh <- msg:
			case <-e.done:
				return
			}
		case KindMountAck:
			select {
			case e.mountCh <- msg:
			case <-e.done:
				return
			}
		case KindEvent:
			kind, nodeID, value, err := decodeEventBody(msg.Body)
			if err != nil {
				continue // malformed event; skip
			}
			ev := engine.Event{
				Kind:   kind,
				Target: protocol.NodeID(nodeID),
				Value:  value,
			}
			select {
			case e.events <- ev:
			case <-e.done:
				return
			}
		}
	}
}

// LoadHTML sends the host document to the web-process DOM agent. The call
// is fire-and-forget on the IPC channel (no ack expected); the page-load
// notification arrives asynchronously when the document is ready.
func (e *Engine) LoadHTML(html string) error {
	e.callMu.Lock()
	defer e.callMu.Unlock()
	return e.tr.Send(Message{Kind: KindLoadHTML, Body: []byte(html)})
}

// RootNode resolves selector in the web-process document and returns the
// NodeID the app mounts under. Returns an error if the selector is not found.
func (e *Engine) RootNode(selector string) (protocol.NodeID, error) {
	e.callMu.Lock()
	defer e.callMu.Unlock()

	if err := e.tr.Send(Message{Kind: KindMount, Body: []byte(selector)}); err != nil {
		return 0, err
	}
	select {
	case msg := <-e.mountCh:
		v, err := decodeU64(msg.Body)
		if err != nil {
			return 0, err
		}
		if v == 0 {
			return 0, fmt.Errorf("webkitengine: selector %q not found in document", selector)
		}
		return protocol.NodeID(v), nil
	case <-e.done:
		return 0, errors.New("webkitengine: engine closed while waiting for MountAck")
	}
}

// Apply forwards the GDOM batch to the web-process agent, waits for the
// DOMAck, and returns the new committed revision. Returns
// engine.ErrRevisionMismatch if the batch's BaseRevision does not match the
// current engine revision (either detected locally or by the agent NACK).
func (e *Engine) Apply(batch []byte) (uint64, error) {
	e.callMu.Lock()
	defer e.callMu.Unlock()

	// Local fast-reject: parse header without allocating a full decode.
	h, _, err := protocol.DecodeHeader(batch)
	if err != nil {
		return 0, err
	}
	e.revMu.Lock()
	cur := e.rev
	e.revMu.Unlock()
	if h.BaseRevision != cur {
		return 0, engine.ErrRevisionMismatch
	}

	if err := e.tr.Send(Message{Kind: KindDOMBatch, Body: batch}); err != nil {
		return 0, err
	}
	select {
	case msg := <-e.ackCh:
		newRev, err := decodeU64(msg.Body)
		if err != nil {
			return 0, err
		}
		if newRev == 0 {
			// Agent NACK: stale revision or decode error.
			return 0, engine.ErrRevisionMismatch
		}
		e.revMu.Lock()
		e.rev = newRev
		e.revMu.Unlock()
		return newRev, nil
	case <-e.done:
		return 0, errors.New("webkitengine: engine closed while waiting for DOMAck")
	}
}

// Revision returns the current committed DOM revision.
func (e *Engine) Revision() uint64 {
	return atomic.LoadUint64(&e.rev)
}

// Events returns the channel of input events received from the web process.
// The channel is buffered; events are dropped if the caller does not drain it.
func (e *Engine) Events() <-chan engine.Event { return e.events }

// Close shuts down the engine, stops the recv loop, and releases the transport.
func (e *Engine) Close() error {
	var err error
	e.closeOnce.Do(func() {
		close(e.done)
		err = e.tr.Close()
	})
	return err
}

// ---- helpers ----------------------------------------------------------------

var uidCounter uint64

func nextUID() uint64 {
	return atomic.AddUint64(&uidCounter, 1)
}
