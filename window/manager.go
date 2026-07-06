package window

import (
	"fmt"
	"sync"
	"sync/atomic"

	"github.com/monstercameron/gowebbrowser/session"
)

// Manager is the top-level registry for all browser windows. It binds together
// the user profile, the current session, and the crash policy that the
// supervised session layer will apply when tabs crash.
//
// Manager is goroutine-safe.
type Manager struct {
	mu        sync.Mutex
	profileID string
	sessionID string
	windows   map[WindowID]*Window
	nextWinID atomic.Uint64
	policy    session.CrashPolicy
}

// NewManager creates a Manager for the given profile and session identifiers,
// applying policy as the default crash policy for all supervised tabs.
func NewManager(profileID, sessionID string, policy session.CrashPolicy) *Manager {
	return &Manager{
		profileID: profileID,
		sessionID: sessionID,
		windows:   make(map[WindowID]*Window),
		policy:    policy,
	}
}

// ProfileID returns the profile identifier this Manager is associated with.
func (m *Manager) ProfileID() string { return m.profileID }

// SessionID returns the session identifier this Manager is associated with.
func (m *Manager) SessionID() string { return m.sessionID }

// Policy returns the crash policy carried by this Manager.
func (m *Manager) Policy() session.CrashPolicy {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.policy
}

// NewWindow allocates a new, empty Window, registers it, and returns it.
func (m *Manager) NewWindow() *Window {
	id := WindowID(m.nextWinID.Add(1))
	w := newWindow(id)
	m.mu.Lock()
	m.windows[id] = w
	m.mu.Unlock()
	return w
}

// Window returns the Window with the given id and true, or nil and false if
// no such window exists.
func (m *Manager) Window(id WindowID) (*Window, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	w, ok := m.windows[id]
	return w, ok
}

// CloseWindow marks the window as closed and removes it from the registry.
func (m *Manager) CloseWindow(id WindowID) error {
	m.mu.Lock()
	w, ok := m.windows[id]
	if !ok {
		m.mu.Unlock()
		return fmt.Errorf("%w: %d", ErrWindowNotFound, id)
	}
	delete(m.windows, id)
	m.mu.Unlock()

	w.mu.Lock()
	w.closeLocked()
	w.mu.Unlock()
	return nil
}

// Windows returns a snapshot of all currently registered Windows.
func (m *Manager) Windows() []*Window {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]*Window, 0, len(m.windows))
	for _, w := range m.windows {
		out = append(out, w)
	}
	return out
}

// WindowCount returns the number of open windows.
func (m *Manager) WindowCount() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return len(m.windows)
}
