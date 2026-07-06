package protocol

import "testing"

func TestEncodeRoundTripHeader(t *testing.T) {
	b := NewBatch(7, 42)
	// build: <div class="card">Hello</div> appended to root(1)
	const root NodeID = 1
	b.CreateElement(2, "div")
	b.SetAttr(2, "class", "card")
	b.CreateText(3, "Hello")
	b.Append(2, 3)
	b.Append(root, 2)
	b.Commit()

	buf := b.Encode()
	h, off, err := DecodeHeader(buf)
	if err != nil {
		t.Fatalf("DecodeHeader: %v", err)
	}
	if h.Version != Version {
		t.Errorf("version = %d, want %d", h.Version, Version)
	}
	if h.BatchID != 7 || h.BaseRevision != 42 {
		t.Errorf("ids = %d/%d, want 7/42", h.BatchID, h.BaseRevision)
	}
	if h.OpCount != 6 {
		t.Errorf("opCount = %d, want 6", h.OpCount)
	}
	// "div","class","card","Hello" interned once each.
	if h.StringCount != 4 {
		t.Errorf("stringCount = %d, want 4", h.StringCount)
	}
	if off <= 0 || off >= len(buf) {
		t.Errorf("string table offset %d out of range (len %d)", off, len(buf))
	}
}

func TestInternDedupes(t *testing.T) {
	b := NewBatch(1, 0)
	b.SetAttr(2, "class", "x")
	b.SetAttr(3, "class", "x") // same name+value -> reuse both strings
	buf := b.Encode()
	h, _, err := DecodeHeader(buf)
	if err != nil {
		t.Fatal(err)
	}
	if h.StringCount != 2 {
		t.Errorf("stringCount = %d, want 2 (deduped)", h.StringCount)
	}
}
