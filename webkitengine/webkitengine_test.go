package webkitengine

// Internal test package so we can exercise the unexported IPC codec (encodeFrame,
// readFrame, encodeEventBody, decodeEventBody) as well as the public API.

import (
	"bytes"
	"testing"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/protocol"
)

// ============================================================================
// 1. IPC framing codec round-trips
// ============================================================================

// TestIPCFrameRoundTrip encodes a Message for each MessageKind then reads it
// back and asserts that Kind, Flags, and Body are preserved verbatim.
func TestIPCFrameRoundTrip(t *testing.T) {
	cases := []struct {
		name string
		msg  Message
	}{
		{
			name: "DOMBatch",
			msg:  Message{Kind: KindDOMBatch, Body: []byte("GDOM-fake-batch")},
		},
		{
			name: "DOMAck",
			msg:  Message{Kind: KindDOMAck, Body: encodeU64(17)},
		},
		{
			name: "Event",
			msg:  Message{Kind: KindEvent, Body: encodeEventBody(engine.EventClick, 42, "clicked")},
		},
		{
			name: "LoadHTML",
			msg:  Message{Kind: KindLoadHTML, Body: []byte("<html><body><div id=\"app\"></div></body></html>")},
		},
		{
			name: "Mount",
			msg:  Message{Kind: KindMount, Body: []byte("#app")},
		},
		{
			name: "MountAck",
			msg:  Message{Kind: KindMountAck, Body: encodeU64(1)},
		},
		{
			name: "EmptyBody",
			msg:  Message{Kind: KindLoadHTML, Body: []byte{}},
		},
		{
			name: "FlagsPreserved",
			msg:  Message{Kind: KindDOMAck, Flags: 0xBEEF, Body: encodeU64(0)},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			wire := encodeFrame(tc.msg)

			got, err := readFrame(bytes.NewReader(wire))
			if err != nil {
				t.Fatalf("readFrame: %v", err)
			}
			if got.Kind != tc.msg.Kind {
				t.Errorf("Kind: got %d, want %d", got.Kind, tc.msg.Kind)
			}
			if got.Flags != tc.msg.Flags {
				t.Errorf("Flags: got %d, want %d", got.Flags, tc.msg.Flags)
			}
			if !bytes.Equal(got.Body, tc.msg.Body) {
				t.Errorf("Body mismatch:\n got  %q\n want %q", got.Body, tc.msg.Body)
			}
		})
	}
}

// TestIPCFrameBadMagic verifies that readFrame rejects frames with wrong magic.
func TestIPCFrameBadMagic(t *testing.T) {
	// Corrupt the magic bytes of a valid frame.
	wire := encodeFrame(Message{Kind: KindDOMAck, Body: encodeU64(1)})
	wire[0] = 'X'
	if _, err := readFrame(bytes.NewReader(wire)); err == nil {
		t.Fatal("expected error for bad magic, got nil")
	}
}

// TestIPCFrameShortHeader verifies that readFrame rejects truncated headers.
func TestIPCFrameShortHeader(t *testing.T) {
	wire := encodeFrame(Message{Kind: KindDOMAck, Body: encodeU64(1)})
	if _, err := readFrame(bytes.NewReader(wire[:ipcHeaderLen-1])); err == nil {
		t.Fatal("expected error for short header, got nil")
	}
}

// TestEventBodyRoundTrip verifies encodeEventBody / decodeEventBody for all
// three event kinds defined in engine.
func TestEventBodyRoundTrip(t *testing.T) {
	cases := []struct {
		kind   uint32
		nodeID uint64
		value  string
	}{
		{engine.EventClick, 99, ""},
		{engine.EventInput, 512, "hello world"},
		{engine.EventKey, 1, "Enter"},
	}

	for _, tc := range cases {
		body := encodeEventBody(tc.kind, tc.nodeID, tc.value)
		kind, nodeID, value, err := decodeEventBody(body)
		if err != nil {
			t.Fatalf("decodeEventBody: %v", err)
		}
		if kind != tc.kind {
			t.Errorf("kind: got %d, want %d", kind, tc.kind)
		}
		if nodeID != tc.nodeID {
			t.Errorf("nodeID: got %d, want %d", nodeID, tc.nodeID)
		}
		if value != tc.value {
			t.Errorf("value: got %q, want %q", value, tc.value)
		}
	}
}

// TestDecodeEventBodyShort verifies that decodeEventBody rejects short input.
func TestDecodeEventBodyShort(t *testing.T) {
	if _, _, _, err := decodeEventBody([]byte{0, 1, 2}); err == nil {
		t.Fatal("expected error for short body, got nil")
	}
}

// ============================================================================
// 2. loopbackTransport integration
// ============================================================================

// buildTestBatch creates a small GDOM batch that appends a <div class="hello">
// text node under parent.
func buildTestBatch(baseRev uint64, parent protocol.NodeID) []byte {
	b := protocol.NewBatch(1, baseRev)
	const (
		divID  protocol.NodeID = 10
		textID protocol.NodeID = 11
	)
	b.CreateElement(divID, "div")
	b.SetAttr(divID, "class", "hello")
	b.CreateText(textID, "world")
	b.Append(divID, textID)
	b.Append(parent, divID)
	b.Commit()
	return b.Encode()
}

