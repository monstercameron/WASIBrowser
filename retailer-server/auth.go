package main

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"strconv"
	"time"
)

// auth is intentionally smaller than server/auth.go's: this demo has no
// login/session layer (a retail catalog + guest cart needs no user
// accounts), only the L1 channel authn every GWB RPC service shares —
// every request is ed25519-signed by the app-scoped key (docs/04-WEB-RPC.md
// §2). The verified pubkey itself doubles as the cart's identity (see
// store.go's cartFor) — no separate session concept is needed.

const sigMaxSkewMs = 60_000 // GWB-Ts replay window (±60s), matches server/auth.go

type authConfig struct {
	requireChannel bool // if false (dev/tests via curl), skip L1 sig verification
}

var nowMs = func() int64 { return time.Now().UnixMilli() }

// canonicalBytes is the exact string both host and server sign/verify —
// identical to server/auth.go and the Rust search-server, so any compliant
// GWB host/service pair interoperates.
func canonicalBytes(iface, method, reqID, ts string, body []byte) []byte {
	sum := sha256.Sum256(body)
	return []byte(iface + "\n" + method + "\n" + reqID + "\n" + ts + "\n" + hex.EncodeToString(sum[:]))
}

// verifyChannel checks the channel signature and returns the verified
// app-key (base64) as the caller's identity, or an rpcError (401).
func (a *authConfig) verifyChannel(c *rpcCtx) (string, *rpcError) {
	appKeyB64 := c.AppKey
	sigB64 := c.r.Header.Get("GWB-Sig")
	tsStr := c.r.Header.Get("GWB-Ts")
	if !a.requireChannel && appKeyB64 == "" {
		return "dev:unsigned", nil // dev/tests: unsigned allowed, shared guest identity
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
