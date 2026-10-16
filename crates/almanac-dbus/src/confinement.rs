//! The `Confinement` property of `org.quire.Memory1.Control`: whether the daemon keeps itself to
//! its own folders (its Landlock sandbox), as one stable word.

/// How far the daemon confines itself. The words of [`Confinement::as_word`] are a public
/// contract: the Settings app reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    /// Every restriction of the policy is in force.
    Confined,
    /// The kernel's Landlock is older than the policy: part of it is in force.
    Partial,
    /// The kernel has no Landlock: nothing is restricted by the daemon.
    NotAvailable,
    /// A test build ran with the sandbox switched off. Never in a normal build.
    Off,
}

impl Confinement {
    /// The word the property carries.
    pub const fn as_word(self) -> &'static str {
        match self {
            Confinement::Confined => "confined",
            Confinement::Partial => "partial",
            Confinement::NotAvailable => "not-available",
            Confinement::Off => "off",
        }
    }
}
