// Package session implements process-supervision types and the crash-recovery
// supervisor for the Go browser broker.
//
// Design overview
//
// The broker runs one or more TabHost processes (or in-process WASM runtimes
// for the single-process dev mode). When a TabHost crashes, the broker
// constructs a CrashEvent and hands it to a Supervisor. The Supervisor applies
// a CrashPolicy: restart the WASM runtime up to MaxRestarts times within a
// RestartWindow, then give up.
//
// IsolationLevel controls how many OS processes are used for isolation.
package session

import (
	"fmt"
	"sync"
	"time"

	"github.com/monstercameron/gowebbrowser/tab"
)

// ProcessKind identifies the role of a process in the browser architecture.
type ProcessKind string

const (
	// ProcessBrowser is the main broker / orchestrator process.
	ProcessBrowser ProcessKind = "browser"
	// ProcessTab is the TabHost process that owns a tab's WASM runtime.
	ProcessTab ProcessKind = "tab"
	// ProcessWASM is the WASM app instance within a TabHost.
	ProcessWASM ProcessKind = "wasm"
	// ProcessRender is the engine renderer process (WebKit/CEF renderer).
	ProcessRender ProcessKind = "render"
	// ProcessGPU is the GPU process used by the rendering engine.
	ProcessGPU ProcessKind = "gpu"
)

// IsolationLevel describes how strictly WASM app instances are isolated from
// each other and from the broker process.
type IsolationLevel int

const (
	// IsolationSingleProcess runs all tabs in the same OS process. Fast for
	// development and testing; offers no crash isolation between tabs.
	IsolationSingleProcess IsolationLevel = iota

	// IsolationRuntimePerTab gives each tab its own WASM runtime instance
	// inside the same OS process. A panicking WASM module can be restarted
	// without restarting the entire process.
	IsolationRuntimePerTab

	// IsolationProcessPerTab spawns a dedicated TabHost OS process per tab.
	// A tab crash cannot corrupt the broker or other tabs. This is the default
	// production mode.
	IsolationProcessPerTab

	// IsolationProcessPerSiteInstance spawns a separate process per
	// same-origin site-instance group, mirroring modern browser site-isolation
	// policy. Cross-origin tabs are always separated; same-origin related tabs
	// may share a process.
	IsolationProcessPerSiteInstance
)

// CrashEvent describes a process failure reported to the supervisor.
type CrashEvent struct {
	// Kind identifies which subsystem crashed.
	Kind ProcessKind
	// TabID identifies the tab that was serving the crashed process.
	TabID tab.TabID
	// ProcessID is the OS PID of the crashed process, or 0 for in-process failures.
	ProcessID int
	// Reason is a human-readable description of the failure.
	Reason string
	// ExitCode is the OS exit status of the crashed process.
	ExitCode int
	// Recoverable indicates whether the supervisor should attempt a restart.
	Recoverable bool
	// LastURL is the URL the tab was serving when the crash occurred.
	LastURL string
	// LastApp is the wasm module identifier (path or hash) that was running.
	LastApp string
}

// CrashPolicy configures how the supervisor responds to a crash.
type CrashPolicy struct {
	// RestartWASM instructs the supervisor to restart the WASM runtime after a
	// ProcessWASM crash.
	RestartWASM bool
	// ReloadRenderer instructs the supervisor to reload the renderer after a
	// ProcessRender crash.
	ReloadRenderer bool
	// RestoreDOMSnapshot instructs the supervisor to restore the last DOM
	// snapshot when restarting.
	RestoreDOMSnapshot bool
	// MaxRestarts is the maximum number of restart attempts permitted within
	// RestartWindow. Zero means "no restarts".
	MaxRestarts int
	// RestartWindow is the rolling time window over which MaxRestarts is
	// counted. If MaxRestarts restarts occur within RestartWindow, the
	// supervisor gives up.
	RestartWindow time.Duration
}

// RestartFunc is a callback invoked by the Supervisor when it decides to
// restart a tab after a crash. Implementations should reload the WASM module,
// restore the DOM snapshot, etc. A non-nil error from RestartFunc is
// propagated back through HandleCrash.
type RestartFunc func(ev CrashEvent) error

// Supervisor applies a CrashPolicy when tabs crash. It tracks per-tab restart
// history to enforce the MaxRestarts × RestartWindow budget.
//
// A Supervisor is goroutine-safe.
type Supervisor struct {
	mu       sync.Mutex
	policy   CrashPolicy
	restart  RestartFunc
	attempts map[tab.TabID][]time.Time // sliding window of restart timestamps
}

// NewSupervisor creates a Supervisor that applies policy and calls restart
// when a recoverable crash occurs within the budget. restart must not be nil.
func NewSupervisor(policy CrashPolicy, restart RestartFunc) *Supervisor {
	return &Supervisor{
		policy:   policy,
		restart:  restart,
		attempts: make(map[tab.TabID][]time.Time),
	}
}

// HandleCrash evaluates the crash event against the policy and, if warranted,
// calls the restart callback.
//
// Returns an error if:
//   - ev.Recoverable is false (non-recoverable crash; no restart attempted).
//   - The tab has already exhausted its MaxRestarts budget within RestartWindow.
//   - The restart callback itself returns an error.
func (s *Supervisor) HandleCrash(ev CrashEvent) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if !ev.Recoverable {
		return fmt.Errorf("session: non-recoverable crash for tab %d (%s): %s (exit %d)",
			ev.TabID, ev.Kind, ev.Reason, ev.ExitCode)
	}
	if s.policy.MaxRestarts <= 0 {
		return fmt.Errorf("session: crash policy forbids restarts for tab %d", ev.TabID)
	}

	now := time.Now()

	// Trim the history to the current RestartWindow.
	// A zero RestartWindow means "never expire": all history is kept.
	history := s.attempts[ev.TabID]
	var trimmed []time.Time
	if s.policy.RestartWindow > 0 {
		cutoff := now.Add(-s.policy.RestartWindow)
		for _, ts := range history {
			if ts.After(cutoff) {
				trimmed = append(trimmed, ts)
			}
		}
	} else {
		trimmed = history
	}
	s.attempts[ev.TabID] = trimmed

	if len(trimmed) >= s.policy.MaxRestarts {
		return fmt.Errorf(
			"session: tab %d exceeded %d restart(s) within %s; giving up",
			ev.TabID, s.policy.MaxRestarts, s.policy.RestartWindow,
		)
	}

	// Record this restart attempt before calling the callback to prevent
	// re-entrant HandleCrash calls from racing past the budget.
	s.attempts[ev.TabID] = append(s.attempts[ev.TabID], now)

	if err := s.restart(ev); err != nil {
		return fmt.Errorf("session: restart callback for tab %d: %w", ev.TabID, err)
	}
	return nil
}

// RestartCount returns the number of restarts recorded for tabID within the
// current RestartWindow. A zero RestartWindow returns the total since the last
// ResetTab. Useful for diagnostics and testing.
func (s *Supervisor) RestartCount(tabID tab.TabID) int {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.policy.RestartWindow == 0 {
		// No expiry: count all retained history.
		return len(s.attempts[tabID])
	}
	cutoff := time.Now().Add(-s.policy.RestartWindow)
	count := 0
	for _, ts := range s.attempts[tabID] {
		if ts.After(cutoff) {
			count++
		}
	}
	return count
}

// ResetTab clears the restart history for tabID (e.g. after a successful
// user-initiated reload that resets the crash budget).
func (s *Supervisor) ResetTab(tabID tab.TabID) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.attempts, tabID)
}
