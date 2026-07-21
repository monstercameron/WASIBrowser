//! App manifest resolution + `web://` navigation (docs/04-WEB-RPC.md §4).
//!
//! `renderer web://shop.local` resolves `{manifest_root}/shop.local.json` into a
//! bundle path + a service registry (the capabilities the app may call). This
//! replaces the hardcoded `.wasm` path — the browser navigates by name, and the
//! app can only call services the manifest declares. A bare `.wasm` path still
//! works (back-compat): it yields a default manifest with a dev `echo` service.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};

use crate::abi::ServiceEntry;

pub struct ResolvedApp {
    pub bundle_path: String,
    pub title: String,
    pub services: HashMap<String, ServiceEntry>,
    /// Manifest-declared cross-site link targets (bare `web://` names this
    /// app may navigate to/open a new tab for). Mirrors `services`' capability
    /// model — the `navigate` host import rejects any target not in this set.
    pub links: HashSet<String>,
    /// True when loaded from a filesystem bundle without b3: verification — we
    /// log this so the trust downgrade is never silent (Constitution rule 1).
    pub dev_unverified: bool,
}

/// Resolve a navigation target (a `web://name` address or a `.wasm` path).
pub fn resolve(target: &str, manifest_root: &str) -> Result<ResolvedApp> {
    if let Some(name) = target.strip_prefix("web://") {
        resolve_manifest(name, manifest_root)
    } else {
        // Bare wasm path: synthesize a minimal manifest with the dev echo
        // service so smoke tests / ad-hoc guests still work.
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
            dev_unverified: true,
        })
    }
}

fn dev_endpoint() -> String {
    std::env::var("GWB_RPC_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8787".to_string())
}

fn resolve_manifest(name: &str, manifest_root: &str) -> Result<ResolvedApp> {
    let path = format!("{}/{}.json", manifest_root.trim_end_matches('/'), name);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading manifest for web://{name} at {path}"))?;
    let m: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing manifest {path}"))?;

    let bundle = m
        .get("bundle")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("manifest {path}: missing 'bundle'"))?;
    // A b3: bundle would be fetched + hash-verified here (production). A local
    // filesystem path in dev is loaded as-is; the caller logs dev_unverified.
    let bundle_path = resolve_bundle_path(bundle, manifest_root);
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
            let iface = spec
                .get("iface")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("service '{svc_name}': missing 'iface'"))?
                .to_string();
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
                links.insert(name.to_string());
            }
        }
    }

    Ok(ResolvedApp {
        bundle_path,
        title,
        services,
        links,
        dev_unverified: !bundle.starts_with("b3:"),
    })
}

/// A `b3:` bundle resolves via the swarm/cache (not wired in dev); a relative
/// path resolves against the manifest root's parent (the app dir), an absolute
/// path as-is.
fn resolve_bundle_path(bundle: &str, manifest_root: &str) -> String {
    if bundle.starts_with("b3:") {
        return bundle.to_string(); // production: content-addressed fetch (TODO)
    }
    let p = std::path::Path::new(bundle);
    if p.is_absolute() || bundle.contains('/') || bundle.contains('\\') {
        bundle.to_string()
    } else {
        // bare filename: sits next to the renderer cwd (where bundles are built)
        bundle.to_string()
    }
    .to_string()
    .replace("{root}", manifest_root)
}
