//! The sealed-file format: `"QMEM" 0x01 ‖ 24-byte nonce ‖ XChaCha20-Poly1305(ciphertext)`.
//!
//! The associated data binds the Space and the vault-relative path, so a sealed file moved to
//! another path or another Space fails to open.

use crate::keys::SubKey;
use almanac_core::SpaceId;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

/// The four bytes every sealed file starts with.
pub const MAGIC: &[u8; 4] = b"QMEM";
/// The format version byte.
pub const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = MAGIC.len() + 1 + NONCE_LEN;

/// Why a sealed file did not open (or could not be made).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    /// Not a sealed file.
    #[error("not a sealed file")]
    BadMagic,
    /// Shorter than the format allows.
    #[error("sealed file is truncated")]
    Truncated,
    /// Wrong key, wrong place, or altered bytes.
    #[error("sealed file failed authentication")]
    Unauthentic,
    /// A format version this build does not know.
    #[error("unknown sealed-file version {0}")]
    UnknownVersion(u8),
    /// The plaintext is too large for the cipher.
    #[error("plaintext too large to seal")]
    TooLarge,
}

/// Associated data: the Space and the vault-relative path, each length-prefixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aad(Vec<u8>);

impl Aad {
    /// The associated data of the file at `path` in `space`.
    pub fn file(space: &SpaceId, path: &str) -> Self {
        let mut bytes = Vec::new();
        for part in [space.as_str(), path] {
            bytes.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_be_bytes());
            bytes.extend_from_slice(part.as_bytes());
        }
        Self(bytes)
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A 24-byte XChaCha nonce. Injected into [`seal`], so sealing is testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub [u8; NONCE_LEN]);

impl Nonce {
    /// A random nonce.
    pub fn random() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut bytes)?;
        Ok(Self(bytes))
    }
}

/// Seals `plain` for the file `aad` names.
pub fn seal(key: &SubKey, aad: &Aad, plain: &[u8], nonce: Nonce) -> Result<Vec<u8>, SealError> {
    let cipher = XChaCha20Poly1305::new(key.expose().into());
    let payload = Payload {
        msg: plain,
        aad: aad.as_bytes(),
    };
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce.0), payload)
        .map_err(|_| SealError::TooLarge)?;
    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&nonce.0);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens bytes made by [`seal`] with the same key and associated data.
pub fn unseal(key: &SubKey, aad: &Aad, sealed: &[u8]) -> Result<Vec<u8>, SealError> {
    let Some(rest) = sealed.strip_prefix(MAGIC.as_slice()) else {
        return Err(if sealed.len() < MAGIC.len() && MAGIC.starts_with(sealed) {
            SealError::Truncated
        } else {
            SealError::BadMagic
        });
    };
    let Some((&version, rest)) = rest.split_first() else {
        return Err(SealError::Truncated);
    };
    if version != VERSION {
        return Err(SealError::UnknownVersion(version));
    }
    if rest.len() < NONCE_LEN + TAG_LEN {
        return Err(SealError::Truncated);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    XChaCha20Poly1305::new(key.expose().into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| SealError::Unauthentic)
}
