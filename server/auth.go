package main

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"strconv"
	"strings"
	"time"
)

// auth implements the two-layer model of docs/04-WEB-RPC.md §3:
//   L1 channel authn — every request is ed25519-signed by the app-scoped key.
//   L2 user authn    — auth.login issues a server-signed capability token.
// Authz is one per-method guard against the resulting principal.

const (
	sigMaxSkewMs = 60_000 // GWB-Ts replay window (±60s)
	sessionTTLms = 24 * 60 * 60 * 1000
)

// authConfig holds the server signing key and a dev toggle.
type authConfig struct {
	serverPriv     ed25519.PrivateKey
	serverPub      ed25519.PublicKey
	requireChannel bool // if false (dev/tests via curl), skip L1 sig verification
}

// principal is who the guard decides against.
type principal struct {
	AppKey string // verified app-scoped pubkey (L1), base64
	Sub    string // user email (L2), empty if not logged in
	Role   string // "user" | "admin", empty if not logged in
}

func (p principal) authed() bool { return p.Sub != "" }
func (p principal) isAdmin() bool { return p.Role == "admin" }

// nowMs is overridable in tests.
var nowMs = func() int64 { return time.Now().UnixMilli() }

// canonicalBytes is the exact string both host and server sign/verify (§2).
func canonicalBytes(iface, method, reqID, ts string, body []byte) []byte {
	sum := sha256.Sum256(body)
	return []byte(iface + "\n" + method + "\n" + reqID + "\n" + ts + "\n" + hex.EncodeToString(sum[:]))
}

// verifyChannel checks L1: the request carries a valid, fresh ed25519 signature
// over the canonical bytes, made by GWB-App-Key. Returns the app principal or an
// rpcError (401 -> err_class authn).
func (a *authConfig) verifyChannel(c *rpcCtx) (string, *rpcError) {
	appKeyB64 := c.AppKey
	sigB64 := c.r.Header.Get("GWB-Sig")
	tsStr := c.r.Header.Get("GWB-Ts")
	if !a.requireChannel && appKeyB64 == "" {
		return "", nil // dev/tests: unsigned allowed
	}
	if appKeyB64 == "" || sigB64 == "" || tsStr == "" {
		return "", errUnauthorized("missing channel auth (GWB-App-Key/Sig/Ts)")
	}
	ts, err := strconv.ParseInt(tsStr, 10, 64)
	if err != nil {
		return "", errUnauthorized("bad GWB-Ts")
	}
	if d := nowMs() - ts; d > sigMaxSkewMs || d < -sigMaxSkewMs {
		return "", errUnauthorized("stale GWB-Ts (replay window)")
	}
	pub, err := base64.StdEncoding.DecodeString(appKeyB64)
	if err != nil || len(pub) != ed25519.PublicKeySize {
		return "", errUnauthorized("bad GWB-App-Key")
	}
	sig, err := base64.StdEncoding.DecodeString(sigB64)
	if err != nil || len(sig) != ed25519.SignatureSize {
		return "", errUnauthorized("bad GWB-Sig")
	}
	msg := canonicalBytes(c.Iface, c.Method, c.ReqID, tsStr, c.Body)
	if !ed25519.Verify(pub, msg, sig) {
		return "", errUnauthorized("channel signature does not verify")
	}
	return appKeyB64, nil
}

// --- L2 session tokens (server-signed capability tokens) ---

type sessionClaims struct {
	Sub  string `json:"sub"`
	Role string `json:"role"`
	Iat  int64  `json:"iat"`
	Exp  int64  `json:"exp"`
}

func (a *authConfig) issueSession(u *User) string {
	claims := sessionClaims{Sub: u.Sub, Role: u.Role, Iat: nowMs(), Exp: nowMs() + sessionTTLms}
	body, _ := json.Marshal(claims)
	sig := ed25519.Sign(a.serverPriv, body)
	return base64.RawURLEncoding.EncodeToString(body) + "." + base64.RawURLEncoding.EncodeToString(sig)
}

// verifySession parses + verifies a session token, returning its claims or nil.
func (a *authConfig) verifySession(token string) *sessionClaims {
	if token == "" {
		return nil
	}
	bodyB64, sigB64, ok := strings.Cut(token, ".")
	if !ok {
		return nil
	}
	body, err := base64.RawURLEncoding.DecodeString(bodyB64)
	if err != nil {
		return nil
	}
	sig, err := base64.RawURLEncoding.DecodeString(sigB64)
	if err != nil || !ed25519.Verify(a.serverPub, body, sig) {
		return nil
	}
	var claims sessionClaims
	if json.Unmarshal(body, &claims) != nil {
		return nil
	}
	if nowMs() > claims.Exp {
		return nil
	}
	return &claims
}

// resolvePrincipal runs both layers and produces the principal for the guard.
func (a *authConfig) resolvePrincipal(c *rpcCtx) (principal, *rpcError) {
	appKey, aerr := a.verifyChannel(c)
	if aerr != nil {
		return principal{}, aerr
	}
	p := principal{AppKey: appKey}
	if claims := a.verifySession(c.Session); claims != nil {
		p.Sub, p.Role = claims.Sub, claims.Role
	}
	return p, nil
}

// requireRole is the per-method authz guard (§3). role: "public"|"user"|"admin".
func requireRole(role string, p principal) *rpcError {
	switch role {
	case "public", "":
		return nil
	case "user":
		if !p.authed() {
			return errUnauthorized("login required")
		}
	case "admin":
		if !p.authed() {
			return errUnauthorized("login required")
		}
		if !p.isAdmin() {
			return errForbidden("admin role required")
		}
	}
	return nil
}
