//! Read-only Solana lookup against the WASM registry program.
//!
//! Given a 32-byte module hash, this module performs a JSON-RPC
//! [`getProgramAccounts`](https://solana.com/docs/rpc/http/getProgramAccounts) call
//! against `oxide-wasm-registry` (program ID below) using a `memcmp` filter on the
//! `hash` field of [`WasmEntry`](../../solana-wasm-registry/programs/wasm-registry/src/lib.rs).
//!
//! We deliberately avoid pulling in the full Solana SDK: a single `reqwest`
//! POST + manual deserialisation of the `WasmEntry` byte layout is enough.
//!
//! Cluster URL defaults to devnet; override with `OXIDE_SOLANA_RPC_URL`.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

/// Program ID of the on-chain WASM registry. Mirrors `declare_id!` in
/// `solana-wasm-registry/programs/wasm-registry/src/lib.rs`.
pub const PROGRAM_ID: &str = "8CZfcw3uB6wXjmzsQaVmDwxEGvKirwbBpemZE8eF8Sjb";

/// Default Solana JSON-RPC endpoint (devnet). Override with `OXIDE_SOLANA_RPC_URL`.
const DEFAULT_RPC: &str = "https://api.devnet.solana.com";

/// Byte offset of the `hash` field inside the `WasmEntry` account layout:
/// 8 (Anchor discriminator) + 32 (publisher pubkey) = 40.
const HASH_OFFSET: usize = 40;

/// A `WasmEntry` PDA decoded into the fields the UI cares about.
#[derive(Clone, Debug)]
pub struct SolanaAttestation {
    /// Publisher pubkey, base58-encoded.
    pub publisher: String,
    /// Registered module name (publisher-scoped).
    pub name: String,
    /// Monotonic version counter (0 on first register, +1 per update).
    pub version: u32,
    /// Unix seconds when the entry was first registered.
    pub created_at: i64,
    /// Unix seconds of the last update.
    pub updated_at: i64,
    /// Cluster RPC URL we queried.
    pub cluster_rpc: String,
    /// Program ID we queried under.
    pub program_id: String,
}

/// Returns the configured RPC URL (env override or default).
fn rpc_url() -> String {
    std::env::var("OXIDE_SOLANA_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC.to_string())
}

/// Look up a 32-byte module hash on chain. Returns the first matching attestation
/// (a hash collision under different `(publisher, name)` pairs is technically
/// possible but extraordinarily unlikely for SHA-256). `None` on any RPC or
/// decode failure — callers must treat this as a best-effort enrichment.
pub async fn check_hash(hash: [u8; 32]) -> Option<SolanaAttestation> {
    let rpc = rpc_url();
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getProgramAccounts",
        "params": [
            PROGRAM_ID,
            {
                "encoding": "base64",
                "commitment": "confirmed",
                "filters": [
                    {
                        "memcmp": {
                            "offset": HASH_OFFSET,
                            "bytes": B64.encode(hash),
                            "encoding": "base64"
                        }
                    }
                ]
            }
        ]
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let body_text = serde_json::to_string(&body).ok()?;
    let resp = client
        .post(&rpc)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body_text)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let text = resp.text().await.ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let result = value.get("result")?.as_array()?;
    let first = result.first()?;
    let account = first.get("account")?;
    let data = account.get("data")?.as_array()?;
    let encoded = data.first()?.as_str()?;
    let bytes = B64.decode(encoded).ok()?;
    let entry = decode_wasm_entry(&bytes)?;
    Some(SolanaAttestation {
        publisher: bs58::encode(entry.publisher).into_string(),
        name: entry.name,
        version: entry.version,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        cluster_rpc: rpc,
        program_id: PROGRAM_ID.to_string(),
    })
}

struct RawEntry {
    publisher: [u8; 32],
    name: String,
    version: u32,
    created_at: i64,
    updated_at: i64,
}

/// Decode the on-chain byte layout of `WasmEntry`. Layout:
/// `discriminator(8) | publisher(32) | hash(32) | name(4 + N) | version(4) | created_at(8) | updated_at(8) | bump(1)`.
fn decode_wasm_entry(bytes: &[u8]) -> Option<RawEntry> {
    // Discriminator (8) + publisher (32) + hash (32) + name_len (4) = 76
    if bytes.len() < 76 {
        return None;
    }
    let publisher: [u8; 32] = bytes[8..40].try_into().ok()?;
    let name_len = u32::from_le_bytes(bytes[72..76].try_into().ok()?) as usize;
    // Anchor caps name at 64, but be defensive.
    if name_len > 256 {
        return None;
    }
    let name_end = 76usize.checked_add(name_len)?;
    let fixed_tail = name_end.checked_add(4 + 8 + 8 + 1)?;
    if bytes.len() < fixed_tail {
        return None;
    }
    let name = std::str::from_utf8(&bytes[76..name_end]).ok()?.to_string();
    let version = u32::from_le_bytes(bytes[name_end..name_end + 4].try_into().ok()?);
    let ca_off = name_end + 4;
    let created_at = i64::from_le_bytes(bytes[ca_off..ca_off + 8].try_into().ok()?);
    let ua_off = ca_off + 8;
    let updated_at = i64::from_le_bytes(bytes[ua_off..ua_off + 8].try_into().ok()?);
    Some(RawEntry {
        publisher,
        name,
        version,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_well_formed_entry() {
        // Build a synthetic WasmEntry buffer.
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xAA; 8]); // discriminator
        buf.extend_from_slice(&[0x11; 32]); // publisher
        buf.extend_from_slice(&[0x22; 32]); // hash
        let name = "hello";
        buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes()); // version
        buf.extend_from_slice(&100i64.to_le_bytes()); // created_at
        buf.extend_from_slice(&200i64.to_le_bytes()); // updated_at
        buf.push(255); // bump

        let raw = decode_wasm_entry(&buf).expect("decoded");
        assert_eq!(raw.publisher, [0x11; 32]);
        assert_eq!(raw.name, "hello");
        assert_eq!(raw.version, 3);
        assert_eq!(raw.created_at, 100);
        assert_eq!(raw.updated_at, 200);
    }

    #[test]
    fn rejects_too_short() {
        assert!(decode_wasm_entry(&[0u8; 70]).is_none());
    }
}
