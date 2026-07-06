package app_test

import (
	"strings"
	"testing"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/eventmsg"
	"github.com/monstercameron/gowebbrowser/sdk/app"
	"github.com/monstercameron/gowebbrowser/sdk/dom"
)

// applyBatch decodes buf into td, failing the test on error.
func applyBatch(t *testing.T, td *engine.TestDOM, buf []byte) {
	t.Helper()
	if len(buf) == 0 {
		t.Fatal("batch buffer is empty")
	}
	if _, err := td.Apply(buf); err != nil {
		t.Fatalf("TestDOM.Apply: %v", err)
	}
}

func TestNativeCounterRender(t *testing.T) {
	// Reset runtime state so this test is independent of import order / other
	// tests that might have run beforehand.
	app.NativeReset()
	dom.ResetRevision()

	count := 0
	app.Run(func() *app.Node {
		return app.El("button",
			app.OnClick{Fn: func() { count++ }},
			app.Text(countLabel(count)),
		)
	})

	td := engine.NewTestDOM()

	// TestDOM.RootNode creates node 1 (the mount root) so that Append(1, child)
	// ops in the batch find a live parent. NativeMount(1) tells the app the
	// same root ID.
	if _, err := td.RootNode("#app"); err != nil {
		t.Fatalf("RootNode: %v", err)
	}
	app.NativeMount(1)

	applyBatch(t, td, dom.LastBatch)

	if html := td.HTML(); !strings.Contains(html, "Count: 0") {
		t.Fatalf("after mount: want 'Count: 0' in HTML, got:\n%s", html)
	}

	// Ask the app which NodeIDs have click handlers — pick the first one.
	ids := app.NativeClickHandlerIDs()
	if len(ids) == 0 {
		t.Fatal("no click handlers registered after mount")
	}
	buttonID := ids[0]

	// Encode and deliver a click event on the button.
	evBuf := eventmsg.Encode(engine.Event{
		Kind:   engine.EventClick,
		Target: buttonID,
	})
	app.NativeHandleEvent(evBuf)

	applyBatch(t, td, dom.LastBatch)

	if html := td.HTML(); !strings.Contains(html, "Count: 1") {
		t.Fatalf("after click: want 'Count: 1' in HTML, got:\n%s", html)
	}
}

func countLabel(n int) string {
	return "Count: " + itoa(n)
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	buf := [20]byte{}
	pos := len(buf)
	for n > 0 {
		pos--
		buf[pos] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[pos:])
}
