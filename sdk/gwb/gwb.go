// Package gwb is the low-level Go binding for the GWB ABI (docs/ABI.md).
// v0: enough surface for the click demo — batch builder, well-known atoms,
// click events. See docs/SDK.md for the tier model.
package gwb

import (
	"encoding/binary"
	"math"
)

// Root is the pre-defined mount root node id.
const Root uint32 = 1

// Well-known atoms (mirror of the host table in renderer/src/abi.rs).
const (
	Div      uint32 = 1
	Span     uint32 = 2
	P        uint32 = 3
	H1       uint32 = 4
	H2       uint32 = 5
	H3       uint32 = 6
	Button   uint32 = 7
	Input    uint32 = 8
	A        uint32 = 9
	Img      uint32 = 10
	Pre      uint32 = 26
	Code     uint32 = 27
	Strong   uint32 = 28
	Em       uint32 = 29

	AttrClass       uint32 = 100
	AttrID          uint32 = 101
	AttrStyle       uint32 = 102
	AttrHref        uint32 = 103
	AttrValue       uint32 = 104
	AttrType        uint32 = 105
	AttrPlaceholder uint32 = 106

	StyleDisplay      uint32 = 200
	StyleColor        uint32 = 201
	StyleBackground   uint32 = 202
	StyleWidth        uint32 = 203
	StyleHeight       uint32 = 204
	StyleMargin       uint32 = 205
	StylePadding      uint32 = 206
	StyleBorder       uint32 = 207
	StyleFontSize     uint32 = 208
	StyleFontWeight   uint32 = 209
	StyleGap          uint32 = 210
	StyleFlexDir      uint32 = 211
	StyleAlignItems   uint32 = 212
	StyleJustify      uint32 = 213
	StyleBorderRadius uint32 = 214
	StyleCursor       uint32 = 215
	StyleTextAlign    uint32 = 216
)

// Event kinds.
const (
	EvClick uint16 = 4
)

// Log levels.
const (
	LogDebug uint32 = 0
	LogInfo  uint32 = 1
	LogWarn  uint32 = 2
	LogError uint32 = 3
)

const noStr uint32 = 0xFFFFFFFF

// App callbacks, set in init() before the host calls gwb_start.
var (
	// OnStart builds the initial DOM. The default batch auto-flushes after.
	OnStart func(w, h, scale float32, flags uint32)
	// OnEvent handles one event record; return ABI flags (0 for none).
	OnEvent func(e *Event) uint32
)

// Event is a decoded event record.
type Event struct {
	Kind     uint16
	Flags    uint16
	Target   uint32
	Listener uint32
	TimeMS   float64
	X, Y     float32
	Buttons  uint16
	Mods     uint16
	Str      string
}

// eventBuf is the registered event region.
var eventBuf [8192]byte

var nextID uint32 = 1 // 1 is Root

// NewID allocates a fresh guest node id.
func NewID() uint32 {
	nextID++
	return nextID
}

// ---------------------------------------------------------------- batch

// Batch builds GWB1 write batches. The zero value is ready to use and can be
// reused after Submit.
type Batch struct {
	ops   []byte
	heap  []byte
	count uint32
}

func (b *Batch) op(code, flags byte, a uint16, x, y, z uint32) {
	var rec [16]byte
	rec[0] = code
	rec[1] = flags
	binary.LittleEndian.PutUint16(rec[2:4], a)
	binary.LittleEndian.PutUint32(rec[4:8], x)
	binary.LittleEndian.PutUint32(rec[8:12], y)
	binary.LittleEndian.PutUint32(rec[12:16], z)
	b.ops = append(b.ops, rec[:]...)
	b.count++
}

func (b *Batch) str(s string) uint32 {
	off := uint32(len(b.heap))
	var l [4]byte
	binary.LittleEndian.PutUint32(l[:], uint32(len(s)))
	b.heap = append(b.heap, l[:]...)
	b.heap = append(b.heap, s...)
	for len(b.heap)%4 != 0 {
		b.heap = append(b.heap, 0)
	}
	return off
}

