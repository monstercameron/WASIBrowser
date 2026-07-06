// Package webkitengine implements engine.Engine backed by the WebKit2/WinCairo
// web-process DOM agent. It communicates with the agent via a lightweight
// length-prefixed binary IPC channel whose framing is defined here.
//
// # IPC frame layout (all little-endian)
//
//	[4]byte  magic    = "GWBI"   // GoWebBrowser IPC
//	uint16   kind                // MessageKind
//	uint16   flags               // reserved, must be 0
//	uint32   bodyLen             // byte count of body that follows
//	[bodyLen]byte body
//
// Total header size: 12 bytes.
//
// # MessageKind bodies
//
//	KindDOMBatch  (Go→WebKit): raw GDOM batch bytes (see protocol/).
//	KindDOMAck    (WebKit→Go): uint64 newRevision; 0 == NACK (revision mismatch).
//	KindEvent     (WebKit→Go): uint32 kind, uint64 nodeID, uint32 valueLen, []byte value.
//	KindLoadHTML  (Go→WebKit): UTF-8 HTML string; length given by bodyLen.
//	KindMount     (Go→WebKit): UTF-8 CSS selector string (e.g. "#app").
//	KindMountAck  (WebKit→Go): uint64 nodeID; 0 == selector not found.
package webkitengine

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
)

// ipcMagic is the 4-byte frame prefix, chosen to be distinct from the GDOM
// batch magic ("GDOM") so a misrouted batch is immediately detectable.
const ipcMagic = "GWBI"

// ipcHeaderLen is the fixed header size in bytes.
const ipcHeaderLen = 4 + 2 + 2 + 4 // magic + kind + flags + bodyLen

// MessageKind identifies the purpose of an IPC frame.
type MessageKind uint16

const (
	// KindDOMBatch carries a raw GDOM batch from the Go broker to the
	// web-process DOM agent.
	KindDOMBatch MessageKind = 1

	// KindDOMAck is the agent's reply to KindDOMBatch. Body is a uint64
	// new revision; 0 means NACK (stale base revision or decode error).
	KindDOMAck MessageKind = 2

	// KindEvent carries an input event from the agent to the Go broker.
	// Body: uint32 kind, uint64 nodeID, uint32 valueLen, []byte value.
	KindEvent MessageKind = 3

	// KindLoadHTML asks the agent to load a new host document.
	// Body: UTF-8 HTML string.
	KindLoadHTML MessageKind = 4

	// KindMount asks the agent to resolve a CSS selector to a NodeID.
	// Body: UTF-8 selector string.
	KindMount MessageKind = 5

	// KindMountAck is the agent's reply to KindMount.
	// Body: uint64 nodeID; 0 means not found.
	KindMountAck MessageKind = 6
)

// errBadIPCMagic is returned when a frame does not start with ipcMagic.
var errBadIPCMagic = errors.New("webkitengine: bad IPC frame magic")

// Message is a decoded IPC frame.
type Message struct {
	Kind  MessageKind
	Flags uint16 // reserved
	Body  []byte
}

// encodeFrame serializes m into the wire format: header + body.
func encodeFrame(m Message) []byte {
	out := make([]byte, ipcHeaderLen, ipcHeaderLen+len(m.Body))
	copy(out[0:4], ipcMagic)
	binary.LittleEndian.PutUint16(out[4:], uint16(m.Kind))
	binary.LittleEndian.PutUint16(out[6:], m.Flags)
	binary.LittleEndian.PutUint32(out[8:], uint32(len(m.Body)))
	return append(out, m.Body...)
}

// readFrame reads exactly one IPC frame from r, blocking until the full frame
// arrives or an error occurs.
func readFrame(r io.Reader) (Message, error) {
	var hdr [ipcHeaderLen]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return Message{}, err
	}
	if string(hdr[0:4]) != ipcMagic {
		return Message{}, errBadIPCMagic
	}
	kind := MessageKind(binary.LittleEndian.Uint16(hdr[4:]))
	flags := binary.LittleEndian.Uint16(hdr[6:])
	bodyLen := binary.LittleEndian.Uint32(hdr[8:])
	body := make([]byte, bodyLen)
	if _, err := io.ReadFull(r, body); err != nil {
		return Message{}, fmt.Errorf("webkitengine: reading frame body (kind=%d len=%d): %w",
			kind, bodyLen, err)
	}
	return Message{Kind: kind, Flags: flags, Body: body}, nil
}

// --- Event body codec -------------------------------------------------------

// encodeEventBody packs a KindEvent body.
//
//	uint32 kind
//	uint64 nodeID
//	uint32 valueLen
//	[valueLen]byte value (UTF-8)
func encodeEventBody(kind uint32, nodeID uint64, value string) []byte {
	vb := []byte(value)
	out := make([]byte, 4+8+4+len(vb))
	binary.LittleEndian.PutUint32(out[0:], kind)
	binary.LittleEndian.PutUint64(out[4:], nodeID)
	binary.LittleEndian.PutUint32(out[12:], uint32(len(vb)))
	copy(out[16:], vb)
	return out
}

// decodeEventBody unpacks a KindEvent body.
func decodeEventBody(body []byte) (kind uint32, nodeID uint64, value string, err error) {
	const minLen = 4 + 8 + 4
	if len(body) < minLen {
		return 0, 0, "", errors.New("webkitengine: KindEvent body too short")
	}
	kind = binary.LittleEndian.Uint32(body[0:])
	nodeID = binary.LittleEndian.Uint64(body[4:])
	vLen := binary.LittleEndian.Uint32(body[12:])
	if uint32(len(body)-minLen) < vLen {
		return 0, 0, "", errors.New("webkitengine: KindEvent value truncated")
	}
	value = string(body[minLen : minLen+int(vLen)])
	return kind, nodeID, value, nil
}

// --- Scalar helpers ---------------------------------------------------------

// encodeU64 packs v into a little-endian 8-byte slice.
func encodeU64(v uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, v)
	return b
}

// decodeU64 unpacks a little-endian uint64 from the first 8 bytes of b.
func decodeU64(b []byte) (uint64, error) {
	if len(b) < 8 {
		return 0, errors.New("webkitengine: too short to decode uint64")
	}
	return binary.LittleEndian.Uint64(b), nil
}
