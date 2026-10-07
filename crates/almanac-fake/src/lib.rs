//! Test-only: `fake_service` builds a `MemoryService` over in-memory seams (keys, log, vault,
//! an in-memory SQLite index, a hashed-word embedder), a fixed clock and a scripted
//! consolidator; plus scratch directories and the fixtures the specs talk about. No test
//! touches a bus, a keyring, the network or the real XDG directories.

mod backend;
mod clock;
mod consolidator;
mod fixtures;
mod scratch;

pub use backend::{FAKE_SPACES, FakeBackend, SharedVault, fake_service, fake_space_metas};
pub use clock::{FixedClock, NOW, SteppedClock};
pub use consolidator::ScriptedConsolidator;
pub use fixtures::{
    companion_forwarded, cua_run_step, file_saved_from_attachment, mail, mail_label,
    mail_thread_archived, policy_ask, session_entry, session_thing, thing, trusted_label,
};
pub use scratch::Scratch;
