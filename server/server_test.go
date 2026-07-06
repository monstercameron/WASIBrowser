package main

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
)

// client signs requests like the host does (docs/04-WEB-RPC.md §2).
type client struct {
	pub  ed25519.PublicKey
	priv ed25519.PrivateKey
}

func newClient() *client {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return &client{pub, priv}
}

// call issues a signed RPC against the server; returns status + decoded body.
func (c *client) call(t *testing.T, s *server, iface, method, session string, payload any, sign bool) (int, map[string]any) {
	t.Helper()
	body, _ := json.Marshal(payload)
	reqID := "1"
	ts := strconv.FormatInt(nowMs(), 10)
	r := httptest.NewRequest("POST", "/rpc/"+iface+"/"+method, strings.NewReader(string(body)))
	r.Header.Set("GWB-Req-Id", reqID)
	if sign {
		sum := sha256.Sum256(body)
		msg := []byte(iface + "\n" + method + "\n" + reqID + "\n" + ts + "\n" + hex.EncodeToString(sum[:]))
		sig := ed25519.Sign(c.priv, msg)
		r.Header.Set("GWB-App-Key", base64.StdEncoding.EncodeToString(c.pub))
		r.Header.Set("GWB-Sig", base64.StdEncoding.EncodeToString(sig))
		r.Header.Set("GWB-Ts", ts)
	}
	if session != "" {
		r.Header.Set("GWB-Session", session)
	}
	w := httptest.NewRecorder()
	s.ServeHTTP(w, r)
	var out map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &out)
	return w.Code, out
}

func (c *client) login(t *testing.T, s *server, email, pw string) string {
	t.Helper()
	code, out := c.call(t, s, "shop.auth.v1", "login", "", map[string]string{"Email": email, "Password": pw}, true)
	if code != 200 {
		t.Fatalf("login %s: want 200, got %d (%v)", email, code, out)
	}
	tok, _ := out["token"].(string)
	if tok == "" {
		t.Fatalf("login %s: empty token", email)
	}
	return tok
}

func TestChannelAuthn(t *testing.T) {
	s := newServer(true) // channel required
	c := newClient()

	// signed public call succeeds
	if code, _ := c.call(t, s, "shop.catalog.v1", "list", "", map[string]string{}, true); code != 200 {
		t.Fatalf("signed catalog.list: want 200, got %d", code)
	}
	// unsigned rejected (401 authn) when channel required
	if code, _ := c.call(t, s, "shop.catalog.v1", "list", "", map[string]string{}, false); code != 401 {
		t.Fatalf("unsigned catalog.list: want 401, got %d", code)
	}
}

func TestReplayWindow(t *testing.T) {
	s := newServer(true)
	c := newClient()
	// craft a request with a stale timestamp
	body, _ := json.Marshal(map[string]string{})
	staleTs := strconv.FormatInt(nowMs()-120_000, 10) // 2 min old
	sum := sha256.Sum256(body)
	msg := []byte("shop.catalog.v1\nlist\n1\n" + staleTs + "\n" + hex.EncodeToString(sum[:]))
	sig := ed25519.Sign(c.priv, msg)
	r := httptest.NewRequest("POST", "/rpc/shop.catalog.v1/list", strings.NewReader(string(body)))
	r.Header.Set("GWB-Req-Id", "1")
	r.Header.Set("GWB-App-Key", base64.StdEncoding.EncodeToString(c.pub))
	r.Header.Set("GWB-Sig", base64.StdEncoding.EncodeToString(sig))
	r.Header.Set("GWB-Ts", staleTs)
	w := httptest.NewRecorder()
	s.ServeHTTP(w, r)
	if w.Code != 401 {
		t.Fatalf("stale ts: want 401, got %d", w.Code)
	}
}

func TestTamperedBodyRejected(t *testing.T) {
	s := newServer(true)
	c := newClient()
	// sign one body, send another
	ts := strconv.FormatInt(nowMs(), 10)
	sum := sha256.Sum256([]byte(`{"Category":"tops"}`))
	msg := []byte("shop.catalog.v1\nlist\n1\n" + ts + "\n" + hex.EncodeToString(sum[:]))
	sig := ed25519.Sign(c.priv, msg)
	r := httptest.NewRequest("POST", "/rpc/shop.catalog.v1/list", strings.NewReader(`{"Category":"bottoms"}`))
	r.Header.Set("GWB-Req-Id", "1")
	r.Header.Set("GWB-App-Key", base64.StdEncoding.EncodeToString(c.pub))
	r.Header.Set("GWB-Sig", base64.StdEncoding.EncodeToString(sig))
	r.Header.Set("GWB-Ts", ts)
	w := httptest.NewRecorder()
	s.ServeHTTP(w, r)
	if w.Code != 401 {
		t.Fatalf("tampered body: want 401, got %d", w.Code)
	}
}

