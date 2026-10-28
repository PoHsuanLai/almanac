//! What the `desktop` scope admits: only facts the person stated themselves (preferences,
//! names, style). The rule is almanac's, so it lives here and not in the shared vocabulary.

use porter_core::SpaceId;
use prov::{Confidentiality, Flow, Integrity, Label, Source};

/// Why the desktop scope will not hold a labelled value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopRefusal {
    /// It is secret.
    Secret,
    /// It is untrusted: something else wrote it, or a model derived it from that.
    Untrusted,
    /// Trusted, but not only the person's own words (an app's metadata, a model's inference).
    NotUserStated,
    /// It is private to a real Space, and the desktop scope would show it to all of them.
    SpaceBound,
}

/// May the desktop scope hold a value with this label? Only what the person stated: trusted,
/// sourced from the person alone, not secret, and not private to a real Space.
pub fn desktop_admits(label: &Label) -> Result<(), DesktopRefusal> {
    let only_user = label.sources.len() == 1 && label.sources.contains(&Source::User);
    if label.confidentiality == Confidentiality::Secret {
        Err(DesktopRefusal::Secret)
    } else if label.integrity == Integrity::Untrusted {
        Err(DesktopRefusal::Untrusted)
    } else if !only_user {
        Err(DesktopRefusal::NotUserStated)
    } else if label.confidentiality.flow_to(&SpaceId::desktop()) == Flow::Denied {
        Err(DesktopRefusal::SpaceBound)
    } else {
        Ok(())
    }
}
