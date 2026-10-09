//! The header and its canonical bytes: defined in `almanac-store`, re-exported here.

pub use almanac_store::{
    GENESIS_CONTEXT, HEADER_MAGIC, Header, NewHeader, body_digest, genesis_link, header_bytes, link,
};
