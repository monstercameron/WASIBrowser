//! Host network policy for guest-initiated I/O (`fetch` and `rpc_call`).
//!
//! The guest never sees a socket; every request it asks for goes through here:
//!
//! * **Origin allowlist.** `fetch` may only reach origins the app manifest
//!   declares under `"fetch"` (same capability model as `services`/`links`).
//!   An undeclared origin is rejected before any DNS lookup happens.
//! * **Private-address guard.** A declared origin whose host is a *name* may
//!   only connect to public addresses. The check runs inside the connection's
//!   own resolver, so DNS rebinding can't swap in `127.0.0.1`/`169.254.169.254`
//!   between check and connect. Loopback/LAN targets need an explicit IP
//!   literal (or `localhost`) in the manifest. That's what local dev servers use.
//! * **No redirects.** A 3xx is returned to the guest as-is; following it would
//!   let a declared origin bounce the request to an undeclared one.
//! * **Bounded bodies and concurrency.** Responses are capped at
//!   [`MAX_BODY_BYTES`] (after decompression), and each guest may have at most
//!   [`MAX_INFLIGHT`] requests outstanding.

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use url::Url;

/// Largest response body handed to a guest (decompressed).
pub const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;
/// Largest request payload a guest may send via `rpc_call`.
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
/// Outstanding fetch + rpc requests allowed per guest.
pub const MAX_INFLIGHT: usize = 16;
const TIMEOUT: Duration = Duration::from_secs(15);

/// A scheme://host:port triple (the unit of network capability).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

impl Origin {
    /// Parse a manifest declaration such as `https://api.example.com` or
    /// `http://127.0.0.1:8787`. Paths, queries, fragments and credentials are
    /// rejected so a declaration can never be mistaken for something narrower.
    pub fn parse_declared(s: &str) -> Result<Origin> {
        let u = Url::parse(s).map_err(|e| anyhow!("bad origin '{s}': {e}"))?;
        if u.path() != "/" || u.query().is_some() || u.fragment().is_some() {
            bail!("origin '{s}' must not have a path, query or fragment");
        }
        Origin::of(&u).ok_or_else(|| anyhow!("origin '{s}' must be http(s) with a host and no credentials"))
    }

    /// The origin of a request URL, or None if the URL isn't an http(s) URL we
    /// are willing to issue at all.
    pub fn of(u: &Url) -> Option<Origin> {
        if !matches!(u.scheme(), "http" | "https") {
            return None;
        }
        if !u.username().is_empty() || u.password().is_some() {
            return None;
        }
        Some(Origin {
            scheme: u.scheme().to_string(),
            host: u.host_str()?.to_ascii_lowercase(),
            port: u.port_or_known_default()?,
        })
    }

    /// True when the declaration itself names a loopback/private target (an IP
    /// literal or `localhost`), which is explicit consent to reach it.
    pub fn explicitly_local(&self) -> bool {
        if self.host == "localhost" {
            return true;
        }
        let bare = self.host.trim_start_matches('[').trim_end_matches(']');
        bare.parse::<IpAddr>().is_ok()
    }
}

/// Is this address reachable on the public internet? Everything else
/// (loopback, RFC 1918, link-local incl. cloud metadata, CGNAT, ULA,
/// multicast, documentation and reserved ranges) is treated as private.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let s = v6.segments();
            // NAT64 (64:ff9b::/96) embeds a v4 address; judge that instead.
            if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
                let o = v6.octets();
                return is_public_v4(Ipv4Addr::new(o[12], o[13], o[14], o[15]));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || s[0..6] == [0, 0, 0, 0, 0, 0] // ::/96 (IPv4-compatible, deprecated)
                || (s[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (s[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
                || (s[0] == 0x2001 && s[1] == 0x0db8)) // documentation
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local() // 169.254/16, incl. 169.254.169.254 metadata
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24 IETF
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 benchmarking
        || o[0] >= 240) // 240/4 reserved
}

/// Decide whether a guest `fetch` of `url` is allowed under `allowed`.
/// Returns the parsed URL and whether private addresses may be dialed.
pub fn check_fetch(url: &str, allowed: &[Origin]) -> Result<(Url, bool), String> {
    let u = Url::parse(url).map_err(|e| format!("bad url: {e}"))?;
    let origin = Origin::of(&u).ok_or("only http(s) URLs without credentials are allowed")?;
    let Some(declared) = allowed.iter().find(|o| **o == origin) else {
        return Err(format!(
            "origin {}://{}:{} is not declared in the app manifest's \"fetch\" list",
            origin.scheme, origin.host, origin.port
        ));
    };
    Ok((u, declared.explicitly_local()))
}

/// RPC endpoints come from the manifest, so the declaration is the endpoint
/// itself; the same private-address rule applies to it.
pub fn endpoint_allows_private(endpoint: &str) -> bool {
    Url::parse(endpoint)
        .ok()
        .as_ref()
        .and_then(Origin::of)
        .is_some_and(|o| o.explicitly_local())
}

fn tls() -> Option<Arc<native_tls::TlsConnector>> {
    static TLS: OnceLock<Option<Arc<native_tls::TlsConnector>>> = OnceLock::new();
    TLS.get_or_init(|| match native_tls::TlsConnector::new() {
        Ok(c) => Some(Arc::new(c)),
        Err(e) => {
            crate::logger::log("net", &format!("TLS init failed, https disabled: {e}"));
            None
        }
    })
    .clone()
}

/// A ureq agent enforcing the policy above. `allow_private` is only true when
/// the manifest named a loopback/private target explicitly.
pub fn agent(allow_private: bool) -> ureq::Agent {
    let resolver = move |netloc: &str| -> std::io::Result<Vec<SocketAddr>> {
        let addrs: Vec<SocketAddr> = netloc.to_socket_addrs()?.collect();
        let kept: Vec<SocketAddr> = if allow_private {
            addrs
        } else {
            addrs.into_iter().filter(|a| is_public(a.ip())).collect()
        };
        if kept.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{netloc}: resolves only to non-public addresses (blocked by network policy)"),
            ));
        }
        Ok(kept)
    };
    let mut b = ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .redirects(0)
        .resolver(resolver);
    if let Some(t) = tls() {
        b = b.tls_connector(t);
    }
    b.build()
}

