//! `org.quire.Memory1.Error.<Variant>`: the bus form of `Refusal`, one to one.

use almanac_core::Refusal;

/// The errors memoryd's methods return. Each maps 1:1 to a [`Refusal`].
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.quire.Memory1.Error")]
pub enum MemoryError {
    /// zbus's own failures.
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The caller's class may not do this.
    NotAllowed(String),
    /// The Space's key is not available.
    SpaceLocked(String),
    /// No such Space.
    SpaceUnknown(String),
    /// The request reaches into another Space.
    OutsideSpace(String),
    /// The closure changed since the plan was made.
    PlanStale(String),
    /// The plan lapsed.
    PlanExpired(String),
    /// No such fact.
    NoSuchFact(String),
    /// The fact is not pending.
    NotPending(String),
    /// Busy; try again.
    Busy(String),
    /// The request is malformed.
    Invalid(String),
}

impl From<&Refusal> for MemoryError {
    fn from(refusal: &Refusal) -> Self {
        let text = String::new();
        match refusal {
            Refusal::NotAllowed => MemoryError::NotAllowed(text),
            Refusal::SpaceLocked => MemoryError::SpaceLocked(text),
            Refusal::SpaceUnknown => MemoryError::SpaceUnknown(text),
            Refusal::OutsideSpace => MemoryError::OutsideSpace(text),
            Refusal::PlanStale => MemoryError::PlanStale(text),
            Refusal::PlanExpired => MemoryError::PlanExpired(text),
            Refusal::NoSuchFact => MemoryError::NoSuchFact(text),
            Refusal::NotPending => MemoryError::NotPending(text),
            Refusal::Busy => MemoryError::Busy(text),
            Refusal::Invalid(why) => MemoryError::Invalid(why.clone()),
        }
    }
}

impl MemoryError {
    /// The refusal this error carries, or `None` for a bus failure.
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            MemoryError::ZBus(_) => None,
            MemoryError::NotAllowed(_) => Some(Refusal::NotAllowed),
            MemoryError::SpaceLocked(_) => Some(Refusal::SpaceLocked),
            MemoryError::SpaceUnknown(_) => Some(Refusal::SpaceUnknown),
            MemoryError::OutsideSpace(_) => Some(Refusal::OutsideSpace),
            MemoryError::PlanStale(_) => Some(Refusal::PlanStale),
            MemoryError::PlanExpired(_) => Some(Refusal::PlanExpired),
            MemoryError::NoSuchFact(_) => Some(Refusal::NoSuchFact),
            MemoryError::NotPending(_) => Some(Refusal::NotPending),
            MemoryError::Busy(_) => Some(Refusal::Busy),
            MemoryError::Invalid(why) => Some(Refusal::Invalid(why.clone())),
        }
    }
}
