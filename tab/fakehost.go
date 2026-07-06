package tab

import (
	"context"
	"sync"
)

// FakeHost is an AppHost test double that records every call made to it.
// It is exported so that cmd/broker and external test packages can use it
// without importing test-only files.
//
// All fields are safe to read after the relevant methods have returned.
type FakeHost struct {
	mu sync.Mutex

	// LoadCalls records the arguments passed to each Load invocation.
	LoadCalls [][]byte
	// MountCalls records the selector arguments passed to each Mount invocation.
	MountCalls []string
	// CloseCalled reports whether Close has been called at least once.
	CloseCalled bool

	// Optionally inject errors.
	LoadErr  error
	MountErr error
	CloseErr error
}

// Load records the call and returns LoadErr.
func (f *FakeHost) Load(_ context.Context, wasm []byte) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	cp := make([]byte, len(wasm))
	copy(cp, wasm)
	f.LoadCalls = append(f.LoadCalls, cp)
	return f.LoadErr
}

// Mount records the selector and returns MountErr.
func (f *FakeHost) Mount(selector string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.MountCalls = append(f.MountCalls, selector)
	return f.MountErr
}

// Close records the call and returns CloseErr.
func (f *FakeHost) Close() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.CloseCalled = true
	return f.CloseErr
}

// MountCount returns the number of times Mount has been called.
func (f *FakeHost) MountCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return len(f.MountCalls)
}
