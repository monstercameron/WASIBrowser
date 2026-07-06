package session_test

import (
	"errors"
	"testing"
	"time"

	"github.com/monstercameron/gowebbrowser/session"
	"github.com/monstercameron/gowebbrowser/tab"
)

// recoverableEv returns a recoverable CrashEvent for the given tab.
func recoverableEv(id tab.TabID) session.CrashEvent {
	return session.CrashEvent{
		Kind:        session.ProcessWASM,
		TabID:       id,
		Reason:      "simulated panic",
		ExitCode:    1,
		Recoverable: true,
		LastURL:     "https://example.com",
	}
}

// ---- Supervisor basic behaviour ---------------------------------------------

func TestSupervisorFirstRestartSucceeds(t *testing.T) {
	called := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{
			RestartWASM:   true,
			MaxRestarts:   3,
			RestartWindow: time.Minute,
		},
		func(ev session.CrashEvent) error {
			called++
			return nil
		},
	)

	if err := sup.HandleCrash(recoverableEv(1)); err != nil {
		t.Fatalf("first restart should succeed, got: %v", err)
	}
	if called != 1 {
		t.Fatalf("want restart callback called once, got %d", called)
	}
}

func TestSupervisorNonRecoverableReturnsError(t *testing.T) {
	called := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 3, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { called++; return nil },
	)

	ev := recoverableEv(1)
	ev.Recoverable = false

	if err := sup.HandleCrash(ev); err == nil {
		t.Fatal("want error for non-recoverable crash")
	}
	if called != 0 {
		t.Fatal("restart callback must not be called for non-recoverable crash")
	}
}

func TestSupervisorZeroMaxRestartsReturnsError(t *testing.T) {
	called := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 0, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { called++; return nil },
	)

	if err := sup.HandleCrash(recoverableEv(1)); err == nil {
		t.Fatal("want error when MaxRestarts=0")
	}
	if called != 0 {
		t.Fatal("restart callback must not be called when MaxRestarts=0")
	}
}

// ---- MaxRestarts enforcement ------------------------------------------------

func TestSupervisorExceedMaxRestartsGivesUp(t *testing.T) {
	restarts := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{
			RestartWASM:   true,
			MaxRestarts:   2,
			RestartWindow: time.Minute,
		},
		func(ev session.CrashEvent) error { restarts++; return nil },
	)

	ev := recoverableEv(1)

	// First two restarts must succeed.
	for i := 1; i <= 2; i++ {
		if err := sup.HandleCrash(ev); err != nil {
			t.Fatalf("restart %d should succeed, got: %v", i, err)
		}
	}
	// Third restart must fail (budget exhausted).
	if err := sup.HandleCrash(ev); err == nil {
		t.Fatal("third restart should fail after MaxRestarts=2")
	}
	if restarts != 2 {
		t.Fatalf("want 2 actual restarts, got %d", restarts)
	}
}

func TestSupervisorRestartCountTracker(t *testing.T) {
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 5, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { return nil },
	)
	ev := recoverableEv(7)

	_ = sup.HandleCrash(ev)
	_ = sup.HandleCrash(ev)

	if n := sup.RestartCount(7); n != 2 {
		t.Fatalf("want RestartCount=2, got %d", n)
	}
}

// ---- RestartWindow (sliding window) -----------------------------------------

func TestSupervisorRestartWindowExpiry(t *testing.T) {
	restarts := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{
			RestartWASM:   true,
			MaxRestarts:   1,
			RestartWindow: 20 * time.Millisecond,
		},
		func(ev session.CrashEvent) error { restarts++; return nil },
	)

	ev := recoverableEv(1)

	// First restart: OK.
	if err := sup.HandleCrash(ev); err != nil {
		t.Fatalf("first restart: %v", err)
	}
	// Immediate second: should fail (MaxRestarts=1 exhausted).
	if err := sup.HandleCrash(ev); err == nil {
		t.Fatal("second immediate restart should fail")
	}

	// Wait for the window to expire.
	time.Sleep(30 * time.Millisecond)

	// After expiry the budget resets; restart should succeed again.
	if err := sup.HandleCrash(ev); err != nil {
		t.Fatalf("restart after window expiry: %v", err)
	}
	if restarts != 2 {
		t.Fatalf("want 2 successful restarts, got %d", restarts)
	}
}

// ---- Per-tab isolation ------------------------------------------------------

func TestSupervisorIndependentPerTab(t *testing.T) {
	restarts := map[tab.TabID]int{}
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 1, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { restarts[ev.TabID]++; return nil },
	)

	ev1 := recoverableEv(1)
	ev2 := recoverableEv(2)

	// Each tab gets its own budget.
	if err := sup.HandleCrash(ev1); err != nil {
		t.Fatalf("tab 1 first restart: %v", err)
	}
	if err := sup.HandleCrash(ev2); err != nil {
		t.Fatalf("tab 2 first restart: %v", err)
	}
	// Both budgets now exhausted.
	if err := sup.HandleCrash(ev1); err == nil {
		t.Fatal("tab 1 second restart should fail")
	}
	if err := sup.HandleCrash(ev2); err == nil {
		t.Fatal("tab 2 second restart should fail")
	}
	if restarts[1] != 1 || restarts[2] != 1 {
		t.Fatalf("unexpected restart counts: %v", restarts)
	}
}

// ---- ResetTab ----------------------------------------------------------------

func TestSupervisorResetTab(t *testing.T) {
	restarts := 0
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 1, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { restarts++; return nil },
	)

	ev := recoverableEv(42)
	_ = sup.HandleCrash(ev) // budget used

	// Budget exhausted; next call should fail.
	if err := sup.HandleCrash(ev); err == nil {
		t.Fatal("expected failure after budget exhaustion")
	}

	// Reset clears the history.
	sup.ResetTab(42)

	// Budget is fresh again.
	if err := sup.HandleCrash(ev); err != nil {
		t.Fatalf("restart after ResetTab: %v", err)
	}
	if restarts != 2 {
		t.Fatalf("want 2 restarts, got %d", restarts)
	}
}

// ---- Restart callback error propagation -------------------------------------

func TestSupervisorCallbackErrorPropagates(t *testing.T) {
	boom := errors.New("restart infrastructure unavailable")
	sup := session.NewSupervisor(
		session.CrashPolicy{MaxRestarts: 3, RestartWindow: time.Minute},
		func(ev session.CrashEvent) error { return boom },
	)

	err := sup.HandleCrash(recoverableEv(1))
	if err == nil {
		t.Fatal("want error from restart callback")
	}
	if !errors.Is(err, boom) {
		t.Fatalf("want wrapped boom error, got: %v", err)
	}
}
