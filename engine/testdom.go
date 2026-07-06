package engine

import (
	"sort"
	"strings"
	"sync"

	"github.com/monstercameron/gowebbrowser/protocol"
)

// rootNodeID is the fixed id TestDOM assigns to the mount root.
const rootNodeID protocol.NodeID = 1

type tnode struct {
	id     protocol.NodeID
	tag    string // "" => text node
	text   string
	isText bool
	attrs  map[string]string
	parent *tnode
	kids   []*tnode
}

// TestDOM is a pure-Go Engine: it applies GDOM batches to an in-memory tree and
// can serialize to HTML. Used for headless runs and tests; mirrors what the
// WebKit agent does against WebCore.
type TestDOM struct {
	mu     sync.Mutex // guards top-level entry points (Apply/HTML/Revision/RootNode/FindFirst)
	nodes  map[protocol.NodeID]*tnode
	root   *tnode
	rev    uint64
	events chan Event
}

// NewTestDOM returns an empty engine.
func NewTestDOM() *TestDOM {
	return &TestDOM{
		nodes:  make(map[protocol.NodeID]*tnode),
		events: make(chan Event, 64),
	}
}

var _ Engine = (*TestDOM)(nil)

func (d *TestDOM) LoadHTML(string) error { return nil }

func (d *TestDOM) RootNode(selector string) (protocol.NodeID, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	id := strings.TrimPrefix(selector, "#")
	d.root = &tnode{id: rootNodeID, tag: "div", attrs: map[string]string{"id": id}}
	d.nodes[rootNodeID] = d.root
	return rootNodeID, nil
}

func (d *TestDOM) Revision() uint64 {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.rev
}

// TextOf returns the concatenated text content of the node subtree rooted at id
// (used by native renderers to read an element's label).
func (d *TestDOM) TextOf(id protocol.NodeID) string {
	d.mu.Lock()
	defer d.mu.Unlock()
	n := d.nodes[id]
	if n == nil {
		return ""
	}
	var sb strings.Builder
	var walk func(*tnode)
	walk = func(x *tnode) {
		if x.isText {
			sb.WriteString(x.text)
			return
		}
		for _, c := range x.kids {
			walk(c)
		}
	}
	walk(n)
	return sb.String()
}

func (d *TestDOM) Events() <-chan Event { return d.events }

func (d *TestDOM) Close() error { close(d.events); return nil }

// InjectEvent delivers a synthetic input event (used by tests / headless driver).
func (d *TestDOM) InjectEvent(e Event) { d.events <- e }

// FindFirst returns the id of the first element (depth-first from the mount
// root) whose tag matches, or 0 if none. Used by headless drivers/tests to
// locate a node to dispatch events at, since TestDOM has no hit-testing.
func (d *TestDOM) FindFirst(tag string) protocol.NodeID {
	d.mu.Lock()
	defer d.mu.Unlock()
	var walk func(n *tnode) protocol.NodeID
	walk = func(n *tnode) protocol.NodeID {
		if n == nil {
			return 0
		}
		if !n.isText && n.tag == tag {
			return n.id
		}
		for _, c := range n.kids {
			if id := walk(c); id != 0 {
				return id
			}
		}
		return 0
	}
	return walk(d.root)
}

func (d *TestDOM) Apply(buf []byte) (uint64, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	h, _, err := protocol.DecodeHeader(buf)
	if err != nil {
		return 0, err
	}
	if h.BaseRevision != d.rev {
		return 0, ErrRevisionMismatch
	}
	if _, err := protocol.Decode(buf, d); err != nil {
		return 0, err
	}
	return d.rev, nil
}

// HTML serializes the tree under the mount root.
func (d *TestDOM) HTML() string {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.root == nil {
		return ""
	}
	var sb strings.Builder
	d.write(&sb, d.root)
	return sb.String()
}

func (d *TestDOM) write(sb *strings.Builder, n *tnode) {
	if n.isText {
		sb.WriteString(escapeText(n.text))
		return
	}
	sb.WriteByte('<')
	sb.WriteString(n.tag)
	keys := make([]string, 0, len(n.attrs))
	for k := range n.attrs {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		sb.WriteByte(' ')
		sb.WriteString(k)
		sb.WriteString(`="`)
		sb.WriteString(escapeAttr(n.attrs[k]))
		sb.WriteByte('"')
	}
	sb.WriteByte('>')
	for _, c := range n.kids {
		d.write(sb, c)
	}
	sb.WriteString("</")
	sb.WriteString(n.tag)
	sb.WriteByte('>')
}

// --- protocol.Visitor ------------------------------------------------------

func (d *TestDOM) CreateElement(node protocol.NodeID, tag string) {
	d.nodes[node] = &tnode{id: node, tag: tag, attrs: map[string]string{}}
}

func (d *TestDOM) CreateText(node protocol.NodeID, text string) {
	d.nodes[node] = &tnode{id: node, isText: true, text: text}
}

func (d *TestDOM) SetText(node protocol.NodeID, text string) {
	if n := d.nodes[node]; n != nil {
		n.text = text
	}
}

func (d *TestDOM) SetAttr(node protocol.NodeID, name, value string) {
	if n := d.nodes[node]; n != nil {
		if n.attrs == nil {
			n.attrs = map[string]string{}
		}
		n.attrs[name] = value
	}
}

func (d *TestDOM) RemoveAttr(node protocol.NodeID, name string) {
	if n := d.nodes[node]; n != nil {
		delete(n.attrs, name)
	}
}

func (d *TestDOM) Append(parent, child protocol.NodeID) {
	p, c := d.nodes[parent], d.nodes[child]
	if p == nil || c == nil {
		return
	}
	detach(c)
	c.parent = p
	p.kids = append(p.kids, c)
}

func (d *TestDOM) InsertBefore(parent, child, ref protocol.NodeID) {
	p, c := d.nodes[parent], d.nodes[child]
	if p == nil || c == nil {
		return
	}
	r := d.nodes[ref]
	detach(c)
	c.parent = p
	if r == nil {
		p.kids = append(p.kids, c)
		return
	}
	for i, k := range p.kids {
		if k == r {
			p.kids = append(p.kids[:i], append([]*tnode{c}, p.kids[i:]...)...)
			return
		}
	}
	p.kids = append(p.kids, c)
}

func (d *TestDOM) Remove(node protocol.NodeID) {
	if n := d.nodes[node]; n != nil {
		detach(n)
		delete(d.nodes, node)
	}
}

func (d *TestDOM) SetValue(node protocol.NodeID, value string) {
	d.SetAttr(node, "value", value)
}

func (d *TestDOM) Commit() { d.rev++ }

// --- helpers ---------------------------------------------------------------

func detach(n *tnode) {
	if n.parent == nil {
		return
	}
	siblings := n.parent.kids
	for i, k := range siblings {
		if k == n {
			n.parent.kids = append(siblings[:i], siblings[i+1:]...)
			break
		}
	}
	n.parent = nil
}

func escapeText(s string) string {
	r := strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;")
	return r.Replace(s)
}

func escapeAttr(s string) string {
	r := strings.NewReplacer("&", "&amp;", `"`, "&quot;", "<", "&lt;")
	return r.Replace(s)
}
