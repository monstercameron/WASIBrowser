// Command hello is the minimal wasip1 guest: prove that a Go program runs
// inside the Rust renderer's wasmtime host and can reach its console.
package main

import (
	"fmt"
	"runtime"
)

func main() {
	fmt.Printf("hello from Go %s (%s/%s) inside the Rust renderer\n",
		runtime.Version(), runtime.GOOS, runtime.GOARCH)
}
