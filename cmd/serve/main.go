//go:build !wasip1

// Command serve renders the GoWebBrowser counter app in your real browser with
// ZERO JavaScript. The visible surface is plain HTML: the app's <button> sits
// inside a <form method=POST>, so clicking it submits (a default-submit button)
// and the server round-trips a fresh render. No JS, no client-side DOM patching.
//
// The full no-JS stack drives it:
//
//	browser (form POST) -> Go server -> engine.InjectEvent -> host pump ->
//	app.wasm handler -> GDOM batch -> engine -> serialized HTML -> browser
//
// The engine here is the pure-Go TestDOM; the WebKit2 adapter slots into the
// same engine.Engine seam later (then the browser renders via WebKit instead of
// this server-render shim).
package main

import (
	"context"
	_ "embed"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/host"
)

//go:embed app.wasm
var counterWasm []byte

const addr = "127.0.0.1:8099"

var eng *engine.TestDOM

func main() {
	ctx := context.Background()

	eng = engine.NewTestDOM()
	h := host.New(eng)
	if err := h.Load(ctx, counterWasm, host.Capabilities{DOM: true}); err != nil {
		log.Fatalf("load app.wasm: %v", err)
	}
	if err := h.Mount("#app"); err != nil {
		log.Fatalf("mount: %v", err)
	}

	http.HandleFunc("/", handleIndex)
	http.HandleFunc("/click", handleClick)

	fmt.Printf("\n  GoWebBrowser counter — no JavaScript\n")
	fmt.Printf("  open  http://%s  and click the button\n\n", addr)
	log.Fatal(http.ListenAndServe(addr, nil))
}

// handleClick injects a click at the current button node, waits for the wasm app
// to re-render, then redirects back to the page (Post/Redirect/Get).
func handleClick(w http.ResponseWriter, r *http.Request) {
	if btn := eng.FindFirst("button"); btn != 0 {
		before := eng.Revision()
		eng.InjectEvent(engine.Event{Kind: engine.EventClick, Target: btn})
		waitRevision(before, time.Second)
	}
	http.Redirect(w, r, "/", http.StatusSeeOther)
}

// handleIndex serves the page. The app's rendered HTML (including its <button>)
// is wrapped in a <form method=POST>; a bare <button> inside a form defaults to
// type=submit, so clicking it posts to /click with no JavaScript.
func handleIndex(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	fmt.Fprintf(w, page, eng.HTML(), eng.Revision())
}

func waitRevision(from uint64, timeout time.Duration) {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if eng.Revision() > from {
			return
		}
		time.Sleep(2 * time.Millisecond)
	}
}

// page is the host document. CSS only (CSS is not JavaScript). %s = app HTML,
// %d = engine revision.
const page = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>GoWebBrowser — Counter</title>
<style>
  :root { color-scheme: dark; }
  body { font-family: system-ui, sans-serif; background:#0b1020; color:#e6e9f0;
         margin:0; min-height:100vh; display:flex; align-items:center; justify-content:center; }
  .card { background:#161c2e; padding:44px 60px; border-radius:18px;
          box-shadow:0 14px 50px rgba(0,0,0,.45); text-align:center; }
  h1 { margin:0 0 18px; font-size:13px; letter-spacing:.14em; text-transform:uppercase; color:#8b93a7; }
  form { margin:0; }
  #app button { font-size:22px; font-weight:600; padding:16px 40px; border:0; border-radius:12px;
                background:#4f7cff; color:#fff; cursor:pointer; transition:background .12s; }
  #app button:hover { background:#3a63d0; }
  #app button:active { transform:translateY(1px); }
  .foot { margin-top:22px; font-size:12px; color:#5b6478; line-height:1.6; }
  code { color:#8fa3ff; }
</style>
</head>
<body>
  <div class="card">
    <h1>GoWebBrowser &middot; no&#8209;JS</h1>
    <form method="POST" action="/click">
      %s
    </form>
    <div class="foot">
      browser &rarr; Go &rarr; wasm &rarr; GDOM &rarr; engine &nbsp;&middot;&nbsp; rev %d<br>
      zero JavaScript &mdash; the button is an HTML <code>form</code> submit
    </div>
  </div>
</body>
</html>
`
