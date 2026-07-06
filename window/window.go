// Package window implements the browser window abstraction and the multi-window
// manager that forms the top-level broker registry.
//
// A Window owns an ordered list of Tabs and tracks the currently active
// (foreground) tab. A Manager owns a registry of Windows and associates them
// with a user profile and session, carrying the crash policy used by the
// supervised session layer.
package window

import (
	"errors"
	"fmt"
	"sync"

	"github.com/monstercameron/gowebbrowser/tab"
)

// WindowID uniquely identifies a Window within a Manager.
type WindowID uint64

// Sentinel errors.
var (
	// ErrNoTabs is returned when an operation requires at least one tab but the
	// window is empty.
	ErrNoTabs = errors.New("window: no tabs")
	// ErrTabNotFound is returned when a TabID is not present in this window.
	ErrTabNotFound = errors.New("window: tab not found")
	// ErrWindowClosed is returned when an operation is attempted on a closed window.
	ErrWindowClosed = errors.New("window: window is closed")
	// ErrWindowNotFound is returned when a WindowID is not present in the Manager.
	ErrWindowNotFound = errors.New("window: window not found")
)

// Window is a top-level browser window that owns an ordered list of tabs and
// tracks which one is currently active (in the foreground). All methods are
// goroutine-safe.
type Window struct {
	mu        sync.Mutex
	id        WindowID
	tabs      []*tab.Tab // ordered list; position matches visual tab strip order
	activeIdx int        // index into tabs; -1 when the window is empty
	closed    bool
}

// newWindow allocates an empty Window. Called only by Manager.
func newWindow(id WindowID) *Window {
	return &Window{id: id, activeIdx: -1}
}

// ID returns the window's unique identifier.
func (w *Window) ID() WindowID {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.id
}

// TabCount returns the number of tabs currently in this window.
func (w *Window) TabCount() int {
	w.mu.Lock()
	defer w.mu.Unlock()
	return len(w.tabs)
}

// ActiveTab returns the currently active tab, or nil if the window is empty.
func (w *Window) ActiveTab() *tab.Tab {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.activeIdx < 0 || w.activeIdx >= len(w.tabs) {
		return nil
	}
	return w.tabs[w.activeIdx]
}

// ActiveIndex returns the 0-based index of the active tab in the tab strip,
// or -1 if there are no tabs.
func (w *Window) ActiveIndex() int {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.activeIdx
}

// Tabs returns a snapshot copy of the current tab slice in visual order.
// Callers must not modify the returned slice.
func (w *Window) Tabs() []*tab.Tab {
	w.mu.Lock()
	defer w.mu.Unlock()
	out := make([]*tab.Tab, len(w.tabs))
	copy(out, w.tabs)
	return out
}

// OpenTab appends t to the tab strip. If activate is true, or if the window was
// previously empty, the new tab becomes the active tab.
func (w *Window) OpenTab(t *tab.Tab, activate bool) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return ErrWindowClosed
	}
	w.tabs = append(w.tabs, t)
	if activate || w.activeIdx < 0 {
		w.activeIdx = len(w.tabs) - 1
	}
	return nil
}

// CloseTab removes the tab with the given id from this window. If the active
// tab is removed, the window activates the adjacent tab (prefer the previous;
// fall back to the next when the removed tab was at index 0).
func (w *Window) CloseTab(id tab.TabID) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return ErrWindowClosed
	}
	idx := w.indexOfLocked(id)
	if idx < 0 {
		return ErrTabNotFound
	}

	w.tabs = append(w.tabs[:idx], w.tabs[idx+1:]...)

	switch {
	case len(w.tabs) == 0:
		w.activeIdx = -1
	case w.activeIdx >= len(w.tabs):
		// The removed tab was the last one; move active to the new last.
		w.activeIdx = len(w.tabs) - 1
	case idx < w.activeIdx:
		// A tab before the active was removed; shift the index left.
		w.activeIdx--
	case idx == w.activeIdx:
		// The active tab itself was removed; prefer the tab at the same index
		// (which is now the next tab), clamped to the new length.
		if w.activeIdx >= len(w.tabs) {
			w.activeIdx = len(w.tabs) - 1
		}
	}
	return nil
}

// ActivateTab brings the tab with the given id to the foreground.
func (w *Window) ActivateTab(id tab.TabID) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return ErrWindowClosed
	}
	idx := w.indexOfLocked(id)
	if idx < 0 {
		return ErrTabNotFound
	}
	w.activeIdx = idx
	return nil
}

// MoveTab repositions the tab with id to toIndex (0-based). Tabs between
// fromIndex and toIndex shift by one to fill the gap. The active tab tracker
// follows the moved tab if it was active.
func (w *Window) MoveTab(id tab.TabID, toIndex int) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return ErrWindowClosed
	}
	fromIdx := w.indexOfLocked(id)
	if fromIdx < 0 {
		return ErrTabNotFound
	}
	if toIndex < 0 || toIndex >= len(w.tabs) {
		return fmt.Errorf("window: target index %d out of range [0, %d)", toIndex, len(w.tabs))
	}
	if fromIdx == toIndex {
		return nil
	}

	// Remember the active tab's ID so we can re-find it after the shuffle.
	var activeID tab.TabID
	hasActive := w.activeIdx >= 0 && w.activeIdx < len(w.tabs)
	if hasActive {
		activeID = w.tabs[w.activeIdx].ID()
	}

	// Extract the tab.
	moved := w.tabs[fromIdx]
	// Build new slice without the moved tab.
	newTabs := make([]*tab.Tab, 0, len(w.tabs))
	for i, tb := range w.tabs {
		if i != fromIdx {
			newTabs = append(newTabs, tb)
		}
	}
	// Insert at the target position.
	final := make([]*tab.Tab, len(w.tabs))
	copy(final[:toIndex], newTabs[:toIndex])
	final[toIndex] = moved
	copy(final[toIndex+1:], newTabs[toIndex:])
	w.tabs = final

	// Re-resolve the active index.
	if hasActive {
		w.activeIdx = w.indexOfLocked(activeID)
	}
	return nil
}

// indexOfLocked returns the slice index of the tab with the given id, or -1 if
// not found. The caller must hold w.mu.
func (w *Window) indexOfLocked(id tab.TabID) int {
	for i, t := range w.tabs {
		if t.ID() == id {
			return i
		}
	}
	return -1
}

// HasTab reports whether the window contains a tab with the given id.
func (w *Window) HasTab(id tab.TabID) bool {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.indexOfLocked(id) >= 0
}

// close marks the window as closed. Called by Manager.CloseWindow.
// The caller must hold w.mu.
func (w *Window) closeLocked() {
	w.closed = true
}

// IsClosed reports whether the window has been closed.
func (w *Window) IsClosed() bool {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.closed
}
