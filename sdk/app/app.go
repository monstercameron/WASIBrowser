// Package app provides a minimal retained-mode UI layer for wasm apps built on
// the GoWebBrowser runtime. Apps declare a tree with El/Text/Attr/OnClick and
// call Run; the runtime re-renders on every dispatched event.
package app

import (
	"sync/atomic"

	"github.com/monstercameron/gowebbrowser/protocol"
	"github.com/monstercameron/gowebbrowser/sdk/dom"
)

// ---------------------------------------------------------------------------
// Virtual-node API
// ---------------------------------------------------------------------------

// Attr sets an HTML attribute on the enclosing element.
type Attr struct{ Name, Value string }

// OnClick registers a click handler on the enclosing element.
type OnClick struct{ Fn func() }

// Node is one node in the virtual tree — either an element or a text leaf.
type Node struct {
	tag      string
	text     string    // text nodes only
	attrs    []Attr
	handlers []func()  // click handlers
	children []*Node
}

// El creates a virtual element node. parts may contain *Node children, Attr
// values, or OnClick values.
func El(tag string, parts ...any) *Node {
	n := &Node{tag: tag}
	for _, p := range parts {
		switch v := p.(type) {
		case *Node:
			n.children = append(n.children, v)
		case Attr:
			n.attrs = append(n.attrs, v)
		case OnClick:
			n.handlers = append(n.handlers, v.Fn)
		}
	}
	return n
}

// Text creates a virtual text-node leaf.
func Text(s string) *Node { return &Node{text: s} }

// ---------------------------------------------------------------------------
// Runtime state (package-level; single-instance per binary)
// ---------------------------------------------------------------------------

var (
	rootFn          func() *Node
	rootNodeID      protocol.NodeID
	nextNodeID      uint64 = 1 // allocID adds 1 before returning
	batchCounter    uint64
	currentRevision uint64
	prevRootChild   protocol.NodeID // NodeID of the subtree appended in the last render (0=none)
	clickHandlers   map[protocol.NodeID]func()
)

func allocID() protocol.NodeID {
	return protocol.NodeID(atomic.AddUint64(&nextNodeID, 1))
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

// renderNode walks the virtual tree, allocates NodeIDs, appends DOM ops to b,
// records click handlers, and returns the allocated NodeID for this node.
func renderNode(b *protocol.Batch, n *Node, handlers map[protocol.NodeID]func()) protocol.NodeID {
	id := allocID()
	if n.tag == "" {
		// Text leaf.
		b.CreateText(id, n.text)
		return id
	}
	b.CreateElement(id, n.tag)
	for _, a := range n.attrs {
		b.SetAttr(id, a.Name, a.Value)
	}
	if len(n.handlers) > 0 {
		handlers[id] = n.handlers[0]
	}
	for _, c := range n.children {
		cid := renderNode(b, c, handlers)
		b.Append(id, cid)
	}
	return id
}

// rerender builds and commits a fresh DOM patch.
func rerender() {
	batchCounter++
	b := protocol.NewBatch(batchCounter, currentRevision)

	// Remove the previous subtree (full-replace strategy).
	if prevRootChild != protocol.NullNode {
		b.Remove(prevRootChild)
	}

	newHandlers := make(map[protocol.NodeID]func())
	vroot := rootFn()
	childID := renderNode(b, vroot, newHandlers)
	b.Append(rootNodeID, childID)
	b.Commit()

	rev := dom.Commit(b)
	currentRevision = uint64(rev)
	prevRootChild = childID
	clickHandlers = newHandlers
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

// Run registers the root render function. In wasip1 builds the host calls
// gobrowser_mount to trigger the first render; in native tests call
// NativeMount directly after Run.
func Run(root func() *Node) {
	rootFn = root
}

// mount performs the initial render with the host-assigned root NodeID.
func mount(hostRootID uint64) {
	rootNodeID = protocol.NodeID(hostRootID)
	prevRootChild = protocol.NullNode
	clickHandlers = make(map[protocol.NodeID]func())
	rerender()
}

// handleEvent decodes a raw event message and dispatches to the registered
// click handler, then re-renders. The wire format is the eventmsg encoding:
// kind u32 | target u64 | valueLen u32 | value bytes (little-endian).
func handleEvent(buf []byte) {
	if len(buf) < 16 {
		return
	}
	target := protocol.NodeID(
		uint64(buf[4]) | uint64(buf[5])<<8 | uint64(buf[6])<<16 | uint64(buf[7])<<24 |
			uint64(buf[8])<<32 | uint64(buf[9])<<40 | uint64(buf[10])<<48 | uint64(buf[11])<<56,
	)
	if fn, ok := clickHandlers[target]; ok {
		fn()
	}
	rerender()
}
