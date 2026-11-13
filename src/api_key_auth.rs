//! API key format, generation and verification.
//!
//! ## Current format
//!
//! `imk_<key id, 32 hex>_<secret, 43 url-safe base64>`. The key id selects the
//! stored record directly, and the record holds `sha256:<hex>` of the secret.
//! Checking a key is one hash and a constant-time compare: cheap enough that a
//! flood of bad tokens costs the server nothing.
//!
//! ## Legacy format (deprecated)
//!
//! Older keys were a bare base64 secret with an Argon2 hash and no id, so the
//! only way to find the matching record was to run Argon2 against every
//! stored key: each bad token cost N Argon2 hashes, an easy way to pin the
//! server's CPU. Those keys still work, so existing deployments keep running,
//! but:
//!
//! - verification runs off the async runtime and behind a small concurrency
//!   limit (see `api::auth`), so legacy traffic can't starve anything else;
//! - a successful match is cached by `sha256(token)`, so a legacy client pays
//!   for Argon2 once per server run, not once per request;
//! - every legacy hit logs a deprecation warning (once per key) and the API
//!   response carries `Deprecation` / `Warning` headers.
//!
//! Rotate a legacy key with `ironmud-admin api-key rotate <id>`; it keeps the
//! id, name and permissions and issues a current-format secret.

use crate::ApiKey;
use parking_lot::Mutex;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Prefix of every current-format key.
pub const KEY_PREFIX: &str = "imk_";
/// Prefix of a current-format stored hash.
const HASH_PREFIX: &str = "sha256:";

/// Base64 encode bytes (URL-safe alphabet, no padding).
pub fn base64_url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        }
    }
    result
}

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A freshly generated key: show `raw` to the operator once, store `key_hash`.
pub struct GeneratedKey {
    pub raw: String,
    pub key_hash: String,
}

/// Generate a current-format key for the record `id`.
pub fn generate(id: Uuid) -> GeneratedKey {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let secret = base64_url_encode(&secret);
    GeneratedKey {
        raw: format!("{KEY_PREFIX}{}_{secret}", id.simple()),
        key_hash: format!("{HASH_PREFIX}{}", hex(&sha256(secret.as_bytes()))),
    }
}

/// Split a current-format token into (key id, secret). `None` for anything
/// else, including legacy tokens.
pub fn parse(token: &str) -> Option<(Uuid, &str)> {
    let rest = token.strip_prefix(KEY_PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    if id.len() != 32 || secret.is_empty() {
        return None;
    }
    Some((Uuid::parse_str(id).ok()?, secret))
}

/// True when `token` claims the current format. Such tokens never fall back
/// to the expensive legacy path, even if they fail to verify.
pub fn is_current_format(token: &str) -> bool {
    token.starts_with(KEY_PREFIX)
}

/// True when the stored hash is a legacy Argon2 hash.
pub fn is_legacy_hash(key_hash: &str) -> bool {
    !key_hash.starts_with(HASH_PREFIX)
}

/// Constant-time check of a current-format secret against a stored hash.
pub fn verify_current(secret: &str, key_hash: &str) -> bool {
    let Some(stored_hex) = key_hash.strip_prefix(HASH_PREFIX) else {
        return false;
    };
    let computed = hex(&sha256(secret.as_bytes()));
    computed.as_bytes().ct_eq(stored_hex.as_bytes()).into()
}

/// Cache of verified legacy tokens: `sha256(token)` → (key id, the Argon2
/// hash it matched). The stored hash is re-compared on every hit so a key
/// that has been rotated or deleted stops matching immediately.
type LegacyCache = Mutex<HashMap<[u8; 32], (Uuid, String)>>;

fn legacy_cache() -> &'static LegacyCache {
    static CACHE: OnceLock<LegacyCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Look up a previously verified legacy token.
pub fn legacy_cache_get(token: &str) -> Option<(Uuid, String)> {
    let digest = sha256(token.as_bytes());
    legacy_cache().lock().get(&digest).cloned()
}

/// Remember a verified legacy token.
pub fn legacy_cache_put(token: &str, key: &ApiKey) {
    let digest = sha256(token.as_bytes());
    let mut cache = legacy_cache().lock();
    // Bounded: there are only ever a handful of keys. If something odd fills
    // it, start over rather than grow without limit.
    if cache.len() >= 1024 {
        cache.clear();
    }
    cache.insert(digest, (key.id, key.key_hash.clone()));
}

/// Log the deprecation warning for a legacy key, once per key per run.
pub fn warn_legacy_use(key: &ApiKey) {
    static WARNED: OnceLock<Mutex<HashSet<Uuid>>> = OnceLock::new();
    let first = WARNED.get_or_init(|| Mutex::new(HashSet::new())).lock().insert(key.id);
    if first {
        tracing::warn!(
            "[DEPRECATED] API key '{}' ({}) uses the legacy format. Legacy keys are slow to verify \
             and will stop working in a future release. Rotate it with \
             `ironmud-admin api-key rotate {}` and update the client (e.g. IRONMUD_API_KEY in .mcp.json).",
            key.name,
            key.id,
            key.id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_round_trip() {
        let id = Uuid::new_v4();
        let k = generate(id);
        assert!(k.raw.starts_with(KEY_PREFIX));
        let (parsed_id, secret) = parse(&k.raw).expect("parses");
        assert_eq!(parsed_id, id);
        assert!(verify_current(secret, &k.key_hash));
        assert!(!is_legacy_hash(&k.key_hash));
    }

    #[test]
    fn wrong_secret_or_hash_fails() {
        let k = generate(Uuid::new_v4());
        let (_, secret) = parse(&k.raw).unwrap();
        assert!(!verify_current("not-the-secret", &k.key_hash));
        let other = generate(Uuid::new_v4());
        assert!(!verify_current(secret, &other.key_hash));
        // A legacy Argon2 hash never verifies on the fast path.
        assert!(!verify_current(secret, "$argon2id$v=19$m=19456,t=2,p=1$abc$def"));
    }

    #[test]
    fn malformed_tokens_do_not_parse() {
        for bad in [
            "",
            "imk_",
            "imk_abc_def",
            "imk_00000000000000000000000000000000_",
            "imk_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz_secret",
            "legacybase64token",
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
        assert!(is_current_format("imk_whatever"));
        assert!(!is_current_format("legacybase64token"));
    }
}
