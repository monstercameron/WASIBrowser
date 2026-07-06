package engine

import (
	"testing"

	"github.com/monstercameron/gowebbrowser/protocol"
)

// buildCounterFrame renders <div class="card"><button>Count: N</button></div>
// under the root, as a single GDOM batch.
func buildCounterFrame(baseRev uint64, root protocol.NodeID, label string) []byte {
	b := protocol.NewBatch(1, baseRev)
	const (
		div protocol.NodeID = 10
		btn protocol.NodeID = 11
		txt protocol.NodeID = 12
	)
	b.CreateElement(div, "div")
	b.SetAttr(div, "class", "card")
	b.CreateElement(btn, "button")
	b.CreateText(txt, label)
	b.Append(btn, txt)
	b.Append(div, btn)
	b.Append(root, div)
	b.Commit()
	return b.Encode()
}

func TestApplyBuildsTree(t *testing.T) {
	d := NewTestDOM()
	root, err := d.RootNode("#app")
	if err != nil {
		t.Fatal(err)
	}

	rev, err := d.Apply(buildCounterFrame(0, root, "Count: 0"))
	if err != nil {
		t.Fatalf("Apply: %v", err)
	}
	if rev != 1 {
		t.Fatalf("revision = %d, want 1", rev)
	}

	want := `<div id="app"><div class="card"><button>Count: 0</button></div></div>`
	if got := d.HTML(); got != want {
		t.Errorf("HTML mismatch:\n got: %s\nwant: %s", got, want)
	}
}

func TestRevisionGating(t *testing.T) {
	d := NewTestDOM()
	root, _ := d.RootNode("#app")
	if _, err := d.Apply(buildCounterFrame(0, root, "Count: 0")); err != nil {
		t.Fatal(err)
	}
	// Engine is now at revision 1; a batch built against base 0 is stale.
	if _, err := d.Apply(buildCounterFrame(0, root, "Count: 1")); err != ErrRevisionMismatch {
		t.Fatalf("err = %v, want ErrRevisionMismatch", err)
	}
}

func TestRemoveAndReplaceSubtree(t *testing.T) {
	d := NewTestDOM()
	root, _ := d.RootNode("#app")
	if _, err := d.Apply(buildCounterFrame(0, root, "Count: 0")); err != nil {
		t.Fatal(err)
	}

	// New frame at the current revision: remove old div(10), mount a fresh one.
	b := protocol.NewBatch(2, d.Revision())
	b.Remove(10)
	const div2 protocol.NodeID = 20
	b.CreateElement(div2, "p")
	b.CreateText(21, "replaced")
	b.Append(div2, 21)
	b.Append(root, div2)
	b.Commit()
	if _, err := d.Apply(b.Encode()); err != nil {
		t.Fatal(err)
	}

	want := `<div id="app"><p>replaced</p></div>`
	if got := d.HTML(); got != want {
		t.Errorf("HTML mismatch:\n got: %s\nwant: %s", got, want)
	}
}
