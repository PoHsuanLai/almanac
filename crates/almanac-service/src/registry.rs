//! The seam to porter's registry of desktop-wide Spaces (`org.quire.Spaces1`, which any
//! identified caller may list): an app reaches a desktop-wide Space only if the registry lists
//! it, so an app cannot provision a Space nobody made.

/// Whether the registry lists a desktop-wide Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// The registry has it.
    Listed,
    /// The registry does not have it.
    Unlisted,
}

/// Why the registry could not be asked. The service fails closed on it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// The registry did not answer in time, or is not there.
    #[error("the Space registry did not answer")]
    Unreachable,
}
