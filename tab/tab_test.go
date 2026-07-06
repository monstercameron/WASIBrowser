package tab_test

import (
	"context"
	"errors"
	"testing"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/tab"
)

// newTestTab returns a Tab wired to a TestDOM renderer and a FakeHost.
func newTestTab(id tab.TabID) (*tab.Tab, *engine.TestDOM, *tab.FakeHost) {
	dom := engine.NewTestDOM()
	host := &tab.FakeHost{}
	t := tab.New(id, dom, host)
	return t, dom, host
}

// ---- state transitions -------------------------------------------------------

func TestInitialState(t *testing.T) {
	tb, _, _ := newTestTab(1)
	if tb.State() != tab.TabCreated {
		t.Fatalf("want Created, got %s", tb.State())
	}
}

func TestNavigateCreatedToActive(t *testing.T) {
	tb, _, host := newTestTab(1)
	ctx := context.Background()

	if err := tb.Navigate(ctx, "about:blank"); err != nil {
		t.Fatalf("Navigate: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active, got %s", tb.State())
	}
	if host.MountCount() != 1 {
		t.Fatalf("want 1 Mount call, got %d", host.MountCount())
	}
}

func TestIllegalTransitionFrozenToLoading(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()

	// Created -> Loading -> Active -> Frozen
	_ = tb.Navigate(ctx, "about:blank")
	if err := tb.Freeze(); err != nil {
		t.Fatalf("Freeze: %v", err)
	}
	// Frozen -> Loading is valid (Navigate is allowed from Frozen).
	// Actually, let's test a truly illegal transition: Frozen -> Background.
	if err := tb.Background(); !errors.Is(err, tab.ErrInvalidTransition) {
		t.Fatalf("want ErrInvalidTransition, got %v", err)
	}
}

func TestIllegalTransitionClosedToActive(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")
	if err := tb.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	// Any operation on a closed tab must fail.
	if err := tb.Activate(); !errors.Is(err, tab.ErrInvalidTransition) {
		t.Fatalf("want ErrInvalidTransition after Close, got %v", err)
	}
}

func TestIllegalTransitionCreatedToFrozen(t *testing.T) {
	tb, _, _ := newTestTab(1)
	if err := tb.Freeze(); !errors.Is(err, tab.ErrInvalidTransition) {
		t.Fatalf("want ErrInvalidTransition from Created, got %v", err)
	}
}

// ---- Freeze / Resume ---------------------------------------------------------

func TestFreezeResume(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	if err := tb.Freeze(); err != nil {
		t.Fatalf("Freeze: %v", err)
	}
	if tb.State() != tab.TabFrozen {
		t.Fatalf("want Frozen, got %s", tb.State())
	}

	if err := tb.Resume(); err != nil {
		t.Fatalf("Resume: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active after Resume, got %s", tb.State())
	}
}

func TestFreezeFromBackground(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	_ = tb.Background()
	if err := tb.Freeze(); err != nil {
		t.Fatalf("Freeze from Background: %v", err)
	}
	if tb.State() != tab.TabFrozen {
		t.Fatalf("want Frozen, got %s", tb.State())
	}
}

// ---- Discard / Restore -------------------------------------------------------

func TestDiscardRequiresFrozenOrBackground(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")

	// Active -> Discard is not valid.
	if err := tb.Discard(); !errors.Is(err, tab.ErrInvalidTransition) {
		t.Fatalf("want ErrInvalidTransition for Active->Discard, got %v", err)
	}
}

func TestDiscardAndRestoreRoundTrip(t *testing.T) {
	tb, _, host := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	// Capture snapshot before discarding.
	snap, err := tb.Snapshot()
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}

	_ = tb.Background()
	if err := tb.Discard(); err != nil {
		t.Fatalf("Discard: %v", err)
	}
	if tb.State() != tab.TabDiscarded {
		t.Fatalf("want Discarded, got %s", tb.State())
	}

	if err := tb.Restore(ctx, snap); err != nil {
		t.Fatalf("Restore: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active after Restore, got %s", tb.State())
	}
	// Mount should have been called twice: once for Navigate, once for Restore.
	if host.MountCount() != 2 {
		t.Fatalf("want 2 Mount calls, got %d", host.MountCount())
	}
}

