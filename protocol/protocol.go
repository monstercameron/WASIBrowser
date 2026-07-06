// Package protocol defines the binary DOM-patch wire format that the Go/WASM app
// emits and the WebKit web-process DOM agent consumes. No JavaScript is involved:
// the agent decodes these bytes and calls WebCore DOM methods directly.
//
// Wire layout (all little-endian):
//
//	Header
//	  magic         [4]byte = "GDOM"
//	  version       u16
//	  flags         u16
//	  batchID       u64
//	  baseRevision  u64      // batch applies only if engine is at this revision
//	  opCount       u32
//	  stringCount   u32
//	  stringBytes   u32
//	StringTable
//	  offsets       [stringCount]u32   // start offset of each string in blob
//	  blob          [stringBytes]byte  // concatenated UTF-8, no separators
//	Ops
//	  packed records, one per op (see encode* below)
//
// NodeIDs are allocated by the app (guest) as a monotonic u64 space. The agent
// keeps a NodeID -> live DOM node map. NodeID 0 is reserved for "null".
package protocol

import (
	"encoding/binary"
	"errors"
	"math"
)

const Magic = "GDOM"
const Version uint16 = 1

// NodeID is an app-allocated handle for a DOM node. 0 == null/none.
type NodeID uint64

const NullNode NodeID = 0

// Op is the wire opcode. Keep in sync with the C++ agent's enum.
type Op uint16

const (
	OpCreateElement Op = iota + 1
	OpCreateElementNS
	OpCreateText
	OpSetText
	OpSetAttr
	OpRemoveAttr
	OpSetClass
	OpSetStyle
	OpAppend
	OpInsertBefore
	OpRemove
	OpReplace
	OpSetOuterHTML
	OpSubscribeEvent
	OpUnsubscribeEvent
	OpFocus
	OpBlur
	OpSetValue
	OpSetChecked
	OpRequestMeasure
	OpCommit
)

// Batch accumulates ops and interns strings, then serializes to the wire format.
type Batch struct {
	BatchID      uint64
	BaseRevision uint64
	Flags        uint16

	ops     []byte
	opCount uint32

	strOffsets []uint32
	strBlob    []byte
	strIndex   map[string]uint32
}

func NewBatch(batchID, baseRevision uint64) *Batch {
	return &Batch{
		BatchID:      batchID,
		BaseRevision: baseRevision,
		strIndex:     make(map[string]uint32),
	}
}

// intern returns the string-table index for s, adding it if new.
func (b *Batch) intern(s string) uint32 {
	if i, ok := b.strIndex[s]; ok {
		return i
	}
	i := uint32(len(b.strOffsets))
	b.strOffsets = append(b.strOffsets, uint32(len(b.strBlob)))
	b.strBlob = append(b.strBlob, s...)
	b.strIndex[s] = i
	return i
}

func (b *Batch) op(code Op) {
	b.ops = binary.LittleEndian.AppendUint16(b.ops, uint16(code))
	b.opCount++
}

func (b *Batch) u32(v uint32) { b.ops = binary.LittleEndian.AppendUint32(b.ops, v) }
func (b *Batch) u64(v uint64) { b.ops = binary.LittleEndian.AppendUint64(b.ops, v) }
func (b *Batch) node(n NodeID) { b.u64(uint64(n)) }
func (b *Batch) str(s string)  { b.u32(b.intern(s)) }

// --- op builders -----------------------------------------------------------

// CreateElement: node = createElement(tag)
func (b *Batch) CreateElement(node NodeID, tag string) {
	b.op(OpCreateElement)
	b.node(node)
	b.str(tag)
}

// CreateText: node = createTextNode(text)
func (b *Batch) CreateText(node NodeID, text string) {
	b.op(OpCreateText)
	b.node(node)
	b.str(text)
}

// SetText: node.data = text
func (b *Batch) SetText(node NodeID, text string) {
	b.op(OpSetText)
	b.node(node)
	b.str(text)
}

