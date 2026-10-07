//! The inotify contract (feature `linux`): create, rename and delete in a scratch directory.
#![cfg(feature = "linux")]

use almanac_core::*;
use almanac_watch::*;

fn path(p: &str) -> SpacePath {
    SpacePath::parse(p).expect("path")
}

/// Polls a future to completion on this thread, giving up after `limit`.
fn block_on<T>(limit: std::time::Duration, fut: impl std::future::Future<Output = T>) -> Option<T> {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    let deadline = std::time::Instant::now() + limit;
    loop {
        if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
            return Some(value);
        }
        let left = deadline.checked_duration_since(std::time::Instant::now())?;
        std::thread::park_timeout(left);
    }
}

/// inotify instances are a per-user budget; the watcher tests take turns.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn take_turn() -> std::sync::MutexGuard<'static, ()> {
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A watcher; the per-user instance budget is shared with every other program on the machine, so
/// a full budget is waited out rather than failed.
fn watcher() -> InotifyWatch {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match InotifyWatch::new() {
            Err(WatchError::LimitReached) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            other => return other.expect("watcher"),
        }
    }
}

const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

fn next_observed(watch: &mut InotifyWatch) -> Observed {
    block_on(WAIT, watch.next())
        .expect("an observation in time")
        .expect("the stream is open")
}

/// The observations until one satisfies `done`, skipping the rest.
fn until(watch: &mut InotifyWatch, done: impl Fn(&Observed) -> bool) -> Observed {
    loop {
        let seen = next_observed(watch);
        if done(&seen) {
            return seen;
        }
    }
}

fn scratch_root(dir: &tempfile::TempDir) -> (std::path::PathBuf, SpacePath) {
    // The platform may hand out a symlinked temp directory; inotify reports real paths.
    let root = std::fs::canonicalize(dir.path()).expect("canonical");
    let space = path(root.to_str().expect("utf8"));
    (root, space)
}

#[test]
fn inotify_sees_create_rename_delete_in_scratch_dir() {
    let _turn = take_turn();
    let dir = tempfile::tempdir().expect("scratch");
    let (root, space) = scratch_root(&dir);
    let mut watch = watcher();
    watch.watch(&space).expect("watch");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    let (a_text, b_text) = (a.to_str().expect("utf8"), b.to_str().expect("utf8"));

    std::fs::write(&a, b"x").expect("write");
    let created = until(&mut watch, |o| o.change == FileChange::Created);
    assert_eq!(created.path.as_str(), a_text);
    let inode = created.inode;
    assert_ne!(inode, 0);

    std::fs::rename(&a, &b).expect("rename");
    let renamed = until(&mut watch, |o| {
        matches!(o.change, FileChange::Renamed { .. })
    });
    assert_eq!(renamed.path.as_str(), b_text);
    assert_eq!(renamed.change, FileChange::Renamed { from: path(a_text) });
    assert_eq!(renamed.inode, inode, "a rename keeps the inode");
    assert_eq!(
        renamed.content,
        Some(ContentDigest(*blake3::hash(b"x").as_bytes()))
    );

    std::fs::remove_file(&b).expect("remove");
    let deleted = until(&mut watch, |o| o.change == FileChange::Deleted);
    assert_eq!(deleted.path.as_str(), b_text);
    assert_eq!(deleted.inode, inode, "a delete still names the inode");
    assert_eq!(deleted.content, None);
}

#[test]
fn inotify_watches_subdirectories_and_ignores_folders() {
    let _turn = take_turn();
    let dir = tempfile::tempdir().expect("scratch");
    let (root, space) = scratch_root(&dir);
    let mut watch = watcher();
    watch.watch(&space).expect("watch");
    std::fs::create_dir(root.join("sub")).expect("mkdir");
    // Give the recursive watch time to attach before writing inside.
    std::thread::sleep(std::time::Duration::from_millis(200));
    std::fs::write(root.join("sub/c.txt"), b"y").expect("write");
    let seen = next_observed(&mut watch);
    assert_eq!(
        seen.change,
        FileChange::Created,
        "the folder is not reported"
    );
    assert_eq!(
        seen.path.as_str(),
        root.join("sub/c.txt").to_str().expect("utf8")
    );
}

#[test]
fn a_file_moved_out_of_the_tree_counts_as_deleted() {
    let _turn = take_turn();
    let dir = tempfile::tempdir().expect("scratch");
    let outside = tempfile::tempdir().expect("scratch");
    let (root, space) = scratch_root(&dir);
    let mut watch = watcher();
    watch.watch(&space).expect("watch");
    std::fs::write(root.join("a.txt"), b"x").expect("write");
    until(&mut watch, |o| o.change == FileChange::Created);
    std::fs::rename(root.join("a.txt"), outside.path().join("a.txt")).expect("move out");
    let gone = until(&mut watch, |o| o.change == FileChange::Deleted);
    assert_eq!(
        gone.path.as_str(),
        root.join("a.txt").to_str().expect("utf8")
    );
}

#[test]
fn watching_a_missing_root_or_unwatching_an_unwatched_one_fails() {
    let _turn = take_turn();
    let dir = tempfile::tempdir().expect("scratch");
    let (root, _) = scratch_root(&dir);
    let missing = path(root.join("nope").to_str().expect("utf8"));
    let mut watch = watcher();
    assert!(matches!(
        watch.watch(&missing),
        Err(WatchError::Unwatchable(_))
    ));
    assert!(matches!(
        watch.unwatch(&missing),
        Err(WatchError::Unwatchable(_))
    ));
}

#[test]
fn nothing_is_reported_without_a_root_and_unwatch_silences_a_root() {
    let _turn = take_turn();
    let dir = tempfile::tempdir().expect("scratch");
    let (root, space) = scratch_root(&dir);
    let mut watch = watcher();
    std::fs::write(root.join("quiet.txt"), b"x").expect("write");
    assert!(block_on(std::time::Duration::from_millis(300), watch.next()).is_none());
    watch.watch(&space).expect("watch");
    watch.unwatch(&space).expect("unwatch");
    std::fs::write(root.join("quiet2.txt"), b"x").expect("write");
    assert!(block_on(std::time::Duration::from_millis(300), watch.next()).is_none());
}
