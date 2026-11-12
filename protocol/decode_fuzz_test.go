package protocol

import "testing"

type nopVisitor struct{}

func (nopVisitor) CreateElement(NodeID, string)        {}
func (nopVisitor) CreateText(NodeID, string)           {}
func (nopVisitor) SetText(NodeID, string)              {}
func (nopVisitor) SetAttr(NodeID, string, string)      {}
func (nopVisitor) RemoveAttr(NodeID, string)           {}
func (nopVisitor) Append(NodeID, NodeID)               {}
func (nopVisitor) InsertBefore(NodeID, NodeID, NodeID) {}
func (nopVisitor) Remove(NodeID)                       {}
func (nopVisitor) SetValue(NodeID, string)             {}
func (nopVisitor) Commit()                             {}

// FuzzDecode: the GDOM decoder must reject malformed input with an error,
// never panic. Seeds are real batches; `go test -fuzz=FuzzDecode ./protocol`
// mutates them. Under plain `go test` only the seeds run.
func FuzzDecode(f *testing.F) {
	b := NewBatch(1, 0)
	b.CreateElement(1, "div")
	b.CreateText(2, "hello")
	b.SetAttr(1, "class", "x")
	b.Append(1, 2)
	b.InsertBefore(1, 2, 0)
	b.SetValue(1, "v")
	b.Remove(2)
	b.Commit()
	f.Add(b.Encode())
	f.Add(NewBatch(0, 0).Encode())
	f.Add([]byte("GDOM"))
	f.Add([]byte{})
	f.Fuzz(func(t *testing.T, data []byte) {
		_, _ = Decode(data, nopVisitor{})
	})
}
