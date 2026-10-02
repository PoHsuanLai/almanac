//! Keys: the per-Space key, the purpose subkeys derived from it, and the database key.

use almanac_core::{SpaceId, hex_of};
use std::fmt;
use zeroize::Zeroize;

/// One Space's root key. Zeroised on drop, never serialised, never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct SpaceKey([u8; 32]);

impl SpaceKey {
    /// A key from 32 bytes (the key store's, or a test's).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// A fresh random key.
    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// The raw bytes, for the key store only.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for SpaceKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SpaceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SpaceKey(<redacted>)")
    }
}

/// What a subkey is for. Each purpose has its own derivation context, so a key for one use is
/// never valid for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// The event log's SQLCipher key.
    Eventlog,
    /// The recall index's SQLCipher key.
    Index,
    /// Sealing memory files.
    Files,
    /// The keyed digest of event bodies.
    Digest,
}

impl Purpose {
    /// The `blake3::derive_key` context string. Pinned by `derive_is_stable_golden`.
    pub const fn context(self) -> &'static str {
        match self {
            Purpose::Eventlog => "quire-memory 1 eventlog",
            Purpose::Index => "quire-memory 1 index",
            Purpose::Files => "quire-memory 1 files",
            Purpose::Digest => "quire-memory 1 digest",
        }
    }
}

/// A key derived for one purpose in one Space.
#[derive(Clone, PartialEq, Eq)]
pub struct SubKey([u8; 32]);

impl SubKey {
    /// The raw bytes, for the primitives that take a key.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for SubKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SubKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SubKey(<redacted>)")
    }
}

/// The subkey of `key` for `purpose` in `space`: `blake3::derive_key` over the purpose's
/// context, with the key and the length-prefixed Space id as the key material.
pub fn derive(key: &SpaceKey, space: &SpaceId, purpose: Purpose) -> SubKey {
    let mut hasher = blake3::Hasher::new_derive_key(purpose.context());
    hasher.update(key.expose());
    hasher.update(
        &u32::try_from(space.as_str().len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    hasher.update(space.as_str().as_bytes());
    SubKey(*hasher.finalize().as_bytes())
}

/// A SQLCipher raw key, as the pragma wants it.
#[derive(Clone, PartialEq, Eq)]
pub struct DbKey(String);

impl DbKey {
    /// The database key for a subkey (derive it with [`Purpose::Eventlog`] or
    /// [`Purpose::Index`]).
    pub fn of(sub: &SubKey) -> Self {
        Self(hex_of(sub.expose()))
    }

    /// The value of `PRAGMA key`: `x'<64 hex digits>'`, SQLCipher's raw-key form.
    pub fn pragma(&self) -> String {
        format!("x'{}'", self.0)
    }
}

impl Drop for DbKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for DbKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DbKey(<redacted>)")
    }
}
