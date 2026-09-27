//! Ed25519 identities and message signing.
//!
//! Every node (and every client) owns a [`Keypair`]. Messages are signed over a
//! canonical, length-prefixed payload so that a recipient can tell *who* sent a
//! message and that the *contents* were not altered in transit.

mod error;
mod keypair;
mod signature;

pub use error::{Error, Result};
pub use keypair::Keypair;
pub use signature::{PublicKey, Signature};

/// Length of an Ed25519 public key, in bytes.
pub const PUBLIC_KEY_LEN: usize = 32;
/// Length of an Ed25519 secret key, in bytes.
pub const SECRET_KEY_LEN: usize = 32;
/// Length of an Ed25519 signature, in bytes.
pub const SIGNATURE_LEN: usize = 64;

/// Build the canonical byte string that gets signed.
///
/// The domain and every field are length-prefixed, so distinct inputs can never
/// collide (e.g. `["ab", "c"]` and `["a", "bc"]` differ).
pub fn canonical_payload(domain: &str, fields: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    out.extend_from_slice(domain.as_bytes());
    out.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    for field in fields {
        out.extend_from_slice(&(field.len() as u32).to_be_bytes());
        out.extend_from_slice(field);
    }
    out
}

/// Encode a timestamp as the signed big-endian byte representation.
pub fn timestamp_field(timestamp_ms: u64) -> [u8; 8] {
    timestamp_ms.to_be_bytes()
}

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}
