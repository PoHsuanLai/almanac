//! The pure file format of almanac's agent memory: the topic file (front matter, dated headings,
//! bullets with their trailers; frozen, with parse and render built) and the fact `Store` over a
//! vault seam. No file is touched here: the vault comes from `almanac-store`, and `memfiles`
//! supplies the ones on disk.

mod store;
mod topic;
mod trailer;

pub use store::{MemfilesError, PRIMER_MAX_LINES, Primer, PrimerEntry, Store};
pub use topic::{Block, FORMAT_LINE, ParseError, TopicFile, parse_topic, render_topic};
pub use trailer::{TrailerFault, parse_trailer, render_trailer};
