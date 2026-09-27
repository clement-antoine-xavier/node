//! Keypair generation, persistence and signing.

use std::fmt;
use std::path::Path;

use ed25519_dalek::{Signer, SigningKey};

use crate::{Error, PublicKey, Result, SECRET_KEY_LEN, Signature};

/// An Ed25519 keypair.
///
/// `Debug` is implemented by hand so the secret key is never printed.
#[derive(Clone)]
pub struct Keypair {
    signing: SigningKey,
}

impl Keypair {
    /// Generate a fresh keypair from the operating system RNG.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; SECRET_KEY_LEN];
        getrandom::fill(&mut bytes).map_err(|error| Error::Random(error.to_string()))?;
        Ok(Self::from_secret_bytes(&bytes))
    }

    /// Build a keypair from raw secret-key bytes.
    pub fn from_secret_bytes(bytes: &[u8; SECRET_KEY_LEN]) -> Self {
        Self {
            signing: SigningKey::from_bytes(bytes),
        }
    }

    /// The raw secret-key bytes. Handle with care.
    pub fn secret_bytes(&self) -> [u8; SECRET_KEY_LEN] {
        self.signing.to_bytes()
    }

    /// The corresponding public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey::new(self.signing.verifying_key())
    }

    /// Sign `message` with this key.
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature::new(self.signing.sign(message))
    }

    /// Load the keypair from `path`, generating and saving one if absent.
    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let keypair = Self::generate()?;
            keypair.save(path)?;
            Ok(keypair)
        }
    }

    /// Load a keypair from a hex-encoded key file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let bytes = hex::decode(text.trim())?;
        let secret: [u8; SECRET_KEY_LEN] = bytes.as_slice().try_into().map_err(|_| {
            Error::KeyFile(format!(
                "expected a {SECRET_KEY_LEN}-byte secret key, got {} bytes",
                bytes.len()
            ))
        })?;
        Ok(Self::from_secret_bytes(&secret))
    }

    /// Persist the keypair as hex, with owner-only permissions on Unix.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, hex::encode(self.secret_bytes()))?;
        restrict_permissions(path)?;
        Ok(())
    }
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keypair")
            .field("public_key", &self.public_key().to_hex())
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
