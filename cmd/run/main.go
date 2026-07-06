//go:build !wasip1

// Command run is the integrated end-to-end demo: it wires the browser shell
// (window + tab + crash policy) to the wasm host and a rendering engine, loads
// the counter app (Go compiled to wasip1 wasm), mounts it, and drives clicks —
// proving the whole no-JavaScript stack composes:
//
//	window -> tab -> host (wazero) -> app.wasm -> GDOM batch -> engine
//
// The engine here is the pure-Go TestDOM; the WebKit2 adapter slots into the
// same engine.Engine seam later.
package main

import (
	"context"
	_ "embed"
	"fmt"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/host"
	"github.com/monstercameron/gowebbrowser/session"
	"github.com/monstercameron/gowebbrowser/tab"
	"github.com/monstercameron/gowebbrowser/window"
)

//go:embed app.wasm
var counterWasm []byte

// appHostAdapter adapts *host.Host to tab.AppHost. (host.Load takes a
// Capabilities arg the tab seam doesn't know about; this is the glue.)
type appHostAdapter struct{ h *host.Host }

func (a *appHostAdapter) Load(ctx context.Context, wasm []byte) error {
	return a.h.Load(ctx, wasm, host.Capabilities{DOM: true})
}
func (a *appHostAdapter) Mount(selector string) error { return a.h.Mount(selector) }
func (a *appHostAdapter) Close() error                { return a.h.Close() }

func main() {
	ctx := context.Background()
	fmt.Println("GoWebBrowser — integrated no-JS stack demo")
	fmt.Println("==========================================")

	// 1. Rendering engine (pure-Go reference; WebKit adapter slots in here).
	eng := engine.NewTestDOM()

	// 2. wasm host bound to the engine, adapted to the tab seam.
	h := host.New(eng)
	ah := &appHostAdapter{h}
	if err := ah.Load(ctx, counterWasm); err != nil {
		panic(fmt.Errorf("load app.wasm: %w", err))
	}

	// 3. Browser shell: manager -> window -> tab.
	mgr := window.NewManager("demo-profile", "demo-session",
		session.CrashPolicy{RestartWASM: true, MaxRestarts: 3, RestartWindow: 30 * time.Second})
	win := mgr.NewWindow()
	t := tab.New(1, eng, ah)
	if err := win.OpenTab(t, true); err != nil {
		panic(err)
	}

	// 4. Navigate -> LoadHTML + Mount (the app's gobrowser_mount renders Count:0).
	if err := t.Navigate(ctx, "app://counter"); err != nil {
		panic(fmt.Errorf("navigate: %w", err))
	}
	fmt.Printf("\n[tab %d] state=%s\n", t.ID(), t.State())
	fmt.Printf("[render @rev %d] %s\n", eng.Revision(), eng.HTML())

	// 5. Drive clicks through the full stack: inject a click at the current
	//    button node; the host pump delivers it to the wasm app, which
	//    re-renders a new GDOM batch back into the engine.
	for i := 1; i <= 3; i++ {
		btn := eng.FindFirst("button")
		if btn == 0 {
			fmt.Println("no button found; stopping")
			break
		}
		before := eng.Revision()
		eng.InjectEvent(engine.Event{Kind: engine.EventClick, Target: btn})

		if waitRevision(eng, before, time.Second) {
			fmt.Printf("[click %d -> rev %d] %s\n", i, eng.Revision(), eng.HTML())
		} else {
			fmt.Printf("[click %d] no re-render within 1s (rev still %d) %s\n", i, eng.Revision(), eng.HTML())
		}
	}

	_ = t.Close()
	fmt.Println("\ndone.")
}

// waitRevision blocks until the engine revision advances past `from`, or timeout.
func waitRevision(eng *engine.TestDOM, from uint64, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if eng.Revision() > from {
			return true
		}
		time.Sleep(2 * time.Millisecond)
	}
	return false
}