/// Read a response body, refusing anything over [`MAX_BODY_BYTES`].
pub fn read_body(resp: ureq::Response) -> Result<String, String> {
    if let Some(len) = resp.header("Content-Length").and_then(|v| v.trim().parse::<u64>().ok()) {
        if len > MAX_BODY_BYTES {
            return Err(format!("response body is {len} bytes (limit {MAX_BODY_BYTES})"));
        }
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("reading body: {e}"))?;
    if buf.len() as u64 > MAX_BODY_BYTES {
        return Err(format!("response body exceeds {MAX_BODY_BYTES} bytes"));
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Per-guest outstanding-request counter.
#[derive(Clone, Default)]
pub struct Inflight(Arc<AtomicUsize>);

/// Held by a request thread; releases its slot on drop (including on panic).
pub struct InflightGuard(Arc<AtomicUsize>);

impl Inflight {
    pub fn try_acquire(&self) -> Option<InflightGuard> {
        let mut cur = self.0.load(Ordering::Acquire);
        loop {
            if cur >= MAX_INFLIGHT {
                return None;
            }
            match self.0.compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(InflightGuard(self.0.clone())),
                Err(actual) => cur = actual,
            }
        }
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(s: &str) -> Origin {
        Origin::parse_declared(s).unwrap()
    }

    #[test]
    fn declared_origins_are_strict() {
        assert_eq!(o("https://Example.com").host, "example.com");
        assert_eq!(o("https://example.com").port, 443);
        assert_eq!(o("http://127.0.0.1:8787").port, 8787);
        assert!(Origin::parse_declared("https://example.com/api").is_err());
        assert!(Origin::parse_declared("https://example.com/?q=1").is_err());
        assert!(Origin::parse_declared("https://u:p@example.com").is_err());
        assert!(Origin::parse_declared("file:///etc/passwd").is_err());
        assert!(Origin::parse_declared("ftp://example.com").is_err());
    }

    #[test]
    fn fetch_requires_declared_origin() {
        let allowed = vec![o("https://api.example.com")];
        assert!(check_fetch("https://api.example.com/todos?x=1", &allowed).is_ok());
        assert!(check_fetch("https://api.example.com:443/x", &allowed).is_ok());
        assert!(check_fetch("http://api.example.com/x", &allowed).is_err()); // scheme
        assert!(check_fetch("https://api.example.com:8443/x", &allowed).is_err()); // port
        assert!(check_fetch("https://evil.example.com/x", &allowed).is_err());
        assert!(check_fetch("https://user@api.example.com/x", &allowed).is_err());
        assert!(check_fetch("http://169.254.169.254/latest", &allowed).is_err());
        assert!(check_fetch("file:///c:/secret", &allowed).is_err());
        assert!(check_fetch("not a url", &allowed).is_err());
        assert!(check_fetch("https://api.example.com/", &[]).is_err());
    }

    #[test]
    fn private_dialing_only_for_explicit_local_declarations() {
        let (_, p) = check_fetch("https://api.example.com/", &[o("https://api.example.com")]).unwrap();
        assert!(!p);
        let (_, p) = check_fetch("http://127.0.0.1:9000/", &[o("http://127.0.0.1:9000")]).unwrap();
        assert!(p);
        let (_, p) = check_fetch("http://localhost:9000/", &[o("http://localhost:9000")]).unwrap();
        assert!(p);
        let (_, p) = check_fetch("http://[::1]:9000/", &[o("http://[::1]:9000")]).unwrap();
        assert!(p);
        assert!(endpoint_allows_private("http://127.0.0.1:8787"));
        assert!(!endpoint_allows_private("https://rpc.example.com"));
    }

    #[test]
    fn address_classification() {
        for s in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1",
            "0.0.0.0", "255.255.255.255", "224.0.0.1", "198.18.0.1", "240.0.0.1", "192.0.0.8",
            "::1", "::", "fc00::1", "fd12::1", "fe80::1", "ff02::1", "2001:db8::1",
            "::ffff:127.0.0.1", "::ffff:10.0.0.1", "64:ff9b::a9fe:a9fe", "::7f00:1",
        ] {
            assert!(!is_public(s.parse().unwrap()), "{s} should be private");
        }
        for s in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(is_public(s.parse().unwrap()), "{s} should be public");
        }
    }

    #[test]
    fn resolver_blocks_loopback_names_unless_allowed() {
        let strict = agent(false);
        let err = strict.get("http://localhost:1/").call().unwrap_err().to_string();
        assert!(err.contains("non-public") || err.contains("blocked"), "{err}");
    }

    #[test]
    fn inflight_is_bounded_and_released() {
        let f = Inflight::default();
        let guards: Vec<_> = (0..MAX_INFLIGHT).map(|_| f.try_acquire().unwrap()).collect();
        assert!(f.try_acquire().is_none());
        drop(guards);
        assert!(f.try_acquire().is_some());
    }
}
