package webkitengine

import (
	"errors"
	"io"
	"net"
	"os/exec"
	"sync"

	"github.com/monstercameron/gowebbrowser/engine"
)

// ErrNoBinaryConfigured is returned by New when Options.BinaryPath is empty.
// Use NewLoopback for tests; configure BinaryPath when a real WebKit build is
// available.
var ErrNoBinaryConfigured = errors.New(
	"webkitengine: BinaryPath not configured; " +
		"set Options.BinaryPath to the WinCairo MiniBrowser executable path, " +
		"or use NewLoopback for in-process testing")

// Transport abstracts the IPC channel between the Go broker and the web-process
// DOM agent. The framing protocol is defined in ipc.go.
type Transport interface {
	// Send encodes msg as an IPC frame and writes it to the channel.
	Send(msg Message) error
	// Recv reads and decodes one IPC frame. Blocks until a frame arrives or
	// the transport is closed.
	Recv() (Message, error)
	// Close releases transport resources.
	Close() error
}

// ============================================================================
// processTransport — named-pipe connection to the WinCairo host process
// ============================================================================

// processTransport connects to the WinCairo MiniBrowser / WebKit host process
// over a Windows named pipe. It is the production transport; the actual spawn
// is guarded so the package compiles and links without a WebKit tree present.
//
// Named-pipe contract:
//
//	The broker generates a unique pipe name of the form "gdom-<n>" and passes it
//	to the host binary as the argument --ipc-pipe=<name>. The host binary must
//	call CreateNamedPipe / ConnectNamedPipe on \\.\pipe\<name> before signalling
//	that it is ready to accept frames. The broker then opens the same path for
//	duplex I/O and begins sending IPC frames.
//
// NOTE: net.Dial("npipe", ...) is a placeholder; replace with a proper
// named-pipe dialer (e.g. github.com/Microsoft/go-winio) once the host binary
// is built. The Dial call will return "unknown network npipe" at runtime,
// which is acceptable because tests always use loopbackTransport.
type processTransport struct {
	conn io.ReadWriteCloser
	mu   sync.Mutex // serialises concurrent Send calls
}

// newProcessTransport spawns the WebKit host binary and connects to it via a
// Windows named pipe. Returns ErrNoBinaryConfigured if binaryPath is empty.
func newProcessTransport(binaryPath, pipeName string) (*processTransport, error) {
	if binaryPath == "" {
		return nil, ErrNoBinaryConfigured
	}

	// Spawn the host process. It must create \\.\pipe\<pipeName> before
	// accepting connections.
	cmd := exec.Command(binaryPath, "--ipc-pipe="+pipeName)
	if err := cmd.Start(); err != nil {
		return nil, err
	}

	// Open the named pipe for duplex I/O.
	// TODO: replace with a proper Windows named-pipe dialer once the host
	// binary is built (e.g. winio.DialPipe or CreateFile on the pipe path).
	// The "npipe" network token is not recognised by Go's stdlib net package
	// and will produce "unknown network npipe" at runtime; that is expected
	// in the current state where no host binary exists.
	conn, err := net.Dial("npipe", `\\.\pipe\`+pipeName)
	if err != nil {
		_ = cmd.Process.Kill()
		return nil, err
	}

	return &processTransport{conn: conn}, nil
}

func (t *processTransport) Send(msg Message) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	_, err := t.conn.Write(encodeFrame(msg))
	return err
}

func (t *processTransport) Recv() (Message, error) {
	return readFrame(t.conn)
}

func (t *processTransport) Close() error { return t.conn.Close() }

// ============================================================================
// loopbackTransport — in-process transport backed by engine.TestDOM
// ============================================================================

// loopbackTransport is an in-process Transport that drives an engine.TestDOM
// directly. Sending a KindDOMBatch applies the batch to the TestDOM
// immediately; the DOMAck is queued synchronously so the Engine's recv loop
// picks it up without round-tripping to any real process. Events injected via
// dom.InjectEvent surface on the Engine's Events() channel via a forwarder
// goroutine.
//
// It proves the adapter's IPC framing, revision tracking, and event plumbing
// end-to-end without requiring a live WebKit build.
type loopbackTransport struct {
	dom    *engine.TestDOM
	recvCh chan Message  // frames flowing WebKit→Go (acks + events)
	closed chan struct{}
	once   sync.Once
}

// newLoopbackTransport creates a loopbackTransport wired to dom and starts the
// event-forwarding goroutine.
func newLoopbackTransport(dom *engine.TestDOM) *loopbackTransport {
	t := &loopbackTransport{
		dom:    dom,
		recvCh: make(chan Message, 128), // buffered: Send never blocks on acks
		closed: make(chan struct{}),
	}
	go t.forwardEvents()
	return t
}

// forwardEvents bridges engine.Event values emitted by TestDOM into the recv
// channel as KindEvent messages, mirroring what the real web-process agent
// would post back to the broker over the named pipe.
func (t *loopbackTransport) forwardEvents() {
	evCh := t.dom.Events()
	for {
		select {
		case ev, ok := <-evCh:
			if !ok {
				return // TestDOM closed
			}
			body := encodeEventBody(ev.Kind, uint64(ev.Target), ev.Value)
			select {
			case t.recvCh <- Message{Kind: KindEvent, Body: body}:
			case <-t.closed:
				return
			}
		case <-t.closed:
			return
		}
	}
}

// Send handles the Go→WebKit direction by directly invoking TestDOM methods
// and queuing a response frame in recvCh for the Engine's recv loop.
func (t *loopbackTransport) Send(msg Message) error {
	switch msg.Kind {
	case KindLoadHTML:
		// TestDOM.LoadHTML is a no-op that always succeeds; no ack is sent
		// (matches real WebKit behaviour: LoadHTML is fire-and-forget until
		// the page-load callback fires, which is out of scope for loopback).
		_ = t.dom.LoadHTML(string(msg.Body))
		return nil

	case KindMount:
		nodeID, err := t.dom.RootNode(string(msg.Body))
		var v uint64
		if err == nil {
			v = uint64(nodeID)
		}
		return t.push(Message{Kind: KindMountAck, Body: encodeU64(v)})

	case KindDOMBatch:
		newRev, err := t.dom.Apply(msg.Body)
		var v uint64
		if err == nil {
			v = newRev // 0 from a successful Apply would be ambiguous but
			// TestDOM rev starts at 0 and bumps on Commit, so after the
			// first Commit it is always ≥ 1; 0 == NACK is unambiguous.
		}
		return t.push(Message{Kind: KindDOMAck, Body: encodeU64(v)})

	default:
		// Unknown direction; ignore silently (forward compat).
		return nil
	}
}

func (t *loopbackTransport) push(msg Message) error {
	select {
	case t.recvCh <- msg:
		return nil
	case <-t.closed:
		return errors.New("webkitengine: loopback transport closed")
	}
}

// Recv blocks until a frame is available or the transport is closed.
func (t *loopbackTransport) Recv() (Message, error) {
	select {
	case msg, ok := <-t.recvCh:
		if !ok {
			return Message{}, io.EOF
		}
		return msg, nil
	case <-t.closed:
		return Message{}, io.EOF
	}
}

// Close shuts down the forwarder goroutine and unblocks any pending Recv.
func (t *loopbackTransport) Close() error {
	t.once.Do(func() { close(t.closed) })
	return nil
}
