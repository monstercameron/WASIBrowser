// Package eventmsg encodes and decodes host-to-app event messages over the
// WASI linear-memory channel. The wire format is little-endian:
//
//	kind     uint32   (engine.EventClick=1, EventInput=2, EventKey=3)
//	target   uint64   (protocol.NodeID)
//	valueLen uint32
//	value    [valueLen]byte  (UTF-8)
package eventmsg

import (
	"encoding/binary"
	"fmt"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/protocol"
)

const headerSize = 4 + 8 + 4 // kind u32 + target u64 + valueLen u32

// Encode serialises an engine.Event into the wire format.
func Encode(e engine.Event) []byte {
	buf := make([]byte, headerSize+len(e.Value))
	binary.LittleEndian.PutUint32(buf[0:], e.Kind)
	binary.LittleEndian.PutUint64(buf[4:], uint64(e.Target))
	binary.LittleEndian.PutUint32(buf[12:], uint32(len(e.Value)))
	copy(buf[headerSize:], e.Value)
	return buf
}

// Decode deserialises an engine.Event from the wire format.
func Decode(buf []byte) (engine.Event, error) {
	if len(buf) < headerSize {
		return engine.Event{}, fmt.Errorf("eventmsg: buffer too short (%d bytes)", len(buf))
	}
	kind := binary.LittleEndian.Uint32(buf[0:])
	target := binary.LittleEndian.Uint64(buf[4:])
	vlen := binary.LittleEndian.Uint32(buf[12:])
	if uint32(len(buf)-headerSize) < vlen {
		return engine.Event{}, fmt.Errorf("eventmsg: value length %d exceeds buffer", vlen)
	}
	return engine.Event{
		Kind:   kind,
		Target: protocol.NodeID(target),
		Value:  string(buf[headerSize : headerSize+int(vlen)]),
	}, nil
}
