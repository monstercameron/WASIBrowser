//! App manifest resolution + `web://` navigation (docs/04-WEB-RPC.md §4).
//!
//! `renderer web://shop.local` resolves `{manifest_root}/shop.local.json` into a
//! bundle path + the app's capabilities: the RPC services it may call, the
//! `web://` names it may link to, and the origins it may `fetch`. The browser
//! navigates by name, and the app can only reach what its manifest declares.
//! A bare `.wasm` path still works (dev back-compat): it yields a default
//! manifest with a dev `echo` service and the dev fetch origins from
//! `GWB_DEV_FETCH_ORIGINS`.
//!
//! Manifest shape:
//! ```json
//! { "title": "...", "bundle": "shop.wasm",
//!   "services": { "auth": { "iface": "shop.auth.v1", "endpoint": "http://127.0.0.1:8787", "key": "ed:..." } },
//!   "links": ["search.local"],
//!   "fetch": ["https://api.example.com"] }
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use anyhow::{Context, Result, anyhow, bail};

use crate::abi::ServiceEntry;
use crate::net::Origin;

pub struct ResolvedApp {
    pub bundle_path: String,
    pub title: String,
    pub services: HashMap<String, ServiceEntry>,
    /// Manifest-declared cross-site link targets (bare `web://` names this
    /// app may navigate to/open a new tab for). Mirrors `services`' capability
    /// model; the `navigate` host import rejects any target not in this set.
    pub links: HashSet<String>,
    /// Origins the app's `fetch` calls may reach (see net.rs).
    pub fetch: Vec<Origin>,
    /// True when loaded from a filesystem bundle without b3: verification — we
    /// log this so the trust downgrade is never silent (Constitution rule 1).
    pub dev_unverified: bool,
}

/// Resolve a navigation target (a `web://name` address or a `.wasm` path).
pub fn resolve(target: &str, manifest_root: &str) -> Result<ResolvedApp> {
    if let Some(name) = target.strip_prefix("web://") {
        resolve_manifest(name, manifest_root)
    } else {
        // Bare wasm path (typed by the user or passed on the command line):
        // synthesize a minimal manifest with the dev echo service so smoke
        // tests / ad-hoc guests still work.
        let mut services = HashMap::new();
        services.insert(
            "echo".to_string(),
            ServiceEntry {
                endpoint: dev_endpoint(),
                iface: "gwb.echo.v1".to_string(),
                server_key: String::new(),
            },
        );
        Ok(ResolvedApp {
            bundle_path: target.to_string(),
            title: "WASIBrowser".to_string(),
            services,
            links: HashSet::new(),
            fetch: dev_fetch_origins(),
            dev_unverified: true,
        })
    }
}

fn dev_endpoint() -> String {
    std::env::var("GWB_RPC_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8787".to_string())
}

/// Bare-`.wasm` dev mode has no manifest, so its fetch allowlist comes from
/// `GWB_DEV_FETCH_ORIGINS` (comma-separated origins). Unset = no fetch.
fn dev_fetch_origins() -> Vec<Origin> {
    let Ok(list) = std::env::var("GWB_DEV_FETCH_ORIGINS") else {
        return Vec::new();
    };
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| match Origin::parse_declared(s) {
            Ok(o) => Some(o),
            Err(e) => {
                crate::logger::log("sys", &format!("GWB_DEV_FETCH_ORIGINS: ignoring {e}"));
                None
            }
        })
        .collect()
}

/// `web://` names are DNS-like: lowercase letters, digits, `-` and `.`, with
/// no empty labels. That also guarantees `{root}/{name}.json` stays inside
/// the manifest root (no `..`, separators or drive prefixes).
pub fn valid_app_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn resolve_manifest(name: &str, manifest_root: &str) -> Result<ResolvedApp> {
    let name = name.trim_end_matches('/');
    if !valid_app_name(name) {
        bail!("invalid app name 'web://{name}' (expected lowercase letters, digits, '-' and '.')");
    }
    let path = format!("{}/{}.json", manifest_root.trim_end_matches(['/', '\\']), name);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading manifest for web://{name} at {path}"))?;
    let m: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing manifest {path}"))?;

    let bundle = m
        .get("bundle")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("manifest {path}: missing 'bundle'"))?;
    let bundle_path = resolve_bundle_path(bundle).with_context(|| format!("manifest {path}"))?;
    let title = m
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(name)
        .to_string();

    let mut services = HashMap::new();
    if let Some(obj) = m.get("services").and_then(|v| v.as_object()) {
        for (svc_name, spec) in obj {
            let endpoint = spec
                .get("endpoint")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("service '{svc_name}': missing 'endpoint'"))?
                .to_string();
            Origin::parse_declared(endpoint.trim_end_matches('/'))
                .with_context(|| format!("service '{svc_name}': bad endpoint"))?;
            let iface = spec
                .get("iface")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("service '{svc_name}': missing 'iface'"))?
                .to_string();
            if iface.is_empty()
                || !iface.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
                || iface.contains("..")
            {
                bail!("service '{svc_name}': invalid iface '{iface}'");
            }
            let server_key = spec
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            services.insert(svc_name.clone(), ServiceEntry { endpoint, iface, server_key });
        }
    }

    // Cross-site link capability (bare `web://` names, e.g. "retailer.local")
    // this app may navigate to/open a new tab for — mirrors `services` above.
    let mut links = HashSet::new();
    if let Some(arr) = m.get("links").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(name) = v.as_str() {
                if !valid_app_name(name) {
                    bail!("manifest {path}: invalid link '{name}'");
                }
                links.insert(name.to_string());
            }
        }
    }

    let mut fetch = Vec::new();
    if let Some(arr) = m.get("fetch").and_then(|v| v.as_array()) {
        for v in arr {
            let s = v.as_str().ok_or_else(|| anyhow!("manifest {path}: 'fetch' entries must be strings"))?;
            fetch.push(Origin::parse_declared(s).with_context(|| format!("manifest {path}"))?);
        }
    }

    Ok(ResolvedApp {
        bundle_path,
        title,
        services,
        links,
        fetch,
        dev_unverified: true,
    })
}

