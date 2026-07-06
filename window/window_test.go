package window_test

import (
	"context"
	"testing"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/session"
	"github.com/monstercameron/gowebbrowser/tab"
	"github.com/monstercameron/gowebbrowser/window"
)

// newTab creates a tab wired to a fresh TestDOM and FakeHost.
func newTab(id tab.TabID) *tab.Tab {
	return tab.New(id, engine.NewTestDOM(), &tab.FakeHost{})
}

// navigateTab navigates the given tab to url, failing the test on error.
func navigateTab(tb *testing.T, t *tab.Tab, url string) {
	tb.Helper()
	if err := t.Navigate(context.Background(), url); err != nil {
		tb.Fatalf("Navigate %q: %v", url, err)
	}
}

// ---- Window -----------------------------------------------------------------

func TestWindowOpenAndActiveTab(t *testing.T) {
	mgr := window.NewManager("profile1", "sess1", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1 := newTab(1)
	if err := w.OpenTab(t1, true); err != nil {
		t.Fatalf("OpenTab: %v", err)
	}
	if w.ActiveTab() != t1 {
		t.Fatal("active tab should be t1")
	}
	if w.ActiveIndex() != 0 {
		t.Fatalf("want activeIdx=0, got %d", w.ActiveIndex())
	}
}

func TestWindowFirstTabAutoActivates(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1 := newTab(1)
	// activate=false, but it is the first tab so it should become active anyway.
	if err := w.OpenTab(t1, false); err != nil {
		t.Fatalf("OpenTab: %v", err)
	}
	if w.ActiveTab() != t1 {
		t.Fatal("first tab should auto-activate")
	}
}

func TestWindowOpenMultipleTabs(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1, t2, t3 := newTab(1), newTab(2), newTab(3)
	_ = w.OpenTab(t1, true)
	_ = w.OpenTab(t2, false)
	_ = w.OpenTab(t3, true) // activate t3

	if w.TabCount() != 3 {
		t.Fatalf("want 3 tabs, got %d", w.TabCount())
	}
	if w.ActiveTab() != t3 {
		t.Fatalf("active tab should be t3")
	}
}

func TestWindowActivateTab(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1, t2 := newTab(1), newTab(2)
	_ = w.OpenTab(t1, true)
	_ = w.OpenTab(t2, false)

	if err := w.ActivateTab(t2.ID()); err != nil {
		t.Fatalf("ActivateTab: %v", err)
	}
	if w.ActiveTab() != t2 {
		t.Fatal("want t2 active")
	}
	if w.ActiveIndex() != 1 {
		t.Fatalf("want activeIdx=1, got %d", w.ActiveIndex())
	}
}

func TestWindowActivateUnknownTabErrors(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()
	_ = w.OpenTab(newTab(1), true)

	if err := w.ActivateTab(99); err == nil {
		t.Fatal("want error for unknown tab id")
	}
}

func TestWindowCloseActiveTabShiftsToNext(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1, t2, t3 := newTab(1), newTab(2), newTab(3)
	_ = w.OpenTab(t1, true)
	_ = w.OpenTab(t2, false)
	_ = w.OpenTab(t3, false)
	_ = w.ActivateTab(t1.ID()) // activate first

	// Close the active tab (index 0); index 0 in the new strip is t2.
	if err := w.CloseTab(t1.ID()); err != nil {
		t.Fatalf("CloseTab: %v", err)
	}
	if w.TabCount() != 2 {
		t.Fatalf("want 2 tabs, got %d", w.TabCount())
	}
	// After removing t1 (idx 0) the next tab at that position is t2.
	if w.ActiveTab() != t2 {
		t.Fatalf("want t2 active after closing t1, got %v", w.ActiveTab())
	}
}

func TestWindowCloseLastTab(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1 := newTab(1)
	_ = w.OpenTab(t1, true)
	if err := w.CloseTab(t1.ID()); err != nil {
		t.Fatalf("CloseTab: %v", err)
	}
	if w.TabCount() != 0 {
		t.Fatalf("want 0 tabs, got %d", w.TabCount())
	}
	if w.ActiveTab() != nil {
		t.Fatal("want nil active tab on empty window")
	}
	if w.ActiveIndex() != -1 {
		t.Fatalf("want activeIdx=-1, got %d", w.ActiveIndex())
	}
}

func TestWindowCloseNonActiveTab(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1, t2, t3 := newTab(1), newTab(2), newTab(3)
	_ = w.OpenTab(t1, true)
	_ = w.OpenTab(t2, false)
	_ = w.OpenTab(t3, false)
	_ = w.ActivateTab(t2.ID()) // active = t2 (idx 1)

	// Close t3 (idx 2); active t2 should remain active.
	if err := w.CloseTab(t3.ID()); err != nil {
		t.Fatalf("CloseTab t3: %v", err)
	}
	if w.ActiveTab() != t2 {
		t.Fatal("t2 should remain active")
	}
}

func TestWindowMoveTab(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	t1, t2, t3 := newTab(1), newTab(2), newTab(3)
	_ = w.OpenTab(t1, true)
	_ = w.OpenTab(t2, false)
	_ = w.OpenTab(t3, false)
	// Strip: [t1, t2, t3]; active = t1

	// Move t1 to index 2: strip should become [t2, t3, t1].
	if err := w.MoveTab(t1.ID(), 2); err != nil {
		t.Fatalf("MoveTab: %v", err)
	}
	tabs := w.Tabs()
	if tabs[0] != t2 || tabs[1] != t3 || tabs[2] != t1 {
		t.Fatalf("unexpected tab order after MoveTab: %v", tabs)
	}
	// Active tab (t1) must still be active and at the new index.
	if w.ActiveTab() != t1 {
		t.Fatal("moved tab should remain active")
	}
	if w.ActiveIndex() != 2 {
		t.Fatalf("want activeIdx=2, got %d", w.ActiveIndex())
	}
}

func TestWindowMoveTabOutOfRange(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()
	_ = w.OpenTab(newTab(1), true)
	_ = w.OpenTab(newTab(2), false)

	if err := w.MoveTab(1, 5); err == nil {
		t.Fatal("want error for out-of-range target")
	}
}

func TestWindowHasTab(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()
	t1 := newTab(1)
	_ = w.OpenTab(t1, true)

	if !w.HasTab(1) {
		t.Fatal("HasTab should return true for existing tab")
	}
	if w.HasTab(99) {
		t.Fatal("HasTab should return false for absent tab")
	}
}

func TestWindowClosedBlocksOperations(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()
	_ = w.OpenTab(newTab(1), true)

	if err := mgr.CloseWindow(w.ID()); err != nil {
		t.Fatalf("CloseWindow: %v", err)
	}
	if !w.IsClosed() {
		t.Fatal("window should be marked closed")
	}
	if err := w.OpenTab(newTab(2), true); err == nil {
		t.Fatal("OpenTab should fail on closed window")
	}
}

// ---- Manager ----------------------------------------------------------------

func TestManagerWindowRegistry(t *testing.T) {
	mgr := window.NewManager("profile", "session", session.CrashPolicy{})
	if mgr.WindowCount() != 0 {
		t.Fatal("new manager should have 0 windows")
	}

	w1 := mgr.NewWindow()
	w2 := mgr.NewWindow()

	if mgr.WindowCount() != 2 {
		t.Fatalf("want 2 windows, got %d", mgr.WindowCount())
	}
	if got, ok := mgr.Window(w1.ID()); !ok || got != w1 {
		t.Fatal("Manager.Window should return w1")
	}
	if got, ok := mgr.Window(w2.ID()); !ok || got != w2 {
		t.Fatal("Manager.Window should return w2")
	}
}

func TestManagerWindowIDsAreUnique(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	ids := make(map[window.WindowID]bool)
	for i := 0; i < 10; i++ {
		w := mgr.NewWindow()
		if ids[w.ID()] {
			t.Fatalf("duplicate window ID %d", w.ID())
		}
		ids[w.ID()] = true
	}
}

func TestManagerCloseWindowRemovesFromRegistry(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	if err := mgr.CloseWindow(w.ID()); err != nil {
		t.Fatalf("CloseWindow: %v", err)
	}
	if _, ok := mgr.Window(w.ID()); ok {
		t.Fatal("closed window should be removed from registry")
	}
	if mgr.WindowCount() != 0 {
		t.Fatalf("want 0 windows, got %d", mgr.WindowCount())
	}
}

func TestManagerCloseUnknownWindowErrors(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	if err := mgr.CloseWindow(999); err == nil {
		t.Fatal("want error closing unknown window")
	}
}

func TestManagerProfileAndSession(t *testing.T) {
	mgr := window.NewManager("myProfile", "mySession", session.CrashPolicy{})
	if mgr.ProfileID() != "myProfile" {
		t.Fatalf("ProfileID = %q", mgr.ProfileID())
	}
	if mgr.SessionID() != "mySession" {
		t.Fatalf("SessionID = %q", mgr.SessionID())
	}
}

func TestManagerPolicy(t *testing.T) {
	policy := session.CrashPolicy{
		RestartWASM:   true,
		MaxRestarts:   5,
		RestartWindow: 30 * time.Second,
	}
	mgr := window.NewManager("p", "s", policy)
	got := mgr.Policy()
	if got.MaxRestarts != 5 || got.RestartWindow != 30*time.Second {
		t.Fatalf("unexpected policy: %+v", got)
	}
}

// TestActiveTabBookkeepingAfterMultipleCloses exercises the active-index
// invariant across a sequence of close operations.
func TestActiveTabBookkeepingAfterMultipleCloses(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()

	tabs := make([]*tab.Tab, 5)
	for i := range tabs {
		tabs[i] = newTab(tab.TabID(i + 1))
		_ = w.OpenTab(tabs[i], false)
	}
	_ = w.ActivateTab(tabs[4].ID()) // activate last tab

	// Close tab at index 4 — activeIdx should fall back to index 3.
	_ = w.CloseTab(tabs[4].ID())
	if w.ActiveTab() != tabs[3] {
		t.Fatalf("after closing last tab, active should be tabs[3]")
	}

	// Close tab at index 1 (tabs[1]) — does not affect activeIdx of tabs[3].
	_ = w.CloseTab(tabs[1].ID())
	// tabs[3] is now at index 2.
	if w.ActiveTab() != tabs[3] {
		t.Fatalf("active should still be tabs[3] after closing an earlier tab")
	}
	if w.ActiveIndex() != 2 {
		t.Fatalf("want activeIdx=2, got %d", w.ActiveIndex())
	}
}

// TestNavigateIntegration verifies that tab lifecycle integrates with the
// window layer without panics.
func TestNavigateIntegration(t *testing.T) {
	mgr := window.NewManager("p", "s", session.CrashPolicy{})
	w := mgr.NewWindow()
	tb := newTab(1)
	_ = w.OpenTab(tb, true)

	navigateTab(t, tb, "https://example.com")

	if w.ActiveTab().State() != tab.TabActive {
		t.Fatalf("want Active, got %s", w.ActiveTab().State())
	}
}