// SetAttr: node.setAttribute(name, value)
func (b *Batch) SetAttr(node NodeID, name, value string) {
	b.op(OpSetAttr)
	b.node(node)
	b.str(name)
	b.str(value)
}

// RemoveAttr: node.removeAttribute(name)
func (b *Batch) RemoveAttr(node NodeID, name string) {
	b.op(OpRemoveAttr)
	b.node(node)
	b.str(name)
}

// Append: parent.appendChild(child)
func (b *Batch) Append(parent, child NodeID) {
	b.op(OpAppend)
	b.node(parent)
	b.node(child)
}

// InsertBefore: parent.insertBefore(child, ref). ref==NullNode appends.
func (b *Batch) InsertBefore(parent, child, ref NodeID) {
	b.op(OpInsertBefore)
	b.node(parent)
	b.node(child)
	b.node(ref)
}

// Remove: node.remove()
func (b *Batch) Remove(node NodeID) {
	b.op(OpRemove)
	b.node(node)
}

// SetValue: form control value (input/textarea/select).
func (b *Batch) SetValue(node NodeID, value string) {
	b.op(OpSetValue)
	b.node(node)
	b.str(value)
}

// Commit marks the end of a frame. The agent flushes and bumps its revision.
func (b *Batch) Commit() { b.op(OpCommit) }

// Encode serializes the batch to the wire format.
func (b *Batch) Encode() []byte {
	headerLen := 4 + 2 + 2 + 8 + 8 + 4 + 4 + 4
	tableLen := len(b.strOffsets)*4 + len(b.strBlob)
	out := make([]byte, 0, headerLen+tableLen+len(b.ops))

	out = append(out, Magic...)
	out = binary.LittleEndian.AppendUint16(out, Version)
	out = binary.LittleEndian.AppendUint16(out, b.Flags)
	out = binary.LittleEndian.AppendUint64(out, b.BatchID)
	out = binary.LittleEndian.AppendUint64(out, b.BaseRevision)
	out = binary.LittleEndian.AppendUint32(out, b.opCount)
	out = binary.LittleEndian.AppendUint32(out, uint32(len(b.strOffsets)))
	out = binary.LittleEndian.AppendUint32(out, uint32(len(b.strBlob)))

	for _, off := range b.strOffsets {
		out = binary.LittleEndian.AppendUint32(out, off)
	}
	out = append(out, b.strBlob...)
	out = append(out, b.ops...)
	return out
}

// --- decode header (agent side will mirror the op loop in C++) -------------

var ErrBadMagic = errors.New("protocol: bad magic")
var ErrShort = errors.New("protocol: short buffer")

type Header struct {
	Version      uint16
	Flags        uint16
	BatchID      uint64
	BaseRevision uint64
	OpCount      uint32
	StringCount  uint32
	StringBytes  uint32
}

// DecodeHeader reads the fixed header and returns the offset where the string
// table begins.
func DecodeHeader(buf []byte) (Header, int, error) {
	const n = 4 + 2 + 2 + 8 + 8 + 4 + 4 + 4
	if len(buf) < n {
		return Header{}, 0, ErrShort
	}
	if string(buf[0:4]) != Magic {
		return Header{}, 0, ErrBadMagic
	}
	h := Header{
		Version:      binary.LittleEndian.Uint16(buf[4:]),
		Flags:        binary.LittleEndian.Uint16(buf[6:]),
		BatchID:      binary.LittleEndian.Uint64(buf[8:]),
		BaseRevision: binary.LittleEndian.Uint64(buf[16:]),
		OpCount:      binary.LittleEndian.Uint32(buf[24:]),
		StringCount:  binary.LittleEndian.Uint32(buf[28:]),
		StringBytes:  binary.LittleEndian.Uint32(buf[32:]),
	}
	if h.StringCount > math.MaxInt32 {
		return Header{}, 0, ErrShort
	}
	return h, n, nil
}
