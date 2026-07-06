// Package host implements the wazero-based WASI loader for GoWebBrowser wasm
// apps. It wires the gobrowser_dom host import module, mounts the app into the
// engine, and pumps events from the engine into the running guest.
package host

import (
	"context"
	"fmt"
	"log"

	"github.com/monstercameron/gowebbrowser/engine"
	"github.com/monstercameron/gowebbrowser/eventmsg"
	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	wazero_wasi "github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

// Capabilities controls which host import modules the guest may access.
// Wazero will fail instantiation if the guest declares an import not registered
// by the host, so only register modules for capabilities that are granted.
type Capabilities struct {
	// DOM grants access to the gobrowser_dom module (submit_batch).
	// Set to true for any app that renders a UI.
	DOM bool

	// Net lists the URL prefixes the guest may fetch. (Reserved for future
	// network capability module; not yet implemented.)
	Net []string

	// Storage grants access to a future key-value storage module.
	// (Reserved; not yet implemented.)
	Storage bool
}

// Host loads a wasip1 app binary, wires it to an Engine, and pumps events.
type Host struct {
	eng    engine.Engine
	rt     wazero.Runtime
	mod    api.Module
	ctx    context.Context
	cancel context.CancelFunc
}

// New returns a Host backed by eng with default capabilities (DOM=true).
func New(eng engine.Engine) *Host {
	ctx, cancel := context.WithCancel(context.Background())
	return &Host{
		eng:    eng,
		ctx:    ctx,
		cancel: cancel,
	}
}

// Load instantiates the wasm binary. caps controls which import modules are
// registered; pass Capabilities{DOM: true} for a standard UI app.
func (h *Host) Load(ctx context.Context, wasm []byte, caps Capabilities) error {
	cfg := wazero.NewRuntimeConfig().WithCloseOnContextDone(false)
	h.rt = wazero.NewRuntimeWithConfig(ctx, cfg)

	// Always register WASI preview1.
	if _, err := wazero_wasi.Instantiate(ctx, h.rt); err != nil {
		return fmt.Errorf("host: wasi_snapshot_preview1: %w", err)
	}

	// Register gobrowser_dom if the DOM capability is granted.
	if caps.DOM {
		if err := h.registerDOMModule(ctx); err != nil {
			return err
		}
	}

	// Compile and instantiate the guest module.
	compiled, err := h.rt.CompileModule(ctx, wasm)
	if err != nil {
		return fmt.Errorf("host: compile: %w", err)
	}
	modCfg := wazero.NewModuleConfig().WithName("").WithStartFunctions()
	mod, err := h.rt.InstantiateModule(ctx, compiled, modCfg)
	if err != nil {
		return fmt.Errorf("host: instantiate: %w", err)
	}
	h.mod = mod

	// Call _initialize if present (reactor / -buildmode=c-shared pattern).
	if init := mod.ExportedFunction("_initialize"); init != nil {
		if _, err := init.Call(ctx); err != nil {
			return fmt.Errorf("host: _initialize: %w", err)
		}
	}

	return nil
}

// registerDOMModule wires the gobrowser_dom host module. The single export
// submit_batch(ptr i32, len i32) i32 reads [ptr, ptr+len) from guest memory,
// calls eng.Apply, and returns the new revision (0 on mismatch).
func (h *Host) registerDOMModule(ctx context.Context) error {
	submitBatch := func(ctx context.Context, mod api.Module, ptr, length uint32) uint32 {
		buf, ok := mod.Memory().Read(ptr, length)
		if !ok {
			return 0
		}
		rev, err := h.eng.Apply(buf)
		if err != nil {
			// Revision mismatch or decode error: return 0 to signal the guest.
			return 0
		}
		return uint32(rev)
	}

	_, err := h.rt.NewHostModuleBuilder("gobrowser_dom").
		NewFunctionBuilder().
		WithFunc(submitBatch).
		Export("submit_batch").
		Instantiate(ctx)
	if err != nil {
		return fmt.Errorf("host: gobrowser_dom module: %w", err)
	}
	return nil
}

// Mount resolves selector to a root NodeID via the engine, then calls the
// guest's gobrowser_mount export and starts the event-pump goroutine.
func (h *Host) Mount(selector string) error {
	rootID, err := h.eng.RootNode(selector)
	if err != nil {
		return fmt.Errorf("host: RootNode(%q): %w", selector, err)
	}

	fn := h.mod.ExportedFunction("gobrowser_mount")
	if fn == nil {
		return fmt.Errorf("host: guest does not export gobrowser_mount")
	}
	if _, err := fn.Call(h.ctx, uint64(rootID)); err != nil {
		return fmt.Errorf("host: gobrowser_mount: %w", err)
	}

	// Start the event pump in a background goroutine.
	go h.pumpEvents()
	return nil
}

// pumpEvents reads events from the engine and delivers them to the guest via
// gobrowser_alloc + gobrowser_handle_event.
func (h *Host) pumpEvents() {
	allocFn := h.mod.ExportedFunction("gobrowser_alloc")
	handleFn := h.mod.ExportedFunction("gobrowser_handle_event")
	if allocFn == nil || handleFn == nil {
		log.Println("host: guest missing gobrowser_alloc or gobrowser_handle_event; event pump disabled")
		return
	}

	for {
		select {
		case <-h.ctx.Done():
			return
		case ev, ok := <-h.eng.Events():
			if !ok {
				return
			}
			if err := h.deliverEvent(allocFn, handleFn, ev); err != nil {
				log.Printf("host: deliverEvent: %v", err)
			}
		}
	}
}

// deliverEvent encodes ev, allocates guest memory, writes the bytes, and calls
// gobrowser_handle_event.
func (h *Host) deliverEvent(allocFn, handleFn api.Function, ev engine.Event) error {
	buf := eventmsg.Encode(ev)
	n := uint32(len(buf))

	// Ask the guest to allocate a buffer.
	res, err := allocFn.Call(h.ctx, uint64(n))
	if err != nil {
		return fmt.Errorf("gobrowser_alloc: %w", err)
	}
	ptr := uint32(res[0])
	if ptr == 0 {
		return fmt.Errorf("gobrowser_alloc returned null")
	}

	// Write the encoded event into guest linear memory.
	if !h.mod.Memory().Write(ptr, buf) {
		return fmt.Errorf("memory.Write failed at ptr=%d len=%d", ptr, n)
	}

	// Notify the guest.
	if _, err := handleFn.Call(h.ctx, uint64(ptr), uint64(n)); err != nil {
		return fmt.Errorf("gobrowser_handle_event: %w", err)
	}
	return nil
}

// Close stops the event pump and shuts down the wazero runtime.
func (h *Host) Close() error {
	h.cancel()
	if h.mod != nil {
		_ = h.mod.Close(context.Background())
	}
	if h.rt != nil {
		return h.rt.Close(context.Background())
	}
	return nil
}

