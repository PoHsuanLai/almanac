//! What a key store failure means to a client.

use almanac_core::Refusal;
use almanac_seal::KeyError;

/// A destroyed key is final (`SpaceUnknown`); only a store that may recover answers `Busy`.
pub(crate) fn key_refusal(e: KeyError) -> Refusal {
    match e {
        KeyError::Locked => Refusal::SpaceLocked,
        KeyError::Destroyed => Refusal::SpaceUnknown,
        KeyError::Missing | KeyError::Exists | KeyError::Store(_) => Refusal::Busy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_destroyed_key_is_not_retryable() {
        assert_eq!(key_refusal(KeyError::Destroyed), Refusal::SpaceUnknown);
        assert_eq!(key_refusal(KeyError::Store("x".into())), Refusal::Busy);
        assert_eq!(key_refusal(KeyError::Locked), Refusal::SpaceLocked);
    }
}