// ---- KillApp / crash ---------------------------------------------------------

func TestKillApp(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")

	if err := tb.KillApp(); err != nil {
		t.Fatalf("KillApp: %v", err)
	}
	if tb.State() != tab.TabCrashed {
		t.Fatalf("want Crashed, got %s", tb.State())
	}
}

func TestKillAppFromBackground(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")
	_ = tb.Background()

	if err := tb.KillApp(); err != nil {
		t.Fatalf("KillApp from Background: %v", err)
	}
	if tb.State() != tab.TabCrashed {
		t.Fatalf("want Crashed, got %s", tb.State())
	}
}

func TestRestoreAfterCrash(t *testing.T) {
	tb, _, host := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	snap, _ := tb.Snapshot()
	_ = tb.KillApp()

	if err := tb.Restore(ctx, snap); err != nil {
		t.Fatalf("Restore after crash: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active after crash-restore, got %s", tb.State())
	}
	// Navigate + Restore each call Mount once.
	if host.MountCount() != 2 {
		t.Fatalf("want 2 Mount calls, got %d", host.MountCount())
	}
}

func TestRestoreWithNilSnapshotErrors(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")
	_ = tb.KillApp()

	if err := tb.Restore(ctx, nil); err == nil {
		t.Fatal("expected error for nil snapshot")
	}
}

// ---- Reload ------------------------------------------------------------------

func TestReloadFromActive(t *testing.T) {
	tb, _, host := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	if err := tb.Reload(ctx); err != nil {
		t.Fatalf("Reload from Active: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active after Reload, got %s", tb.State())
	}
	// Navigate + Reload each call Mount.
	if host.MountCount() != 2 {
		t.Fatalf("want 2 Mount calls, got %d", host.MountCount())
	}
}

func TestReloadFromCrashed(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")
	_ = tb.KillApp()

	if err := tb.Reload(ctx); err != nil {
		t.Fatalf("Reload from Crashed: %v", err)
	}
	if tb.State() != tab.TabActive {
		t.Fatalf("want Active, got %s", tb.State())
	}
}

// ---- Snapshot ----------------------------------------------------------------

func TestSnapshotCapturesDOMRevision(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "https://example.com")

	snap, err := tb.Snapshot()
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if snap.URL != "https://example.com" {
		t.Fatalf("snapshot URL = %q, want %q", snap.URL, "https://example.com")
	}
	if snap.CapturedAt.IsZero() {
		t.Fatal("CapturedAt is zero")
	}
	if tb.LastSnapshot() != snap {
		t.Fatal("LastSnapshot did not return the most recent snapshot")
	}
}

func TestSnapshotOnClosedTabErrors(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")
	_ = tb.Close()

	if _, err := tb.Snapshot(); !errors.Is(err, tab.ErrTabClosed) {
		t.Fatalf("want ErrTabClosed, got %v", err)
	}
}

// ---- Close -------------------------------------------------------------------

func TestCloseReleasesResources(t *testing.T) {
	tb, _, host := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")

	if err := tb.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if !host.CloseCalled {
		t.Fatal("host.Close was not called")
	}
	if tb.State() != tab.TabClosed {
		t.Fatalf("want Closed, got %s", tb.State())
	}
}

func TestCloseIsIdempotent(t *testing.T) {
	tb, _, _ := newTestTab(1)
	ctx := context.Background()
	_ = tb.Navigate(ctx, "about:blank")
	_ = tb.Close()

	if err := tb.Close(); err != nil {
		t.Fatalf("second Close must be a no-op, got: %v", err)
	}
}

// ---- MountError propagation --------------------------------------------------

func TestNavigateMountErrorTransitionsToCrashed(t *testing.T) {
	dom := engine.NewTestDOM()
	host := &tab.FakeHost{MountErr: errors.New("mount failed")}
	tb := tab.New(1, dom, host)
	ctx := context.Background()

	err := tb.Navigate(ctx, "about:blank")
	if err == nil {
		t.Fatal("expected error from failing Mount")
	}
	if tb.State() != tab.TabCrashed {
		t.Fatalf("want Crashed after Mount failure, got %s", tb.State())
	}
}
