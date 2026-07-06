// Command server is the WASIBrowser storefront RPC backend.
//
// It speaks the local-dev rpc-over-HTTP binding from docs/04-WEB-RPC.md:
//
//	POST /rpc/{iface}/{method}
//	  headers: GWB-Req-Id, GWB-App-Key, GWB-Sig, GWB-Ts, GWB-Session
//	  body:    request payload (JSON)
//	  reply:   JSON body + HTTP status (4xx/5xx map to err_class host-side)
//
// Dispatch is a method table (iface -> method -> route). Each route declares an
// authz role; the request first passes L1 channel authn (ed25519 signature),
// then the per-method guard, then the handler. The server's API *is* its method
// table — the RPC-first shape (§4).
package main

import (
	"crypto/ed25519"
	"encoding/json"
	"flag"
	"io"
	"log"
	"net/http"
	"strings"
	"time"
)

// rpcCtx carries one decoded call to a handler.
type rpcCtx struct {
	Iface   string
	Method  string
	ReqID   string
	AppKey  string
	Session string
	Body    []byte
	prin    principal // filled by dispatch after authn
	r       *http.Request
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

// rpcError maps to an HTTP status (and, host-side, an err_class). §1.
type rpcError struct {
	Status int
	Msg    string
}

func (e *rpcError) Error() string { return e.Msg }

func errUnauthorized(msg string) *rpcError { return &rpcError{http.StatusUnauthorized, msg} }
func errForbidden(msg string) *rpcError    { return &rpcError{http.StatusForbidden, msg} }
func errNotFound(msg string) *rpcError     { return &rpcError{http.StatusNotFound, msg} }
func errBadRequest(msg string) *rpcError   { return &rpcError{http.StatusBadRequest, msg} }

type handler func(c *rpcCtx) (any, *rpcError)

// route is a method plus its required authz role.
type route struct {
	role string // "public" | "user" | "admin"
	fn   handler
}

type server struct {
	methods map[string]map[string]route
	store   *store
	auth    *authConfig
}

func newServer(requireChannel bool) *server {
	pub, priv, _ := ed25519.GenerateKey(nil) // server session-signing key
	s := &server{
		methods: map[string]map[string]route{},
		store:   newStore(),
		auth:    &authConfig{serverPriv: priv, serverPub: pub, requireChannel: requireChannel},
	}
	s.routes()
	return s
}

func (s *server) register(iface, method, role string, h handler) {
	if s.methods[iface] == nil {
		s.methods[iface] = map[string]route{}
	}
	s.methods[iface][method] = route{role: role, fn: h}
}

// routes is the full storefront method table with per-method authz (§6 of 04-WEB-RPC).
func (s *server) routes() {
	s.register("gwb.echo.v1", "echo", "public", s.echo)

	s.register("shop.auth.v1", "login", "public", s.login)
	s.register("shop.auth.v1", "logout", "public", s.logout)
	s.register("shop.auth.v1", "me", "user", s.me)

	s.register("shop.catalog.v1", "list", "public", s.catalogList)
	s.register("shop.catalog.v1", "get", "public", s.catalogGet)
	s.register("shop.catalog.v1", "categories", "public", s.catalogCategories)

	s.register("shop.cart.v1", "get", "user", s.cartGet)
	s.register("shop.cart.v1", "add", "user", s.cartAdd)
	s.register("shop.cart.v1", "setQty", "user", s.cartSetQty)
	s.register("shop.cart.v1", "remove", "user", s.cartRemove)

	s.register("shop.orders.v1", "place", "user", s.ordersPlace)
	s.register("shop.orders.v1", "mine", "user", s.ordersMine)

	s.register("shop.admin.v1", "upsertProduct", "admin", s.adminUpsert)
	s.register("shop.admin.v1", "deleteProduct", "admin", s.adminDelete)
	s.register("shop.admin.v1", "orders", "admin", s.adminOrders)
}

func (s *server) echo(c *rpcCtx) (any, *rpcError) {
	var payload any
	_ = json.Unmarshal(c.Body, &payload)
	return map[string]any{
		"iface": c.Iface, "method": c.Method, "reqId": c.ReqID,
		"appKey": c.AppKey, "gotBody": payload, "ts": nowMs(),
	}, nil
}

// ServeHTTP: parse -> L1 authn -> per-method authz -> handler.
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
	c := &rpcCtx{
		Iface: iface, Method: method,
		ReqID:   r.Header.Get("GWB-Req-Id"),
		AppKey:  r.Header.Get("GWB-App-Key"),
		Session: r.Header.Get("GWB-Session"),
		Body:    body, r: r,
	}

	methods, ok := s.methods[iface]
	if !ok {
		writeErr(w, errNotFound("unknown iface: "+iface))
		return
	}
	rt, ok := methods[method]
	if !ok {
		writeErr(w, errNotFound("unknown method: "+iface+"."+method))
		return
	}

	// L1 channel authn + L2 session -> principal.
	prin, aerr := s.auth.resolvePrincipal(c)
	if aerr != nil {
		logCall(iface, method, c.ReqID, aerr, start)
		writeErr(w, aerr)
		return
	}
	c.prin = prin
	// Per-method authz guard.
	if gerr := requireRole(rt.role, prin); gerr != nil {
		logCall(iface, method, c.ReqID, gerr, start)
		writeErr(w, gerr)
		return
	}

	reply, rerr := rt.fn(c)
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
	addr := flag.String("addr", "127.0.0.1:8787", "listen address")
	insecure := flag.Bool("dev-insecure", false, "skip L1 channel signature verification (curl testing)")
	flag.Parse()

	s := newServer(!*insecure)
	mux := http.NewServeMux()
	mux.Handle("/rpc/", s)
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	})

	log.Printf("storefront RPC server on http://%s (channel-auth required: %v)", *addr, !*insecure)
	srv := &http.Server{Addr: *addr, Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	log.Fatal(srv.ListenAndServe())
}