func (b *Batch) CreateElement(id, tag uint32)           { b.op(1, 0, 0, id, tag, noStr) }
func (b *Batch) CreateText(id uint32, text string)      { b.op(2, 0, 0, id, 0, b.str(text)) }
func (b *Batch) SetAttr(id, name uint32, value string)  { b.op(3, 0, 0, id, name, b.str(value)) }
func (b *Batch) RemoveAttr(id, name uint32)             { b.op(4, 0, 0, id, name, noStr) }
func (b *Batch) SetText(id uint32, text string)         { b.op(5, 0, 0, id, 0, b.str(text)) }
func (b *Batch) SetStyle(id, prop uint32, value string) { b.op(6, 0, 0, id, prop, b.str(value)) }
func (b *Batch) RemoveStyle(id, prop uint32)            { b.op(7, 0, 0, id, prop, noStr) }
func (b *Batch) AppendChild(parent, child uint32)       { b.op(8, 0, 0, parent, child, noStr) }
func (b *Batch) Remove(id uint32)                       { b.op(10, 0, 0, id, 0, noStr) }
func (b *Batch) Clear(id uint32)                        { b.op(12, 0, 0, id, 0, noStr) }
func (b *Batch) SetInnerHTML(id uint32, html string)    { b.op(13, 0, 0, id, 0, b.str(html)) }
func (b *Batch) DefineAtom(atom uint32, name string)    { b.op(14, 0, 0, atom, 0, b.str(name)) }
func (b *Batch) Listen(id uint32, kind uint16)          { b.op(15, 0, kind, id, 0, noStr) }
func (b *Batch) Unlisten(id uint32, kind uint16)        { b.op(16, 0, kind, id, 0, noStr) }

// Submit encodes and submits the batch, then resets it for reuse.
func (b *Batch) Submit() uint32 {
	if b.count == 0 {
		return 0
	}
	heapOff := uint32(16 + len(b.ops))
	buf := make([]byte, 0, 16+len(b.ops)+len(b.heap))
	buf = append(buf, 'G', 'W', 'B', '1')
	buf = binary.LittleEndian.AppendUint32(buf, b.count)
	buf = binary.LittleEndian.AppendUint32(buf, heapOff)
	buf = binary.LittleEndian.AppendUint32(buf, uint32(len(b.heap)))
	buf = append(buf, b.ops...)
	buf = append(buf, b.heap...)
	status := hostSubmit(buf)
	b.ops = b.ops[:0]
	b.heap = b.heap[:0]
	b.count = 0
	return status
}

// Default batch + package-level convenience wrappers (auto-flushed after
// OnStart/OnEvent return).
var def Batch

func CreateElement(id, tag uint32)           { def.CreateElement(id, tag) }
func CreateText(id uint32, text string)      { def.CreateText(id, text) }
func SetAttr(id, name uint32, value string)  { def.SetAttr(id, name, value) }
func SetText(id uint32, text string)         { def.SetText(id, text) }
func SetStyle(id, prop uint32, value string) { def.SetStyle(id, prop, value) }
func AppendChild(parent, child uint32)       { def.AppendChild(parent, child) }
func Remove(id uint32)                       { def.Remove(id) }
func Clear(id uint32)                        { def.Clear(id) }
func SetInnerHTML(id uint32, html string)    { def.SetInnerHTML(id, html) }
func DefineAtom(atom uint32, name string)    { def.DefineAtom(atom, name) }
func Listen(id uint32, kind uint16)          { def.Listen(id, kind) }

// Flush submits the default batch (normally automatic).
func Flush() { def.Submit() }

// ---------------------------------------------------------------- dispatch
// Called by the platform layer (platform_wasip1.go).

func dispatchStart(w, h, scale float32, flags uint32) {
	if OnStart != nil {
		OnStart(w, h, scale, flags)
	}
	Flush()
}

func dispatchEvents(count uint32) uint32 {
	var ret uint32
	off := 0
	for i := uint32(0); i < count; i++ {
		if off+40 > len(eventBuf) {
			break
		}
		r := eventBuf[off:]
		e := Event{
			Kind:     binary.LittleEndian.Uint16(r[0:2]),
			Flags:    binary.LittleEndian.Uint16(r[2:4]),
			Target:   binary.LittleEndian.Uint32(r[4:8]),
			Listener: binary.LittleEndian.Uint32(r[8:12]),
			TimeMS:   math.Float64frombits(binary.LittleEndian.Uint64(r[12:20])),
			X:        math.Float32frombits(binary.LittleEndian.Uint32(r[20:24])),
			Y:        math.Float32frombits(binary.LittleEndian.Uint32(r[24:28])),
			Buttons:  binary.LittleEndian.Uint16(r[28:30]),
			Mods:     binary.LittleEndian.Uint16(r[30:32]),
		}
		strLen := int(binary.LittleEndian.Uint32(r[36:40]))
		next := off + 40
		if strLen > 0 && next+strLen <= len(eventBuf) {
			e.Str = string(eventBuf[next : next+strLen])
			next += strLen
			for next%4 != 0 {
				next++
			}
		}
		off = next
		if OnEvent != nil {
			ret |= OnEvent(&e)
		}
	}
	Flush()
	return ret
}
