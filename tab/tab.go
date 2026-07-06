// Package tab implements the Tab abstraction and its lifecycle state machine.
// Each Tab owns a Renderer (engine.Engine) and an AppHost, and drives itself
// through a well-defined set of states with guarded transitions.
package tab

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
)

// TabID is a unique, monotonically increasing identifier for a Tab.
type TabID uint64

// TabState enumerates the lifecycle states a Tab can occupy.
type TabState int

const (
	// TabCreated: the Tab struct has been allocated but navigation has not started.
	TabCreated TabState = iota
	// TabLoading: a navigation is in-progress; the WASM app is loading.
	TabLoading
	// TabActive: the tab is in the foreground; the app is running and receiving events.
	TabActive
	// TabBackground: the tab is hidden; timers may be throttled.
	TabBackground
	// TabFrozen: the WASM is suspended; the rendered DOM remains visible; no timers
	// or event delivery except a visibility-restore trigger.
	TabFrozen
	// TabDiscarded: renderer resources have been released and the WASM state snapshot
	// has been saved to reclaim memory; the thumbnail is retained.
	TabDiscarded
	// TabCrashed: the app or renderer has crashed; crash UI is shown.
	TabCrashed
	// TabRestoring: the tab is being re-initialised from a snapshot after a crash or
	// discard; transitions to Active on success.
	TabRestoring
	// TabClosed: the tab has been closed; all resources have been released. Terminal.
	TabClosed
)

// String returns a human-readable name for a TabState.
func (s TabState) String() string {
	switch s {
	case TabCreated:
		return "Created"
	case TabLoading:
		return "Loading"
	case TabActive:
		return "Active"
	case TabBackground:
		return "Background"
	case TabFrozen:
		return "Frozen"
	case TabDiscarded:
		return "Discarded"
	case TabCrashed:
		return "Crashed"
	case TabRestoring:
		return "Restoring"
	case TabClosed:
		return "Closed"
	default:
		return fmt.Sprintf("TabState(%d)", int(s))
	}
}

// ErrInvalidTransition is returned when the requested state transition is not
// permitted by the state machine.
var ErrInvalidTransition = errors.New("tab: invalid state transition")

// ErrTabClosed is returned when an operation is attempted on a closed tab.
var ErrTabClosed = errors.New("tab: tab is closed")

// AppHost is the seam to the WASM app runtime. The real host package implements
// this; the tab package owns only the interface, so host can be built
// independently.
type AppHost interface {
	// Load initialises the WASM module from the provided binary. Must be called
	// before Mount.
	Load(ctx context.Context, wasm []byte) error
	// Mount attaches the running app to the DOM node identified by selector
	// (e.g. "#app").
	Mount(selector string) error
	// Close shuts down the WASM runtime and releases all resources.
	Close() error
}

// Renderer is an alias for the rendering engine seam defined in the engine
// package. Both names are valid; Renderer is the tab-package vocabulary.
type Renderer = engine.Engine

// TabSnapshot is a lightweight, serialisable snapshot of a tab's essential
// state. It captures enough to restart the app and restore the visible page.
type TabSnapshot struct {
	// URL is the last committed URL at the time the snapshot was taken.
	URL string
	// State is the tab state at snapshot time.
	State TabState
	// DOMRevision is the engine revision when the snapshot was captured.
	DOMRevision uint64
	// CapturedAt is the wall-clock time the snapshot was taken.
	CapturedAt time.Time
}

// validTransitions defines the allowed (from → to) pairs for the state machine.
// A state absent from the map or with an empty allowed set is terminal.
var validTransitions = map[TabState]map[TabState]bool{
	TabCreated:    {TabLoading: true, TabClosed: true},
	TabLoading:    {TabActive: true, TabCrashed: true, TabClosed: true},
	TabActive:     {TabLoading: true, TabBackground: true, TabFrozen: true, TabCrashed: true, TabClosed: true},
	TabBackground: {TabLoading: true, TabActive: true, TabFrozen: true, TabDiscarded: true, TabCrashed: true, TabClosed: true},
	TabFrozen:     {TabLoading: true, TabActive: true, TabDiscarded: true, TabCrashed: true, TabClosed: true},
	TabDiscarded:  {TabRestoring: true, TabClosed: true},
	TabCrashed:    {TabRestoring: true, TabClosed: true},
	TabRestoring:  {TabActive: true, TabCrashed: true, TabClosed: true},
	TabClosed:     {}, // terminal
}

// Tab is a single browser tab. It owns a Renderer and an AppHost, and drives
// itself through the lifecycle state machine. All public methods are
// goroutine-safe.
type Tab struct {
	mu       sync.Mutex
	id       TabID
	url      string
	state    TabState
	renderer Renderer
	host     AppHost
	snapshot *TabSnapshot // last snapshot captured by Snapshot()
}

// New allocates a new Tab in the TabCreated state with the given renderer and
// host. The tab does not start loading until Navigate is called.
func New(id TabID, renderer Renderer, host AppHost) *Tab {
	return &Tab{
		id:       id,
		state:    TabCreated,
		renderer: renderer,
		host:     host,
	}
}

// ID returns the tab's unique identifier.
func (t *Tab) ID() TabID {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.id
}

