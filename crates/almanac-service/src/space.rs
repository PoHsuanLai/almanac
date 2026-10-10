//! The Space lifecycle machine (memory section 4.1): effects are returned as values.

use almanac_core::{Count, DropReason, SpaceState, UnixSeconds};

/// At most this many records are buffered in memory while a Space is locked.
pub const BUFFER_LIMIT: u32 = 512;

/// What happened to a Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpaceEvent {
    /// Its key became available.
    KeyAvailable,
    /// Its key was lost (the keyring locked).
    KeyLost,
    /// A record arrived; `buffered` is how many are already waiting.
    Record {
        /// Records waiting in memory.
        buffered: Count,
    },
    /// The person paused memory.
    Pause {
        /// Until when.
        until: UnixSeconds,
    },
    /// The person resumed it.
    Resume,
    /// Time passed.
    Tick {
        /// Now.
        now: UnixSeconds,
    },
    /// The person confirmed forgetting the whole Space.
    ForgetConfirmed,
    /// Deletion finished.
    Done,
}

/// What the service must do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpaceEffect {
    /// Open the databases.
    OpenDatabases,
    /// Check the log's head against the desktop log's anchor.
    VerifyHeadAgainstAnchor,
    /// Admit the buffered records, oldest first.
    FlushBuffer,
    /// Keep the record in memory.
    Buffer,
    /// Hand the record to admission.
    Admit,
    /// Count a dropped record (a `Dropped` audit event later).
    CountDropped(DropReason),
    /// Log `Memory.Paused`.
    LogPaused {
        /// Until when.
        until: UnixSeconds,
    },
    /// Log `Memory.Resumed`.
    LogResumed,
    /// Emit `StatusChanged`.
    EmitStatus,
    /// Close the databases.
    CloseDatabases,
    /// Record the final head in the desktop log.
    AnchorFinalHead,
    /// Destroy the Space's key.
    DestroyKey,
    /// Remove the Space's directories.
    RemoveDirs,
}

/// The next state and the effects.
pub fn step(state: SpaceState, event: SpaceEvent) -> (SpaceState, Vec<SpaceEffect>) {
    use SpaceEffect as E;
    use SpaceEvent as V;
    use SpaceState as S;
    match (state, event) {
        (S::Gone, _) => (S::Gone, vec![]),
        (_, V::KeyLost) => (S::Locked, vec![E::CloseDatabases, E::EmitStatus]),
        (S::Locked, V::KeyAvailable) => (
            S::Open,
            vec![
                E::OpenDatabases,
                E::VerifyHeadAgainstAnchor,
                E::FlushBuffer,
                E::EmitStatus,
            ],
        ),
        (S::Locked, V::Record { buffered }) if buffered.0 < BUFFER_LIMIT => {
            (S::Locked, vec![E::Buffer])
        }
        (S::Locked, V::Record { .. }) => {
            (S::Locked, vec![E::CountDropped(DropReason::SpaceLocked)])
        }
        (S::Open, V::Record { .. }) => (S::Open, vec![E::Admit]),
        (paused @ S::Paused { .. }, V::Record { .. }) => (paused, vec![E::Admit]),
        (S::Deleting, V::Record { .. }) => {
            (S::Deleting, vec![E::CountDropped(DropReason::SpaceUnknown)])
        }
        (S::Open | S::Paused { .. }, V::Pause { until }) => (
            S::Paused { until },
            vec![E::LogPaused { until }, E::EmitStatus],
        ),
        (S::Paused { .. }, V::Resume) => (S::Open, vec![E::LogResumed, E::EmitStatus]),
        (S::Paused { until }, V::Tick { now }) if now >= until => {
            (S::Open, vec![E::LogResumed, E::EmitStatus])
        }
        (S::Open | S::Paused { .. }, V::ForgetConfirmed) => (S::Deleting, vec![]),
        (S::Deleting, V::Done) => (
            S::Gone,
            vec![E::AnchorFinalHead, E::DestroyKey, E::RemoveDirs],
        ),
        (unchanged, _) => (unchanged, vec![]),
    }
}