func TestAuthzTiers(t *testing.T) {
	s := newServer(true)
	c := newClient()

	// public: catalog.list works with no session
	if code, out := c.call(t, s, "shop.catalog.v1", "list", "", map[string]string{}, true); code != 200 {
		t.Fatalf("catalog.list public: want 200, got %d (%v)", code, out)
	}
	// user-tier without session -> 401
	if code, _ := c.call(t, s, "shop.cart.v1", "get", "", map[string]string{}, true); code != 401 {
		t.Fatalf("cart.get no session: want 401, got %d", code)
	}
	// admin-tier with USER session -> 403
	userTok := c.login(t, s, "shopper@aurelia.dev", "shop1234")
	if code, _ := c.call(t, s, "shop.admin.v1", "orders", userTok, map[string]string{}, true); code != 403 {
		t.Fatalf("admin.orders as user: want 403, got %d", code)
	}
	// admin-tier with ADMIN session -> 200
	adminTok := c.login(t, s, "admin@aurelia.dev", "admin1234")
	if code, _ := c.call(t, s, "shop.admin.v1", "orders", adminTok, map[string]string{}, true); code != 200 {
		t.Fatalf("admin.orders as admin: want 200, got %d", code)
	}
}

func TestLoginFailure(t *testing.T) {
	s := newServer(true)
	c := newClient()
	if code, _ := c.call(t, s, "shop.auth.v1", "login", "", map[string]string{"Email": "shopper@aurelia.dev", "Password": "wrong"}, true); code != 401 {
		t.Fatalf("bad password: want 401, got %d", code)
	}
}

func TestCartAndCheckoutFlow(t *testing.T) {
	s := newServer(true)
	c := newClient()
	tok := c.login(t, s, "shopper@aurelia.dev", "shop1234")

	// add two products
	c.call(t, s, "shop.cart.v1", "add", tok, map[string]any{"ID": "linen-shirt", "Qty": 2}, true)
	code, cart := c.call(t, s, "shop.cart.v1", "add", tok, map[string]any{"ID": "leather-belt", "Qty": 1}, true)
	if code != 200 {
		t.Fatalf("cart.add: %d", code)
	}
	if cnt, _ := cart["count"].(float64); cnt != 2 {
		t.Fatalf("cart count: want 2, got %v", cart["count"])
	}
	// subtotal = 2*8900 + 5900 = 23700
	if sub, _ := cart["subtotal"].(float64); sub != 23700 {
		t.Fatalf("subtotal: want 23700, got %v", cart["subtotal"])
	}
	// checkout
	code, order := c.call(t, s, "shop.orders.v1", "place", tok, map[string]any{
		"Shipping": map[string]string{"Name": "Ava", "Address": "1 Main", "City": "Miami", "Zip": "33101"},
	}, true)
	if code != 200 {
		t.Fatalf("orders.place: %d (%v)", code, order)
	}
	if id, _ := order["id"].(string); !strings.HasPrefix(id, "AUR-") {
		t.Fatalf("order id: %v", order["id"])
	}
	// cart cleared after checkout
	_, cart2 := c.call(t, s, "shop.cart.v1", "get", tok, nil, true)
	if cnt, _ := cart2["count"].(float64); cnt != 0 {
		t.Fatalf("cart after checkout: want 0, got %v", cart2["count"])
	}
}

func TestUndeclaredMethod(t *testing.T) {
	s := newServer(true)
	c := newClient()
	if code, _ := c.call(t, s, "shop.catalog.v1", "nope", "", map[string]string{}, true); code != 404 {
		t.Fatalf("unknown method: want 404, got %d", code)
	}
}

// sanity: catalog seeded and categories present
func TestCatalogSeed(t *testing.T) {
	s := newServer(false) // insecure ok for read
	c := newClient()
	_, out := c.call(t, s, "shop.catalog.v1", "list", "", map[string]string{}, false)
	prods, _ := out["products"].([]any)
	if len(prods) < 10 {
		t.Fatalf("catalog: want >=10 products, got %d", len(prods))
	}
}
