//! Public keys and signatures.

use std::fmt;
use std::str::FromStr;

use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey};

use crate::{Error, PUBLIC_KEY_LEN, Result, SIGNATURE_LEN};

/// An Ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    pub(crate) fn new(key: VerifyingKey) -> Self {
        Self(key)
    }

    /// Parse a public key from raw bytes.
    pub fn from_bytes(bytes: &[u8; PUBLIC_KEY_LEN]) -> Result<Self> {
        VerifyingKey::from_bytes(bytes)
            .map(Self)
            .map_err(|error| Error::PublicKey(error.to_string()))
    }

    /// The raw public-key bytes.
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LEN] {
        self.0.to_bytes()
    }

    /// Verify `signature` over `message`.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        self.0
            .verify_strict(message, &signature.0)
            .map_err(|_| Error::Verification)
    }

    /// Lowercase hex encoding (used as the node/client identity string).
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Parse a public key from lowercase or uppercase hex.
    pub fn from_hex(text: &str) -> Result<Self> {
        let bytes = hex::decode(text)?;
        let array: [u8; PUBLIC_KEY_LEN] = bytes.as_slice().try_into().map_err(|_| {
            Error::PublicKey(format!(
                "expected {PUBLIC_KEY_LEN} bytes, got {}",
                bytes.len()
            ))
        })?;
        Self::from_bytes(&array)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.to_hex())
    }
}

impl FromStr for PublicKey {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::from_hex(text)
    }
}

/// An Ed25519 signature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(Ed25519Signature);

impl Signature {
    pub(crate) fn new(signature: Ed25519Signature) -> Self {
        Self(signature)
    }

    /// Parse a signature from raw bytes.
    pub fn from_bytes(bytes: &[u8; SIGNATURE_LEN]) -> Self {
        Self(Ed25519Signature::from_bytes(bytes))
    }

    /// Parse a signature from a byte slice of any length.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        Ed25519Signature::try_from(bytes)
            .map(Self)
            .map_err(|error| Error::Signature(error.to_string()))
    }

    /// The raw signature bytes.
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LEN] {
        self.0.to_bytes()
    }

    /// Lowercase hex encoding.
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Parse a signature from hex.
    pub fn from_hex(text: &str) -> Result<Self> {
        let bytes = hex::decode(text)?;
        Self::from_slice(&bytes)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", self.to_hex())
    }
}
