package host_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/host"
	"github.com/monstercameron/gowebbrowser/protocol"
)

// buildCounterBytes compiles examples/counter for wasip1 and returns the wasm
// bytes. The test is skipped if the build fails (e.g., missing toolchain).
func buildCounterBytes(t *testing.T) []byte {
	t.Helper()

	out := filepath.Join(t.TempDir(), "counter.wasm")

	// `go test ./host/` is run from the module root, so the counter package is
	// reachable via the relative path ../examples/counter.
	counterPkg, err := filepath.Abs(filepath.Join("..", "examples", "counter"))
	if err != nil {
		t.Skipf("cannot resolve counter package path: %v", err)
	}

	cmd := exec.Command("go", "build",
		"-buildmode=c-shared",
		"-o", out,
		counterPkg,
	)
	cmd.Env = append(os.Environ(), "GOOS=wasip1", "GOARCH=wasm")
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Skipf("wasip1 build failed — skipping host integration test: %v\n%s", err, output)
	}

	data, err := os.ReadFile(out)
	if err != nil {
		t.Fatalf("read wasm: %v", err)
	}
	return data
}

func TestHostCounterIntegration(t *testing.T) {
	wasm := buildCounterBytes(t)

	dom := engine.NewTestDOM()
	h := host.New(dom)
	defer h.Close()

	ctx := context.Background()
	if err := h.Load(ctx, wasm, host.Capabilities{DOM: true}); err != nil {
		t.Fatalf("Load: %v", err)
	}
	if err := h.Mount("#app"); err != nil {
		t.Fatalf("Mount: %v", err)
	}

	// After mount the guest has rendered "Count: 0".
	assertHTMLContains(t, dom, "Count: 0", 5*time.Second)

	// The counter example builds: El("button", OnClick{...}, Text("Count: 0"))
	// Nodes are allocated monotonically; nextNodeID starts at 1 and allocID
	// adds 1 before returning, so the button element is always NodeID 2.
	dom.InjectEvent(engine.Event{
		Kind:   engine.EventClick,
		Target: protocol.NodeID(2),
	})

	// Allow the event pump goroutine to deliver the click and the guest to
	// re-render before asserting.
	assertHTMLContains(t, dom, "Count: 1", 5*time.Second)
}

// assertHTMLContains polls dom.HTML() until it contains want or timeout expires.
func assertHTMLContains(t *testing.T, dom *engine.TestDOM, want string, timeout time.Duration) {
	t.Helper()
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if strings.Contains(dom.HTML(), want) {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("timed out: HTML does not contain %q\nHTML: %s", want, dom.HTML())
}
