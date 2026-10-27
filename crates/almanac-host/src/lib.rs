//! The files memory keeps beside its Spaces, for whoever hosts the service: memoryd, or an app
//! that hosts it in process (`almanac-local`).
//!
//! - [`Locator`] finds `$XDG_CONFIG_HOME/almanac/settings.toml` (then each of `$XDG_CONFIG_DIRS`),
//!   reads it through `almanac-settings`' lenient reader and gate, and keeps the person's
//!   confirmations of loosened values in a store of its own.
//! - [`write_private_atomic`] and [`read_config`] write and read a config file whole.
//!
//! The settings model and the service read no file; they are handed what this crate reads.

mod files;
mod locate;

pub use files::{read_config, write_private_atomic};
pub use locate::Locator;
