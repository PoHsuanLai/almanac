//! memoryd's view of porter's registry of desktop-wide Spaces (`org.quire.Spaces1`).

use almanac_core::SpaceId;
use almanac_service::{Listing, RegistryError};
use porter_client::Spaces;
use porter_client_dbus::Bus;
use std::collections::BTreeSet;
use std::time::Duration;

/// How long the registry has to answer `List` before the Space is refused (fail closed).
pub const REGISTRY_WAIT: Duration = Duration::from_secs(3);

/// Where the desktop-wide Spaces are listed.
#[derive(Debug)]
pub enum SpaceRegistry {
    /// porter's registry, over the bus.
    Porter(Spaces<Bus>),
    /// A fixed list (a test's, or a host with its own).
    Listed(BTreeSet<SpaceId>),
    /// No registry: nothing is listed, and an app opens no desktop-wide Space.
    Closed,
}

impl SpaceRegistry {
    /// A fixed list of these Spaces.
    pub fn listing(spaces: impl IntoIterator<Item = SpaceId>) -> Self {
        Self::Listed(spaces.into_iter().collect())
    }

    /// Whether `space` is listed. A registry that does not answer within [`REGISTRY_WAIT`], or
    /// answers badly, is [`RegistryError::Unreachable`].
    pub(crate) async fn is_listed(&self, space: &SpaceId) -> Result<Listing, RegistryError> {
        match self {
            Self::Porter(spaces) => {
                let listed = tokio::time::timeout(REGISTRY_WAIT, spaces.list())
                    .await
                    .map_err(|_| RegistryError::Unreachable)?
                    .map_err(|_| RegistryError::Unreachable)?;
                let found = listed
                    .iter()
                    .any(|record| &SpaceId::linked(&record.id) == space);
                Ok(listing_of(found))
            }
            Self::Listed(listed) => Ok(listing_of(listed.contains(space))),
            Self::Closed => Err(RegistryError::Unreachable),
        }
    }
}

fn listing_of(found: bool) -> Listing {
    match found {
        true => Listing::Listed,
        false => Listing::Unlisted,
    }
}
