//! Forget wins over Revert. The last applied run keeps whole topic files as they were so
//! `Revert` can put them back; text a forget removed must never come back through them. The rule,
//! the simplest that is obviously safe: any forget that erases anything (facts or event bodies)
//! while pre-images are held drops them all, and `Revert` of the run is refused with a reason the
//! shell can show. No text is matched and nothing is partially restored. Pure: no vault, no clock.

use crate::hunks::PreImage;
use crate::runfile::Gone;

/// What the reason says when `Revert` is refused after a forget.
pub(crate) const REFUSED: &str = "a memory this run touched was forgotten since";

/// Whether the last run can still be reverted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RevertGuard {
    /// No forget has happened since the run applied.
    #[default]
    Open,
    /// A forget happened while pre-images were held: they are gone and `Revert` is refused.
    Forgotten,
}

/// The guard after a forget of `gone`, and the pre-images left: all of them while nothing was
/// erased or none were held, none once a forget closes the guard.
pub(crate) fn after_forget(
    guard: RevertGuard,
    pre_images: Vec<PreImage>,
    gone: &Gone,
) -> (RevertGuard, Vec<PreImage>) {
    let erased = !gone.facts.is_empty() || !gone.events.is_empty();
    match guard {
        RevertGuard::Open if erased && !pre_images.is_empty() => {
            (RevertGuard::Forgotten, Vec::new())
        }
        RevertGuard::Forgotten => (guard, Vec::new()),
        RevertGuard::Open => (guard, pre_images),
    }
}
