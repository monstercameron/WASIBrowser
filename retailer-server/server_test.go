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

type client struct {
	pub  ed25519.PublicKey
	priv ed25519.PrivateKey
}

func newClient() *client {
	pub, priv, _ := ed25519.GenerateKey(nil)
	return &client{pub, priv}
}

func (c *client) call(t *testing.T, s *server, iface, method string, payload any, sign bool) (int, map[string]any) {
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
	w := httptest.NewRecorder()
	s.ServeHTTP(w, r)
	var out map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &out)
	return w.Code, out
}

func TestChannelAuthn(t *testing.T) {
	s := newServer(true)
	c := newClient()
	if code, _ := c.call(t, s, "retailer.catalog.v1", "list", map[string]string{}, true); code != 200 {
		t.Fatalf("signed catalog.list: want 200, got %d", code)
	}
	if code, _ := c.call(t, s, "retailer.catalog.v1", "list", map[string]string{}, false); code != 401 {
		t.Fatalf("unsigned catalog.list: want 401, got %d", code)
	}
}

func TestReplayWindow(t *testing.T) {
	s := newServer(true)
	c := newClient()
	body, _ := json.Marshal(map[string]string{})
	staleTs := strconv.FormatInt(nowMs()-120_000, 10)
	sum := sha256.Sum256(body)
	msg := []byte("retailer.catalog.v1\nlist\n1\n" + staleTs + "\n" + hex.EncodeToString(sum[:]))
	sig := ed25519.Sign(c.priv, msg)
	r := httptest.NewRequest("POST", "/rpc/retailer.catalog.v1/list", strings.NewReader(string(body)))
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
	ts := strconv.FormatInt(nowMs(), 10)
	sum := sha256.Sum256([]byte(`{"Category":"laptops"}`))
	msg := []byte("retailer.catalog.v1\nlist\n1\n" + ts + "\n" + hex.EncodeToString(sum[:]))
	sig := ed25519.Sign(c.priv, msg)
	r := httptest.NewRequest("POST", "/rpc/retailer.catalog.v1/list", strings.NewReader(`{"Category":"phones"}`))
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

func TestCartFlow(t *testing.T) {
	s := newServer(true)
	c := newClient()
	c.call(t, s, "retailer.cart.v1", "add", map[string]any{"ID": "flagship-phone", "Qty": 1}, true)
	code, cart := c.call(t, s, "retailer.cart.v1", "add", map[string]any{"ID": "wireless-earbuds", "Qty": 2}, true)
	if code != 200 {
		t.Fatalf("cart.add: %d", code)
	}
	if cnt, _ := cart["count"].(float64); cnt != 2 {
		t.Fatalf("cart count: want 2, got %v", cart["count"])
	}
	// subtotal = 99900 + 2*17900 = 135700
	if sub, _ := cart["subtotal"].(float64); sub != 135700 {
		t.Fatalf("subtotal: want 135700, got %v", cart["subtotal"])
	}
	_, cart2 := c.call(t, s, "retailer.cart.v1", "remove", map[string]any{"ID": "flagship-phone"}, true)
	if cnt, _ := cart2["count"].(float64); cnt != 1 {
		t.Fatalf("cart after remove: want 1, got %v", cart2["count"])
	}
}

func TestCartsAreIsolatedPerAppKey(t *testing.T) {
	s := newServer(true)
	alice, bob := newClient(), newClient()
	alice.call(t, s, "retailer.cart.v1", "add", map[string]any{"ID": "flagship-phone", "Qty": 1}, true)
	_, bobCart := bob.call(t, s, "retailer.cart.v1", "get", map[string]any{}, true)
	if cnt, _ := bobCart["count"].(float64); cnt != 0 {
		t.Fatalf("bob's cart should be empty (isolated from alice), got count=%v", bobCart["count"])
	}
}

func TestUndeclaredMethod(t *testing.T) {
	s := newServer(true)
	c := newClient()
	if code, _ := c.call(t, s, "retailer.catalog.v1", "nope", map[string]string{}, true); code != 404 {
		t.Fatalf("unknown method: want 404, got %d", code)
	}
}

func TestCatalogSeed(t *testing.T) {
	s := newServer(false)
	c := newClient()
	_, out := c.call(t, s, "retailer.catalog.v1", "list", map[string]string{}, false)
	prods, _ := out["products"].([]any)
	if len(prods) < 10 {
		t.Fatalf("catalog: want >=10 products, got %d", len(prods))
	}
}
