// Command broker is a runnable demonstration of the GoWebBrowser shell layer.
//
// It exercises the full lifecycle path:
//
//	Manager → Window → Tab (Create → Load → Active → Background → Frozen → Resume)
//
// Then simulates a WASM crash and shows the Supervisor restarting the tab up to
// MaxRestarts, after which further crash attempts are rejected.
//
// Run with:
//
//	go run ./cmd/broker
package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/session"
	"github.com/monstercameron/gowebbrowser/tab"
	"github.com/monstercameron/gowebbrowser/window"
)

func main() {
	fmt.Println("GoWebBrowser Broker Demo")
	fmt.Println("========================")
	fmt.Println()

	// ------------------------------------------------------------------ //
	// 1. Build the manager + window.
	// ------------------------------------------------------------------ //
	policy := session.CrashPolicy{
		RestartWASM:        true,
		ReloadRenderer:     true,
		RestoreDOMSnapshot: true,
		MaxRestarts:        3,
		RestartWindow:      30 * time.Second,
	}
	mgr := window.NewManager("demo-profile", "demo-session", policy)
	win := mgr.NewWindow()
	fmt.Printf("[manager]  profile=%s  session=%s\n", mgr.ProfileID(), mgr.SessionID())
	fmt.Printf("[manager]  policy: max=%d restarts\n", policy.MaxRestarts)
	fmt.Println()

	// ------------------------------------------------------------------ //
	// 2. Create a Tab using TestDOM as the renderer and FakeHost as the
	//    WASM app host.
	// ------------------------------------------------------------------ //
	renderer := engine.NewTestDOM()
	host := &tab.FakeHost{}
	tb := tab.New(1, renderer, host)

	printState(tb, "after New")

	if err := win.OpenTab(tb, true); err != nil {
		log.Fatalf("OpenTab: %v", err)
	}
	fmt.Printf("[window %d] opened tab %d (active)\n\n", win.ID(), tb.ID())

	// ------------------------------------------------------------------ //
	// 3. Navigate: Created → Loading → Active.
	// ------------------------------------------------------------------ //
	ctx := context.Background()
	if err := tb.Navigate(ctx, "about:blank"); err != nil {
		log.Fatalf("Navigate: %v", err)
	}
	printState(tb, "after Navigate(\"about:blank\")")
	fmt.Printf("           host.Mount calls: %d  selectors: %v\n\n",
		host.MountCount(), host.MountCalls)

	// Set up a DOM root so HTML() returns something visible.
	rootID, err := renderer.RootNode("#app")
	if err != nil {
		log.Fatalf("RootNode: %v", err)
	}
	fmt.Printf("[renderer] root node id=%d  HTML=%s\n\n", rootID, renderer.HTML())

	// ------------------------------------------------------------------ //
	// 4. Background → Freeze → Resume.
	// ------------------------------------------------------------------ //
	if err := tb.Background(); err != nil {
		log.Fatalf("Background: %v", err)
	}
	printState(tb, "after Background()")

	if err := tb.Freeze(); err != nil {
		log.Fatalf("Freeze: %v", err)
	}
	printState(tb, "after Freeze()")

	// Capture a snapshot while frozen.
	snap, err := tb.Snapshot()
	if err != nil {
		log.Fatalf("Snapshot: %v", err)
	}
	fmt.Printf("[snapshot] url=%q  domRev=%d  capturedAt=%s\n\n",
		snap.URL, snap.DOMRevision, snap.CapturedAt.Format("15:04:05.000"))

	if err := tb.Resume(); err != nil {
		log.Fatalf("Resume: %v", err)
	}
	printState(tb, "after Resume()")

	// ------------------------------------------------------------------ //
	// 5. Simulate a crash + Supervisor-driven restart.
	// ------------------------------------------------------------------ //
	fmt.Println("--- crash simulation ---")
	fmt.Println()

	// Build the supervisor with the manager's policy.
	// The RestartFunc restores the tab from its last snapshot.
	sup := session.NewSupervisor(mgr.Policy(), func(ev session.CrashEvent) error {
		fmt.Printf("[supervisor] restarting tab %d (url=%s)\n", ev.TabID, ev.LastURL)
		lastSnap := tb.LastSnapshot()
		if lastSnap == nil {
			lastSnap = &tab.TabSnapshot{URL: ev.LastURL}
		}
		if err := tb.Restore(ctx, lastSnap); err != nil {
			return fmt.Errorf("restore: %w", err)
		}
		printState(tb, "after Restore")
		return nil
	})

	// Kill the app (Active → Crashed).
	if err := tb.KillApp(); err != nil {
		log.Fatalf("KillApp: %v", err)
	}
	printState(tb, "after KillApp")

	// Report the crash to the supervisor; it should restart the tab.
	crashEv := session.CrashEvent{
		Kind:        session.ProcessWASM,
		TabID:       tb.ID(),
		Reason:      "simulated wasm panic",
		ExitCode:    137,
		Recoverable: true,
		LastURL:     tb.URL(),
	}

	for attempt := 1; ; attempt++ {
		fmt.Printf("\n[crash %d] HandleCrash...\n", attempt)
		if err := sup.HandleCrash(crashEv); err != nil {
			fmt.Printf("[supervisor] gave up: %v\n", err)
			break
		}
		fmt.Printf("[supervisor] restart %d succeeded  restartCount=%d\n",
			attempt, sup.RestartCount(tb.ID()))

		// Crash again to drive the budget down.
		if err := tb.KillApp(); err != nil {
			// Tab might already be in a state that disallows KillApp after
			// the budget is exhausted; stop here.
			fmt.Printf("[demo] KillApp: %v — stopping\n", err)
			break
		}
		printState(tb, fmt.Sprintf("after KillApp (attempt %d)", attempt))
	}

	// ------------------------------------------------------------------ //
	// 6. Summary.
	// ------------------------------------------------------------------ //
	fmt.Println()
	fmt.Printf("[summary]  window=%d  tabs=%d  active=%d\n",
		win.ID(), win.TabCount(), win.ActiveTab().ID())
	fmt.Printf("[summary]  renderer HTML: %s\n", renderer.HTML())
	fmt.Printf("[summary]  total host.Mount calls: %d\n", host.MountCount())
	fmt.Println()
	fmt.Println("Done.")
}

// printState prints the tab ID and current state with a label.
func printState(t *tab.Tab, label string) {
	fmt.Printf("[tab %d]    %-16s  state=%s\n", t.ID(), label, t.State())
}
