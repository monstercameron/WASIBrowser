package protocol

import (
	"encoding/binary"
	"fmt"
)

// Visitor receives decoded ops from a batch. Engines (testdom, and the C++
// WebKit agent mirrors this) implement it. Method set must stay in sync with the
// op builders in protocol.go.
type Visitor interface {
	CreateElement(node NodeID, tag string)
	CreateText(node NodeID, text string)
	SetText(node NodeID, text string)
	SetAttr(node NodeID, name, value string)
	RemoveAttr(node NodeID, name string)
	Append(parent, child NodeID)
	InsertBefore(parent, child, ref NodeID)
	Remove(node NodeID)
	SetValue(node NodeID, value string)
	Commit()
}

type reader struct {
	b   []byte
	pos int
}

func (r *reader) need(n int) error {
	if r.pos+n > len(r.b) {
		return ErrShort
	}
	return nil
}

func (r *reader) u16() (uint16, error) {
	if err := r.need(2); err != nil {
		return 0, err
	}
	v := binary.LittleEndian.Uint16(r.b[r.pos:])
	r.pos += 2
	return v, nil
}

func (r *reader) u32() (uint32, error) {
	if err := r.need(4); err != nil {
		return 0, err
	}
	v := binary.LittleEndian.Uint32(r.b[r.pos:])
	r.pos += 4
	return v, nil
}

func (r *reader) u64() (uint64, error) {
	if err := r.need(8); err != nil {
		return 0, err
	}
	v := binary.LittleEndian.Uint64(r.b[r.pos:])
	r.pos += 8
	return v, nil
}

// Decode parses a GDOM batch and dispatches every op to v. It does not gate on
// revision — callers check Header.BaseRevision first (see DecodeHeader).
func Decode(buf []byte, v Visitor) (Header, error) {
	h, off, err := DecodeHeader(buf)
	if err != nil {
		return h, err
	}

	// String table.
	r := &reader{b: buf, pos: off}
	offsets := make([]uint32, h.StringCount)
	for i := range offsets {
		o, err := r.u32()
		if err != nil {
			return h, err
		}
		offsets[i] = o
	}
	if err := r.need(int(h.StringBytes)); err != nil {
		return h, err
	}
	blob := buf[r.pos : r.pos+int(h.StringBytes)]
	r.pos += int(h.StringBytes)

	str := func(i uint32) (string, error) {
		if i >= h.StringCount {
			return "", fmt.Errorf("protocol: string index %d out of range", i)
		}
		start := offsets[i]
		stop := h.StringBytes
		if int(i)+1 < len(offsets) {
			stop = offsets[i+1]
		}
		if start > stop || stop > h.StringBytes {
			return "", ErrShort
		}
		return string(blob[start:stop]), nil
	}

	node := func() (NodeID, error) { u, err := r.u64(); return NodeID(u), err }

	for i := uint32(0); i < h.OpCount; i++ {
		code, err := r.u16()
		if err != nil {
			return h, err
		}
		switch Op(code) {
		case OpCreateElement:
			n, e1 := node()
			t, e2 := r.u32()
			tag, e3 := str(t)
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.CreateElement(n, tag)
		case OpCreateText:
			n, e1 := node()
			t, e2 := r.u32()
			text, e3 := str(t)
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.CreateText(n, text)
		case OpSetText:
			n, e1 := node()
			t, e2 := r.u32()
			text, e3 := str(t)
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.SetText(n, text)
		case OpSetAttr:
			n, e1 := node()
			ni, e2 := r.u32()
			vi, e3 := r.u32()
			name, e4 := str(ni)
			val, e5 := str(vi)
			if err := firstErr(e1, e2, e3, e4, e5); err != nil {
				return h, err
			}
			v.SetAttr(n, name, val)
		case OpRemoveAttr:
			n, e1 := node()
			ni, e2 := r.u32()
			name, e3 := str(ni)
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.RemoveAttr(n, name)
		case OpAppend:
			p, e1 := node()
			c, e2 := node()
			if err := firstErr(e1, e2); err != nil {
				return h, err
			}
			v.Append(p, c)
		case OpInsertBefore:
			p, e1 := node()
			c, e2 := node()
			ref, e3 := node()
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.InsertBefore(p, c, ref)
		case OpRemove:
			n, e1 := node()
			if e1 != nil {
				return h, e1
			}
			v.Remove(n)
		case OpSetValue:
			n, e1 := node()
			vi, e2 := r.u32()
			val, e3 := str(vi)
			if err := firstErr(e1, e2, e3); err != nil {
				return h, err
			}
			v.SetValue(n, val)
		case OpCommit:
			v.Commit()
		default:
			return h, fmt.Errorf("protocol: unhandled op %d (operand layout unknown, resync)", code)
		}
	}
	return h, nil
}

func firstErr(errs ...error) error {
	for _, e := range errs {
		if e != nil {
			return e
		}
	}
	return nil
}
