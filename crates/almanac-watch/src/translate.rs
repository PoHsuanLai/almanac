//! From notify's events to `Observed`: rename halves paired by their cookie, folders left out,
//! inodes remembered so a delete still names the file it removed, small files digested.

use crate::observed::{Observed, Stamp};
use almanac_core::{ContentDigest, FileChange, SpacePath};
use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind};
use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::task::Waker;
use std::time::SystemTime;

/// Files larger than this are not digested.
pub const DIGEST_LIMIT: u64 = 1 << 20;

/// What the handler thread and `next` share.
#[derive(Debug, Default)]
pub struct Inbox {
    /// Observations ready for `next`.
    pub queue: VecDeque<Observed>,
    /// Who to wake when one arrives.
    pub waker: Option<Waker>,
    /// `MOVED_FROM` halves waiting for their `MOVED_TO`, by cookie.
    moved_from: HashMap<usize, PathBuf>,
    /// The last inode seen at each path.
    inodes: HashMap<PathBuf, u64>,
}

/// The current time as a [`Stamp`].
pub fn now() -> Stamp {
    let since = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Stamp(i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

fn space_path(path: &Path) -> Option<SpacePath> {
    SpacePath::parse(path.to_str()?).ok()
}

impl Inbox {
    /// Takes one notify event; whatever it settles into goes on the queue.
    pub fn accept(&mut self, event: Event, at: Stamp) {
        let path = event.paths.first().cloned();
        let cookie = event.tracker();
        match (event.kind, path) {
            (EventKind::Create(CreateKind::File | CreateKind::Any), Some(p)) => {
                self.push(&p, FileChange::Created, at);
            }
            (EventKind::Modify(ModifyKind::Data(_)), Some(p)) => {
                self.push(&p, FileChange::Modified, at);
            }
            (EventKind::Access(AccessKind::Close(AccessMode::Write)), Some(p)) => {
                self.push(&p, FileChange::Closed, at);
            }
            (EventKind::Remove(RemoveKind::File | RemoveKind::Any), Some(p)) => {
                self.push(&p, FileChange::Deleted, at);
            }
            (EventKind::Modify(ModifyKind::Name(RenameMode::From)), Some(p)) => {
                if let Some(cookie) = cookie {
                    self.moved_from.insert(cookie, p);
                }
            }
            (EventKind::Modify(ModifyKind::Name(RenameMode::To)), Some(to)) => {
                let from = cookie.and_then(|c| self.moved_from.remove(&c));
                match from.as_deref().and_then(space_path) {
                    Some(from) => self.push(&to, FileChange::Renamed { from }, at),
                    None if to.is_dir() => {}
                    None => self.push(&to, FileChange::Created, at),
                }
            }
            // `Both` repeats what `From` and `To` already said; folders and metadata are not
            // file changes.
            _ => {}
        }
    }

    /// Turns `From` halves that never found their `To` (the file left the watched tree) into
    /// deletions.
    pub fn flush_unpaired(&mut self, at: Stamp) {
        let gone: Vec<PathBuf> = self.moved_from.drain().map(|(_, p)| p).collect();
        for path in gone {
            self.push(&path, FileChange::Deleted, at);
        }
    }

    /// Whether a `From` half is waiting.
    pub fn has_unpaired(&self) -> bool {
        !self.moved_from.is_empty()
    }

    fn push(&mut self, path: &Path, change: FileChange, at: Stamp) {
        let Some(space_path) = space_path(path) else {
            return;
        };
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file());
        let inode = match (&meta, &change) {
            (Some(m), _) => {
                self.inodes.insert(path.to_owned(), m.ino());
                m.ino()
            }
            (None, FileChange::Deleted) => self.inodes.remove(path).unwrap_or(0),
            (None, _) => self.inodes.get(path).copied().unwrap_or(0),
        };
        let content = meta
            .filter(|m| m.len() <= DIGEST_LIMIT && !matches!(change, FileChange::Deleted))
            .and_then(|_| std::fs::read(path).ok())
            .map(|bytes| ContentDigest(*blake3::hash(&bytes).as_bytes()));
        self.queue.push_back(Observed {
            path: space_path,
            change,
            inode,
            content,
            at,
            by_app: None,
        });
    }
}