// URL returns the tab's last committed URL.
func (t *Tab) URL() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.url
}

// State returns the current lifecycle state.
func (t *Tab) State() TabState {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.state
}

// Renderer returns the tab's rendering engine. Callers must not close the
// renderer directly; use Tab.Close instead.
func (t *Tab) Renderer() Renderer {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.renderer
}

// transition is the internal guard for state transitions.
// The caller must hold t.mu.
func (t *Tab) transition(to TabState) error {
	allowed, ok := validTransitions[t.state]
	if !ok || !allowed[to] {
		return fmt.Errorf("%w: %s → %s", ErrInvalidTransition, t.state, to)
	}
	t.state = to
	return nil
}

// Navigate starts a navigation to url. Valid from Created, Active, Background,
// and Frozen. The tab transitions through Loading → Active on success, or
// Loading → Crashed on renderer failure.
func (t *Tab) Navigate(ctx context.Context, url string) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if t.state == TabClosed {
		return ErrTabClosed
	}

	if err := t.transition(TabLoading); err != nil {
		return err
	}
	t.url = url

	if err := t.renderer.LoadHTML(url); err != nil {
		_ = t.transition(TabCrashed)
		return fmt.Errorf("tab: renderer LoadHTML: %w", err)
	}
	if err := t.host.Mount("#app"); err != nil {
		_ = t.transition(TabCrashed)
		return fmt.Errorf("tab: host Mount: %w", err)
	}
	return t.transition(TabActive)
}

// Freeze suspends the WASM runtime. Valid from Active or Background.
func (t *Tab) Freeze() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.transition(TabFrozen)
}

// Resume brings a Frozen tab back to Active.
func (t *Tab) Resume() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.transition(TabActive)
}

// Background moves the tab to the Background state. Valid from Active.
func (t *Tab) Background() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.transition(TabBackground)
}

// Activate moves the tab to Active. Valid from Background or Frozen.
func (t *Tab) Activate() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.transition(TabActive)
}

// Discard releases renderer and WASM resources to reclaim memory.
// Valid from Background or Frozen.
func (t *Tab) Discard() error {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.transition(TabDiscarded)
}

// Reload re-navigates to the current URL. From Crashed or Discarded the tab
// goes through the Restoring path; from other states it goes through Loading.
func (t *Tab) Reload(ctx context.Context) error {
	t.mu.Lock()
	url := t.url
	state := t.state
	t.mu.Unlock()

	if state == TabCrashed || state == TabDiscarded {
		return t.Restore(ctx, &TabSnapshot{URL: url})
	}
	return t.Navigate(ctx, url)
}

// KillApp simulates an app/WASM process crash and transitions to TabCrashed.
// Valid from Loading, Active, Background, Frozen, or Restoring.
func (t *Tab) KillApp() error {
	t.mu.Lock()
	defer t.mu.Unlock()

	switch t.state {
	case TabLoading, TabActive, TabBackground, TabFrozen, TabRestoring:
		// valid sources
	default:
		return fmt.Errorf("%w: cannot kill app from %s", ErrInvalidTransition, t.state)
	}
	return t.transition(TabCrashed)
}

// Snapshot captures a lightweight snapshot of the current tab state and caches
// it for later retrieval by LastSnapshot.
func (t *Tab) Snapshot() (*TabSnapshot, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	if t.state == TabClosed {
		return nil, ErrTabClosed
	}
	snap := &TabSnapshot{
		URL:         t.url,
		State:       t.state,
		DOMRevision: t.renderer.Revision(),
		CapturedAt:  time.Now(),
	}
	t.snapshot = snap
	return snap, nil
}

// Restore transitions the tab from Crashed or Discarded through Restoring →
// Active using the provided snapshot. snap must not be nil.
func (t *Tab) Restore(ctx context.Context, snap *TabSnapshot) error {
	if snap == nil {
		return errors.New("tab: nil snapshot")
	}
	t.mu.Lock()
	defer t.mu.Unlock()

	if err := t.transition(TabRestoring); err != nil {
		return err
	}
	t.url = snap.URL

	if err := t.renderer.LoadHTML(snap.URL); err != nil {
		_ = t.transition(TabCrashed)
		return fmt.Errorf("tab: renderer LoadHTML during restore: %w", err)
	}
	if err := t.host.Mount("#app"); err != nil {
		_ = t.transition(TabCrashed)
		return fmt.Errorf("tab: host Mount during restore: %w", err)
	}
	return t.transition(TabActive)
}

// LastSnapshot returns the most recent snapshot captured by Snapshot, or nil
// if none has been taken.
func (t *Tab) LastSnapshot() *TabSnapshot {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.snapshot
}

// Close releases all tab resources and marks the tab as closed. Idempotent:
// calling Close on an already-closed tab is a no-op.
func (t *Tab) Close() error {
	t.mu.Lock()
	defer t.mu.Unlock()

	if t.state == TabClosed {
		return nil
	}
	if err := t.renderer.Close(); err != nil {
		return fmt.Errorf("tab: renderer Close: %w", err)
	}
	if err := t.host.Close(); err != nil {
		return fmt.Errorf("tab: host Close: %w", err)
	}
	t.state = TabClosed
	return nil
}
