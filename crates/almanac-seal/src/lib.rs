//! almanac's keys and the sealed-file format: a root key per Space, subkeys per purpose,
//! XChaCha20-Poly1305 files bound to their Space and path, and the key store seam.
//!
//! Pure over its seams; `oo7` is behind the `oo7` feature.

#[cfg(feature = "test-keys")]
mod file;
mod keys;
#[cfg(feature = "testing")]
mod memory;
#[cfg(feature = "oo7")]
mod oo7;
mod seal;
mod store;

#[cfg(feature = "test-keys")]
pub use file::FileKeys;
pub use keys::{DbKey, Purpose, SpaceKey, SubKey, derive};
#[cfg(feature = "testing")]
pub use memory::MemoryKeys;
#[cfg(feature = "oo7")]
pub use oo7::{Oo7Keys, SCHEMA, attributes, create_in, destroy_in, get_in};
pub use seal::{Aad, MAGIC, Nonce, SealError, VERSION, seal, unseal};
pub use store::{KeyError, KeyStore};