/// A manifest's `bundle` is a relative `.wasm` path, resolved against the
/// renderer's working directory (where bundles are built). Absolute paths,
/// drive prefixes and `..` are refused so a manifest can't point the browser
/// at an arbitrary file on disk. `b3:` content-addressed bundles are the
/// production form but aren't wired yet, so they fail loudly instead of being
/// treated as a filename.
fn resolve_bundle_path(bundle: &str) -> Result<String> {
    if bundle.starts_with("b3:") {
        bail!("content-addressed bundle '{bundle}' is not supported yet (dev builds load local .wasm files)");
    }
    let p = Path::new(bundle);
    if !bundle.ends_with(".wasm") {
        bail!("bundle '{bundle}' must be a .wasm file");
    }
    let only_normal = p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let parent_ref = bundle.split(['/', '\\']).any(|seg| seg == "..");
    if !only_normal || parent_ref || bundle.contains(':') || bundle.starts_with(['/', '\\']) {
        bail!("bundle '{bundle}' must be a relative path without '..'");
    }
    Ok(bundle.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names() {
        for ok in ["shop.local", "a", "my-app.example.com", "x1.y2"] {
            assert!(valid_app_name(ok), "{ok}");
        }
        for bad in [
            "", "..", "../secret", "a/b", "a\\b", "C:x", "Shop.local", ".a", "a.", "a..b", "-a", "a-",
            "a b", "a%2e%2e",
        ] {
            assert!(!valid_app_name(bad), "{bad}");
        }
    }

    #[test]
    fn bundle_paths() {
        assert_eq!(resolve_bundle_path("shop.wasm").unwrap(), "shop.wasm");
        assert!(resolve_bundle_path("bundles/shop.wasm").is_ok());
        for bad in [
            "../shop.wasm", "a/../../x.wasm", "/etc/x.wasm", "C:\\x.wasm", "C:x.wasm", "\\\\srv\\s\\x.wasm",
            "shop.exe", "b3:abc",
        ] {
            assert!(resolve_bundle_path(bad).is_err(), "{bad}");
        }
    }

    fn write_manifest(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{name}.json")), body).unwrap();
    }

    #[test]
    fn manifest_capabilities_parse() {
        let dir = std::env::temp_dir().join(format!("gwb-manifest-test-{}", std::process::id()));
        write_manifest(
            &dir,
            "app.local",
            r#"{"bundle":"app.wasm","services":{"s":{"iface":"a.b.v1","endpoint":"http://127.0.0.1:1"}},
               "links":["other.local"],"fetch":["https://api.example.com"]}"#,
        );
        let r = resolve("web://app.local", dir.to_str().unwrap()).unwrap();
        assert_eq!(r.bundle_path, "app.wasm");
        assert!(r.links.contains("other.local"));
        assert_eq!(r.fetch.len(), 1);
        assert_eq!(r.services["s"].iface, "a.b.v1");

        write_manifest(&dir, "bad1.local", r#"{"bundle":"../x.wasm"}"#);
        assert!(resolve("web://bad1.local", dir.to_str().unwrap()).is_err());
        write_manifest(&dir, "bad2.local", r#"{"bundle":"x.wasm","fetch":["https://a.com/path"]}"#);
        assert!(resolve("web://bad2.local", dir.to_str().unwrap()).is_err());
        write_manifest(&dir, "bad3.local", r#"{"bundle":"x.wasm","services":{"s":{"iface":"a/../b","endpoint":"http://x"}}}"#);
        assert!(resolve("web://bad3.local", dir.to_str().unwrap()).is_err());
        assert!(resolve("web://../app.local", dir.to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
