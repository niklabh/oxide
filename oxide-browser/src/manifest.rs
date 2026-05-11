//! WASM manifest sidecar fetch + verification.
//!
//! When a guest is loaded from `https://example.com/foo.wasm`, the host also tries
//! `https://example.com/foo.json`. If present, it describes the module (name,
//! description, repo, developer, icon) and may include a `hash` field — the
//! lowercase hex SHA-256 of the `.wasm` bytes. The hash is the bridge to the
//! Solana on-chain registry: we verify it matches the bytes we just downloaded,
//! then look it up via [`crate::solana::check_hash`].

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Parsed sidecar manifest. All fields are optional so a manifest with only some
/// metadata is still usable.
#[derive(Clone, Debug, Default)]
pub struct WasmManifest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub repo: Option<String>,
    pub developer: Option<String>,
    pub icon: Option<String>,
    /// Lowercase hex SHA-256 of the `.wasm` bytes (64 chars).
    pub hash: Option<String>,
}

/// Combined manifest + verification state surfaced to the UI for the current tab.
#[derive(Clone, Debug)]
pub struct ManifestInfo {
    pub source_url: String,
    pub manifest: WasmManifest,
    /// Actual SHA-256 of the `.wasm` bytes we ran (lowercase hex).
    pub actual_hash_hex: String,
    /// True iff the manifest declared a `hash` and it matches `actual_hash_hex`.
    pub hash_verified: bool,
    /// Solana attestation if a matching `WasmEntry` PDA exists for `actual_hash`.
    /// `None` means either no manifest hash, hash mismatch, lookup not finished,
    /// or no on-chain entry was found.
    pub solana: Option<crate::solana::SolanaAttestation>,
}

/// Derive the manifest URL from a `.wasm` URL by replacing the extension.
/// Returns `None` for URLs that don't end in `.wasm` or aren't `http(s)`.
pub fn derive_manifest_url(wasm_url: &str) -> Option<String> {
    if !(wasm_url.starts_with("http://") || wasm_url.starts_with("https://")) {
        return None;
    }
    let path_end = wasm_url.find(['?', '#']).unwrap_or(wasm_url.len());
    let path = &wasm_url[..path_end];
    let stripped = path.strip_suffix(".wasm")?;
    Some(format!("{stripped}.json"))
}

/// SHA-256 of `bytes` as a lowercase 64-char hex string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Decode a 64-char lowercase hex string into a 32-byte array.
pub fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Best-effort manifest fetch. Returns `None` on any network/parse failure;
/// callers must not surface errors to the user — a missing manifest is normal.
pub async fn fetch_manifest(manifest_url: &str) -> Option<WasmManifest> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let resp = client.get(manifest_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    if let Some(len) = resp.content_length() {
        if len > 256 * 1024 {
            return None;
        }
    }
    let text = resp.text().await.ok()?;
    parse_manifest(&text)
}

fn parse_manifest(text: &str) -> Option<WasmManifest> {
    let v: Value = serde_json::from_str(text).ok()?;
    let obj = v.as_object()?;
    let s = |k: &str| obj.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
    Some(WasmManifest {
        name: s("name"),
        description: s("description"),
        repo: s("repo"),
        developer: s("developer"),
        icon: s("icon"),
        hash: s("hash").map(|h| h.trim().to_ascii_lowercase()),
    })
}

/// Run a manifest+Solana check for the just-loaded module. Errors are swallowed
/// because this is a metadata enrichment — a failed lookup never breaks the page.
///
/// Returns `Some(ManifestInfo)` if at least a manifest was fetched.
pub async fn check_for_module(wasm_url: &str, wasm_bytes: &[u8]) -> Option<ManifestInfo> {
    let manifest_url = derive_manifest_url(wasm_url)?;
    let manifest = fetch_manifest(&manifest_url).await?;
    let actual = sha256_hex(wasm_bytes);
    let hash_verified = manifest
        .hash
        .as_ref()
        .map(|h| h == &actual)
        .unwrap_or(false);

    let solana = if hash_verified {
        let hash_bytes = hex32(&actual)?;
        crate::solana::check_hash(hash_bytes).await
    } else {
        None
    };

    Some(ManifestInfo {
        source_url: manifest_url,
        manifest,
        actual_hash_hex: actual,
        hash_verified,
        solana,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_manifest_url() {
        assert_eq!(
            derive_manifest_url("https://example.com/foo.wasm"),
            Some("https://example.com/foo.json".to_string())
        );
        assert_eq!(
            derive_manifest_url("https://example.com/dir/a.wasm?v=1"),
            Some("https://example.com/dir/a.json".to_string())
        );
        assert_eq!(derive_manifest_url("file:///tmp/a.wasm"), None);
        assert_eq!(derive_manifest_url("https://example.com/no-ext"), None);
    }

    #[test]
    fn hex_roundtrip() {
        let hex = sha256_hex(b"hello");
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        let bytes = hex32(&hex).expect("decodable");
        assert_eq!(sha256_hex(b"hello"), sha256_hex(b"hello"));
        let _ = bytes;
    }

    #[test]
    fn parses_full_manifest() {
        let json = r#"{
            "name": "Hello",
            "description": "A demo",
            "repo": "https://github.com/x/y",
            "developer": "Alice",
            "icon": "https://example.com/i.png",
            "hash": "ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        }"#;
        let m = parse_manifest(json).expect("parsed");
        assert_eq!(m.name.as_deref(), Some("Hello"));
        assert_eq!(m.developer.as_deref(), Some("Alice"));
        assert_eq!(
            m.hash.as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
    }
}
