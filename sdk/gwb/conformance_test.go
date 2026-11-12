package gwb

// GWB1 conformance vectors shared with the renderer's Rust decoder.
//
// This test builds one batch that exercises every op code through the Go SDK
// encoder and checks it against testdata/abi/all-ops.bin. The expected decode
// (one op per line) lives next to it in all-ops.txt; renderer/src/abi_tests.rs
// decodes the same .bin and must produce exactly those lines. Any drift
// between this encoder and the host decoder fails one side or the other.
//
// After an intentional wire change, regenerate with:
//
//	go test ./sdk/gwb -run TestConformanceVectors -update

import (
	"bytes"
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var update = flag.Bool("update", false, "rewrite testdata/abi conformance vectors")

func conformanceBatch() (*Batch, []string) {
	var b Batch
	var want []string
	add := func(line string) { want = append(want, line) }

	b.DefineAtom(1024, "text-decoration")
	add(`DefineAtom atom=1024 name="text-decoration"`)
	b.CreateElement(2, 1)
	add(`CreateElement id=2 tag=1`)
	b.CreateText(3, "héllo, wörld")
	add(`CreateText id=3 text="héllo, wörld"`)
	b.SetAttr(2, 100, "card primary")
	add(`SetAttr id=2 name=100 value="card primary"`)
	b.RemoveAttr(2, 101)
	add(`RemoveAttr id=2 name=101`)
	b.SetText(3, "")
	add(`SetText id=3 text=""`)
	b.SetStyle(2, 1024, "line-through")
	add(`SetStyle id=2 prop=1024 value="line-through"`)
	b.RemoveStyle(2, 201)
	add(`RemoveStyle id=2 prop=201`)
	b.AppendChild(1, 2)
	add(`AppendChild parent=1 child=2`)
	b.InsertBefore(1, 3, 2)
	add(`InsertBefore child=3 before=2`)
	b.Remove(3)
	add(`Remove id=3`)
	b.ReplaceWith(2, 4)
	add(`ReplaceWith old=2 new=4`)
	b.Clear(4)
	add(`Clear id=4`)
	b.SetInnerHTML(4, "<b>bold</b> & \"quoted\"")
	add(`SetInnerHtml id=4 html="<b>bold</b> & \"quoted\""`)
	b.Listen(4, 4)
	add(`Listen id=4 kind=4`)
	b.Unlisten(4, 6)
	add(`Unlisten id=4 kind=6`)
	b.Observe(4, 1)
	add(`Observe id=4 what=1`)
	b.Unobserve(4, 1)
	add(`Unobserve id=4 what=1`)
	b.Focus(4)
	add(`Focus id=4`)
	return &b, want
}

func TestConformanceVectors(t *testing.T) {
	dir := filepath.Join("..", "..", "testdata", "abi")
	b, want := conformanceBatch()
	got := b.Encode()
	wantTxt := strings.Join(want, "\n") + "\n"

	binPath := filepath.Join(dir, "all-ops.bin")
	txtPath := filepath.Join(dir, "all-ops.txt")
	if *update {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(binPath, got, 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(txtPath, []byte(wantTxt), 0o644); err != nil {
			t.Fatal(err)
		}
		t.Logf("wrote %s (%d bytes) and %s", binPath, len(got), txtPath)
		return
	}

	committed, err := os.ReadFile(binPath)
	if err != nil {
		t.Fatalf("missing vector (run with -update): %v", err)
	}
	if !bytes.Equal(committed, got) {
		t.Fatalf("Go encoder output differs from %s; if the wire change is intentional, rerun with -update", binPath)
	}
	committedTxt, err := os.ReadFile(txtPath)
	if err != nil {
		t.Fatal(err)
	}
	if strings.ReplaceAll(string(committedTxt), "\r\n", "\n") != wantTxt {
		t.Fatalf("%s is out of date; rerun with -update", txtPath)
	}
}

// Header sanity independent of the vector files.
func TestEncodeHeader(t *testing.T) {
	b, want := conformanceBatch()
	buf := b.Encode()
	if string(buf[:4]) != "GWB1" {
		t.Fatalf("magic %q", buf[:4])
	}
	count := uint32(buf[4]) | uint32(buf[5])<<8 | uint32(buf[6])<<16 | uint32(buf[7])<<24
	if int(count) != len(want) {
		t.Fatalf("op count %d, want %d", count, len(want))
	}
	heapOff := uint32(buf[8]) | uint32(buf[9])<<8 | uint32(buf[10])<<16 | uint32(buf[11])<<24
	if int(heapOff) != 16+16*len(want) {
		t.Fatalf("heap offset %d", heapOff)
	}
}
