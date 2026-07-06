//go:build wasip1

// Counter is a minimal GoWebBrowser app that demonstrates the sdk/app retained
// UI loop. A button shows the current count and increments on each click.
//
// Build: GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o counter.wasm .
//
// The init function registers the render tree with the runtime. In reactor
// (c-shared) mode the Go toolchain generates an _initialize export that runs
// package init functions; main() is NOT called in reactor mode. The host then
// calls gobrowser_mount to trigger the first render.
package main

import "github.com/monstercameron/gowebbrowser/sdk/app"

func init() {
	count := 0
	app.Run(func() *app.Node {
		return app.El("button",
			app.OnClick{Fn: func() { count++ }},
			app.Text(countLabel(count)),
		)
	})
}

// main is required by package main but is never called in reactor mode.
func main() {}

// countLabel formats n as "Count: N" without importing fmt.
func countLabel(n int) string { return "Count: " + itoa(n) }

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
