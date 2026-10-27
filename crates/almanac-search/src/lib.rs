//! How memory is searched, as pure values: the recall documents that facts and events
//! contribute to the index ([`docs`]), and `Search` and `Inject` over [`SpaceReader`], the
//! narrow view of one Space that a search reads: its log, its index and its stored facts.
//!
//! Pure over that view: it reads no clock and does no IO of its own. The caller embeds the query
//! first ([`embed_query`], the one async step) and hands the vector in, so a search never holds
//! the index across an await. Writing the audit of a read, and every other reader, stays with
//! whoever owns the Space.

pub mod docs;
mod plan;
mod reader;

pub use plan::{embed_query, inject, search};
pub use reader::SpaceReader;
