// Command server is the WASIBrowser storefront RPC backend.
//
// It speaks the local-dev rpc-over-HTTP binding from docs/04-WEB-RPC.md:
//
//	POST /rpc/{iface}/{method}
//	  headers: GWB-Req-Id, GWB-App-Key, GWB-Sig, GWB-Ts, GWB-Session
//	  body:    request payload (JSON)
//	  reply:   JSON body + HTTP status (4xx/5xx map to err_class host-side)
//
// Dispatch is a method table (iface -> method -> handler), the RPC-first shape:
// the server's API *is* its method table. Authn (ed25519 channel sig) and authz
// (per-method guard) are layered in Phase 2; Phase 1 is the transport + echo.
package main

import (
	"encoding/json"
	"flag"
	"io"
	"log"
	"net/http"
	"strings"
	"time"
)

// rpcCtx carries one call's decoded request to a handler.
type rpcCtx struct {
	Iface   string
	Method  string
	ReqID   string
	AppKey  string          // GWB-App-Key (channel identity; verified in Phase 2)
	Session string          // GWB-Session (user principal; Phase 2)
	Body    []byte          // raw request payload
	r       *http.Request   // underlying request (rarely needed)
}

// bind decodes the JSON body into v. Returns an rpcError on malformed input.
func (c *rpcCtx) bind(v any) *rpcError {
	if len(c.Body) == 0 {
		return nil
	}
	if err := json.Unmarshal(c.Body, v); err != nil {
		return &rpcError{Status: http.StatusBadRequest, Msg: "bad json: " + err.Error()}
	}
	return nil
}

// rpcError is a handler failure that maps to an HTTP status (and, host-side, an
// err_class). See docs/04-WEB-RPC.md §1.
type rpcError struct {
	Status int
	Msg    string
}

func (e *rpcError) Error() string { return e.Msg }

func errUnauthorized(msg string) *rpcError { return &rpcError{http.StatusUnauthorized, msg} }
func errForbidden(msg string) *rpcError    { return &rpcError{http.StatusForbidden, msg} }
func errNotFound(msg string) *rpcError     { return &rpcError{http.StatusNotFound, msg} }
func errBadRequest(msg string) *rpcError   { return &rpcError{http.StatusBadRequest, msg} }

// handler is one RPC method. It returns a JSON-serializable reply or an error.
type handler func(c *rpcCtx) (any, *rpcError)

// server holds the method table and (Phase 2) the signing key + user store.
type server struct {
	methods map[string]map[string]handler // iface -> method -> handler
}

func newServer() *server {
	s := &server{methods: map[string]map[string]handler{}}
	s.register("gwb.echo.v1", "echo", s.echo)
	return s
}

func (s *server) register(iface, method string, h handler) {
	if s.methods[iface] == nil {
		s.methods[iface] = map[string]handler{}
	}
	s.methods[iface][method] = h
}

// echo is the Phase 1 smoke handler: it reflects the request so the whole
// rpc_call -> host -> server -> RPC_RESULT loop is provable end to end.
func (s *server) echo(c *rpcCtx) (any, *rpcError) {
	var payload any
	_ = json.Unmarshal(c.Body, &payload) // best-effort; echo raw if not JSON
	return map[string]any{
		"iface":   c.Iface,
		"method":  c.Method,
		"reqId":   c.ReqID,
		"appKey":  c.AppKey,
		"gotBody": payload,
		"ts":      time.Now().UnixMilli(),
	}, nil
}

// ServeHTTP routes POST /rpc/{iface}/{method} to the method table.
func (s *server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	start := time.Now()
	if r.Method != http.MethodPost {
		writeErr(w, &rpcError{http.StatusMethodNotAllowed, "rpc is POST-only"})
		return
	}
	// path: /rpc/{iface}/{method}
	rest, ok := strings.CutPrefix(r.URL.Path, "/rpc/")
	if !ok {
		writeErr(w, errNotFound("not an /rpc/ path"))
		return
	}
	iface, method, ok := strings.Cut(rest, "/")
	if !ok || iface == "" || method == "" {
		writeErr(w, errNotFound("expected /rpc/{iface}/{method}"))
		return
	}
	body, _ := io.ReadAll(io.LimitReader(r.Body, 1<<20)) // 1 MiB cap
	c := &rpcCtx{
		Iface:   iface,
		Method:  method,
		ReqID:   r.Header.Get("GWB-Req-Id"),
		AppKey:  r.Header.Get("GWB-App-Key"),
		Session: r.Header.Get("GWB-Session"),
		Body:    body,
		r:       r,
	}

	methods, ok := s.methods[iface]
	if !ok {
		writeErr(w, errNotFound("unknown iface: "+iface))
		return
	}
	h, ok := methods[method]
	if !ok {
		writeErr(w, errNotFound("unknown method: "+iface+"."+method))
		return
	}

	reply, rerr := h(c)
	if rerr != nil {
		log.Printf("rpc %s.%s req=%s -> ERR %d %s (%s)", iface, method, c.ReqID, rerr.Status, rerr.Msg, time.Since(start))
		writeErr(w, rerr)
		return
	}
	log.Printf("rpc %s.%s req=%s -> OK (%s)", iface, method, c.ReqID, time.Since(start))
	writeJSON(w, http.StatusOK, reply)
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeErr(w http.ResponseWriter, e *rpcError) {
	writeJSON(w, e.Status, map[string]any{"error": e.Msg})
}

func main() {
	addr := flag.String("addr", "127.0.0.1:8787", "listen address")
	flag.Parse()

	s := newServer()
	mux := http.NewServeMux()
	mux.Handle("/rpc/", s)
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	})

	log.Printf("storefront RPC server listening on http://%s", *addr)
	srv := &http.Server{
		Addr:              *addr,
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
	}
	log.Fatal(srv.ListenAndServe())
}
