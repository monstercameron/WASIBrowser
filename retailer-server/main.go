// Command retailer-server is the RPC backend for the retailer-go demo — an
// electronics storefront proving a real (non-toy), RPC-backed wasm guest
// authored entirely in Go (the sdk/gwb analogue of examples/shop-c),
// exercising the RPC/fetch bindings added to sdk/gwb for this demo.
//
// Same local-dev rpc-over-HTTP binding as server/ and search-server (docs/
// 04-WEB-RPC.md): POST /rpc/{iface}/{method}, GWB-Req-Id/App-Key/Sig/Ts
// headers, JSON body/reply. Unlike shop, there is no login/session layer —
// every method is public, and the cart is keyed by the verified channel app
// key itself (see auth.go), proving the channel-authn contract without
// re-deriving shop's user-account machinery a second time.
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

type rpcCtx struct {
	Iface  string
	Method string
	ReqID  string
	AppKey string
	Body   []byte
	prin   string // verified app-key identity, filled by dispatch
	r      *http.Request
}

func (c *rpcCtx) bind(v any) *rpcError {
	if len(c.Body) == 0 {
		return nil
	}
	if err := json.Unmarshal(c.Body, v); err != nil {
		return errBadRequest("bad json: " + err.Error())
	}
	return nil
}

type rpcError struct {
	Status int
	Msg    string
}

func (e *rpcError) Error() string { return e.Msg }

func errUnauthorized(msg string) *rpcError { return &rpcError{http.StatusUnauthorized, msg} }
func errNotFound(msg string) *rpcError     { return &rpcError{http.StatusNotFound, msg} }
func errBadRequest(msg string) *rpcError   { return &rpcError{http.StatusBadRequest, msg} }

type handler func(c *rpcCtx) (any, *rpcError)

type server struct {
	methods map[string]map[string]handler
	store   *store
	auth    *authConfig
}

func newServer(requireChannel bool) *server {
	s := &server{
		methods: map[string]map[string]handler{},
		store:   newStore(),
		auth:    &authConfig{requireChannel: requireChannel},
	}
	s.routes()
	return s
}

func (s *server) register(iface, method string, h handler) {
	if s.methods[iface] == nil {
		s.methods[iface] = map[string]handler{}
	}
	s.methods[iface][method] = h
}

// routes is the full retailer method table. Every method is public — the
// channel signature is still verified (see ServeHTTP), there's just no user
// role layered on top of it.
func (s *server) routes() {
	s.register("retailer.catalog.v1", "list", s.catalogList)
	s.register("retailer.catalog.v1", "get", s.catalogGet)
	s.register("retailer.catalog.v1", "categories", s.catalogCategories)

	s.register("retailer.cart.v1", "get", s.cartGet)
	s.register("retailer.cart.v1", "add", s.cartAdd)
	s.register("retailer.cart.v1", "setQty", s.cartSetQty)
	s.register("retailer.cart.v1", "remove", s.cartRemove)
}

func (s *server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	start := time.Now()
	if r.Method != http.MethodPost {
		writeErr(w, &rpcError{http.StatusMethodNotAllowed, "rpc is POST-only"})
		return
	}
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
	body, _ := io.ReadAll(io.LimitReader(r.Body, 1<<20))
	c := &rpcCtx{Iface: iface, Method: method, ReqID: r.Header.Get("GWB-Req-Id"), AppKey: r.Header.Get("GWB-App-Key"), Body: body, r: r}

	methods, ok := s.methods[iface]
	if !ok {
		writeErr(w, errNotFound("unknown iface: "+iface))
		return
	}
	fn, ok := methods[method]
	if !ok {
		writeErr(w, errNotFound("unknown method: "+iface+"."+method))
		return
	}

	prin, aerr := s.auth.verifyChannel(c)
	if aerr != nil {
		logCall(iface, method, c.ReqID, aerr, start)
		writeErr(w, aerr)
		return
	}
	c.prin = prin

	reply, rerr := fn(c)
	logCall(iface, method, c.ReqID, rerr, start)
	if rerr != nil {
		writeErr(w, rerr)
		return
	}
	writeJSON(w, http.StatusOK, reply)
}

func logCall(iface, method, reqID string, e *rpcError, start time.Time) {
	if e != nil {
		log.Printf("rpc %s.%s req=%s -> ERR %d %s (%s)", iface, method, reqID, e.Status, e.Msg, time.Since(start))
		return
	}
	log.Printf("rpc %s.%s req=%s -> OK (%s)", iface, method, reqID, time.Since(start))
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
	addr := flag.String("addr", "127.0.0.1:8789", "listen address")
	insecure := flag.Bool("dev-insecure", false, "skip L1 channel signature verification (curl testing)")
	flag.Parse()

	s := newServer(!*insecure)
	mux := http.NewServeMux()
	mux.Handle("/rpc/", s)
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	})

	log.Printf("retailer RPC server on http://%s (channel-auth required: %v)", *addr, !*insecure)
	srv := &http.Server{Addr: *addr, Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	log.Fatal(srv.ListenAndServe())
}
