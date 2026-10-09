//! `Entries`: one stream of the log, oldest first, from a cursor.

use crate::backend::Backend;
use crate::open::{Cx, Open, log_refusal};
use almanac_core::{
    Cursor, EntriesPage, EntriesQuery, KindPattern, ReadScope, RecentEntry, Refusal, Seq,
};
use almanac_store::{Entry, LogRead, RoleFilter};

/// The entries of `candidates` (oldest first) whose kind matches `kinds`, at most `limit`, and
/// whether more matched after them. Pure.
fn take_page(
    candidates: impl Iterator<Item = Entry>,
    kinds: &[KindPattern],
    limit: usize,
) -> (Vec<Entry>, bool) {
    let mut matching = candidates
        .filter(|e| kinds.is_empty() || kinds.iter().any(|k| k.covers(e.header.kind.as_str())));
    let page: Vec<Entry> = matching.by_ref().take(limit).collect();
    let more = matching.next().is_some();
    (page, more)
}

/// The cursor to resume after `page`: its last entry's position, when `more` follows.
fn resume_after(page: &[Entry], more: bool) -> Option<Cursor> {
    page.last().filter(|_| more).map(|e| Cursor(e.header.seq))
}

impl<B: Backend> Open<B> {
    /// The next page of a stream, oldest first, with bodies when asked.
    pub(crate) fn entries_page(
        &mut self,
        cx: &Cx<'_, B>,
        q: EntriesQuery,
    ) -> Result<EntriesPage, Refusal> {
        let after = q.after.map_or(Seq(0), |c| c.0);
        let limit = usize::try_from(q.limit.0).unwrap_or(usize::MAX);
        let (page, more) = match &q.about {
            Some(thing) => {
                let seqs = self
                    .rt
                    .log
                    .touching(thing, RoleFilter::Subject)
                    .map_err(log_refusal)?;
                let entries = seqs
                    .into_iter()
                    .filter(|s| *s > after)
                    .filter_map(|s| self.entry_at(s));
                take_page(entries, &q.kinds, limit)
            }
            None => {
                let entries = self.rt.log.scan(Seq(after.0 + 1)).map_err(log_refusal)?;
                take_page(entries.into_iter(), &q.kinds, limit)
            }
        };
        let entries: Vec<RecentEntry> = page
            .iter()
            .map(|e| self.recent_entry(e, q.bodies))
            .collect();
        self.audit_by(cx, ReadScope::Recent, Vec::new(), entries.len())?;
        Ok(EntriesPage {
            next: resume_after(&page, more),
            entries,
        })
    }
}