// TestLoopbackApplyAndHTML is the primary integration test:
//
//  1. Create a TestDOM and wire a loopback Engine to it.
//  2. Call LoadHTML (smoke-test; TestDOM accepts any HTML).
//  3. Call RootNode("#app") and verify the returned NodeID is non-zero.
//  4. Build and Apply a GDOM batch.
//  5. Assert the new revision is 1 and Engine.Revision() matches.
//  6. Assert dom.HTML() reflects the applied patch.
func TestLoopbackApplyAndHTML(t *testing.T) {
	dom := engine.NewTestDOM()
	defer dom.Close()

	eng, err := NewLoopback(dom)
	if err != nil {
		t.Fatalf("NewLoopback: %v", err)
	}
	defer eng.Close()

	// Step 1: LoadHTML.
	if err := eng.LoadHTML(`<html><body><div id="app"></div></body></html>`); err != nil {
		t.Fatalf("LoadHTML: %v", err)
	}

	// Step 2: RootNode.
	rootID, err := eng.RootNode("#app")
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}
	if rootID == 0 {
		t.Fatal("RootNode returned 0")
	}

	// Step 3: Apply a batch at baseRevision 0.
	batch := buildTestBatch(0, rootID)
	rev, err := eng.Apply(batch)
	if err != nil {
		t.Fatalf("Apply: %v", err)
	}
	if rev != 1 {
		t.Errorf("Apply returned revision %d, want 1", rev)
	}
	if eng.Revision() != 1 {
		t.Errorf("Engine.Revision() = %d, want 1", eng.Revision())
	}

	// Step 4: Verify the DOM tree via TestDOM.HTML().
	want := `<div id="app"><div class="hello">world</div></div>`
	if got := dom.HTML(); got != want {
		t.Errorf("dom.HTML():\n got  %s\n want %s", got, want)
	}
}

// TestLoopbackRevisionMismatch verifies that Apply rejects a stale batch.
func TestLoopbackRevisionMismatch(t *testing.T) {
	dom := engine.NewTestDOM()
	defer dom.Close()

	eng, err := NewLoopback(dom)
	if err != nil {
		t.Fatalf("NewLoopback: %v", err)
	}
	defer eng.Close()

	rootID, err := eng.RootNode("#app")
	if err != nil {
		t.Fatalf("RootNode: %v", err)
	}

	// First Apply succeeds; engine is now at revision 1.
	if _, err := eng.Apply(buildTestBatch(0, rootID)); err != nil {
		t.Fatalf("first Apply: %v", err)
	}

	// Second Apply at baseRevision 0 must fail with ErrRevisionMismatch.
	if _, err := eng.Apply(buildTestBatch(0, rootID)); err != engine.ErrRevisionMismatch {
		t.Fatalf("expected ErrRevisionMismatch, got: %v", err)
	}
}

// TestLoopbackEventDelivery verifies that an event injected into the TestDOM
// arrives on Engine.Events() with the correct fields.
func TestLoopbackEventDelivery(t *testing.T) {
	dom := engine.NewTestDOM()
	defer dom.Close()

	eng, err := NewLoopback(dom)
	if err != nil {
		t.Fatalf("NewLoopback: %v", err)
	}
	defer eng.Close()

	// Inject a click event targeting NodeID 42.
	want := engine.Event{Kind: engine.EventClick, Target: 42, Value: ""}
	dom.InjectEvent(want)

	got := <-eng.Events()
	if got.Kind != want.Kind {
		t.Errorf("Event.Kind: got %d, want %d", got.Kind, want.Kind)
	}
	if got.Target != want.Target {
		t.Errorf("Event.Target: got %d, want %d", got.Target, want.Target)
	}
	if got.Value != want.Value {
		t.Errorf("Event.Value: got %q, want %q", got.Value, want.Value)
	}
}

// TestLoopbackInputEventWithValue verifies an EventInput event carries the
// typed value through the encode/decode pipeline.
func TestLoopbackInputEventWithValue(t *testing.T) {
	dom := engine.NewTestDOM()
	defer dom.Close()

	eng, err := NewLoopback(dom)
	if err != nil {
		t.Fatalf("NewLoopback: %v", err)
	}
	defer eng.Close()

	dom.InjectEvent(engine.Event{Kind: engine.EventInput, Target: 99, Value: "typed text"})

	got := <-eng.Events()
	if got.Kind != engine.EventInput {
		t.Errorf("kind: got %d, want EventInput(%d)", got.Kind, engine.EventInput)
	}
	if got.Target != 99 {
		t.Errorf("target: got %d, want 99", got.Target)
	}
	if got.Value != "typed text" {
		t.Errorf("value: got %q, want %q", got.Value, "typed text")
	}
}

// TestNewWithEmptyBinaryPath verifies that New returns ErrNoBinaryConfigured
// when no BinaryPath is set, without panicking or blocking.
func TestNewWithEmptyBinaryPath(t *testing.T) {
	_, err := New(Options{})
	if err != ErrNoBinaryConfigured {
		t.Fatalf("expected ErrNoBinaryConfigured, got: %v", err)
	}
}
