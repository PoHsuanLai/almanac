//! A removed desktop-wide Space settled by the daemon: the note it keeps so a crash mid-move is
//! finished by the next start, and the Spaces the registry does not know. Real SQLCipher and
//! sealed files over scratch directories; no bus, no clock.

use crate::support::{SharedKeys, TestBackend, dirs_in, service, space};
use almanac_core::*;
use almanac_fake::mail_thread_archived;
use almanac_service::spaces_from_toml;
use memoryd::{Daemon, Removals, SHELL_APP, TablePeers};

type TestDaemon = Daemon<TestBackend, TablePeers>;

const FILES: &str = "org.quire.Files";

fn files_home() -> SpaceId {
    SpaceId::app(&AppName::parse(FILES).expect("app"), LocalSpace(0))
}

fn shell_home() -> SpaceId {
    SpaceId::app(&AppName::parse(SHELL_APP).expect("app"), LocalSpace(0))
}

/// A daemon over `dirs` that knows the Spaces its predecessor wrote down.
fn daemon(dirs: &Dirs, keys: &SharedKeys) -> TestDaemon {
    let service = service(dirs, keys);
    service.set_fallback_owner(AppName::parse(FILES).expect("app"));
    if let Ok(text) = std::fs::read_to_string(dirs.spaces_toml()) {
        for meta in spaces_from_toml(&text).expect("spaces.toml").spaces {
            service.register(meta);
        }
    }
    Daemon::new(service, TablePeers::new(), dirs.clone())
}

fn draft(text: &str) -> FactDraft {
    FactDraft {
        topic: TopicPath::parse("notes/one").expect("topic"),
        text: FactText::parse(text).expect("text"),
        links: vec![],
        supersedes: vec![],
    }
}

/// "old" holds a fact the person wrote through the shell and one the router proposed (pending).
async fn fill_old(daemon: &TestDaemon) {
    let queue = daemon.queue();
    let kept = queue
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Propose(space("old"), draft("kept")),
        )
        .await;
    assert!(
        matches!(kept, MemoryReply::Proposed(_, FactState::Active)),
        "{kept:?}"
    );
    let waiting = queue
        .handle(
            &Caller::Router,
            MemoryRequest::Propose(space("old"), draft("waits")),
        )
        .await;
    assert!(
        matches!(waiting, MemoryReply::Proposed(_, FactState::Pending)),
        "{waiting:?}"
    );
    daemon.persist();
}

async fn pending_in(daemon: &TestDaemon, space: SpaceId) -> usize {
    match daemon
        .queue()
        .handle(&Caller::ShellUi, MemoryRequest::Pending(space))
        .await
    {
        MemoryReply::Pending(facts) => facts.len(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_daemon_that_died_mid_move_finishes_on_the_next_start() {
    let scratch = tempfile::tempdir().expect("scratch");
    let (dirs, keys) = (dirs_in(scratch.path()), SharedKeys::default());
    let first = daemon(&dirs, &keys);
    fill_old(&first).await;
    // It noted the removal and died before moving anything.
    Removals::load(dirs.memory().join("removals.toml")).begin(&space("old"), Removal::KEEP_ALL);
    drop(first);
    assert!(dirs.space(&space("old")).exists());

    let second = daemon(&dirs, &keys);
    let settled = second.resume_removals().await;
    assert_eq!(settled.len(), 1);
    assert_eq!(
        settled[0].1,
        MemoryReply::Relocated(Relocation {
            moved: Count(2),
            kept_pending: Count(1),
            // The audit line of the shell's own fact.
            events_moved: Count(1),
            ..Relocation::NONE
        })
    );
    assert!(!dirs.space(&space("old")).exists(), "the Space is gone");
    assert!(
        Removals::load(dirs.memory().join("removals.toml"))
            .open()
            .is_empty()
    );
    assert_eq!(
        pending_in(&second, files_home()).await,
        1,
        "pending stayed pending"
    );
    assert!(
        dirs.facts(&shell_home()).exists(),
        "the shell's fact went to the shell's Space"
    );
    // Nothing is left to do on the start after that.
    assert!(second.resume_removals().await.is_empty());
}

#[tokio::test]
async fn a_space_the_registry_does_not_know_is_settled_as_removed() {
    let scratch = tempfile::tempdir().expect("scratch");
    let (dirs, keys) = (dirs_in(scratch.path()), SharedKeys::default());
    let first = daemon(&dirs, &keys);
    fill_old(&first).await;
    let kept = first
        .queue()
        .handle(
            &Caller::ShellUi,
            MemoryRequest::Propose(space("work"), draft("stays")),
        )
        .await;
    assert!(matches!(kept, MemoryReply::Proposed(..)), "{kept:?}");
    first.persist();
    drop(first);

    let second = daemon(&dirs, &keys);
    let registry = [DesktopSpace::parse("work").expect("space")];
    let settled = second.reconcile(&registry).await;
    assert_eq!(
        settled.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
        vec![space("old")]
    );
    assert!(
        dirs.space(&space("work")).exists(),
        "a Space the registry has stays"
    );
    assert!(!dirs.space(&space("old")).exists());
    assert_eq!(pending_in(&second, files_home()).await, 1);
}

#[tokio::test]
async fn a_removal_the_registry_announces_moves_the_memories() {
    let scratch = tempfile::tempdir().expect("scratch");
    let (dirs, keys) = (dirs_in(scratch.path()), SharedKeys::default());
    let daemon = daemon(&dirs, &keys);
    fill_old(&daemon).await;
    let gone = DesktopSpace::parse("old").expect("space");
    daemon
        .on_change(&gone, porter_core::SpaceChange::Renamed)
        .await;
    assert!(dirs.space(&space("old")).exists(), "a rename moves nothing");
    daemon
        .on_change(&gone, porter_core::SpaceChange::Removed)
        .await;
    assert!(!dirs.space(&space("old")).exists());
    assert_eq!(pending_in(&daemon, files_home()).await, 1);
}

fn mail_home() -> SpaceId {
    SpaceId::app(
        &AppName::parse("org.quire.Mail").expect("app"),
        LocalSpace(0),
    )
}

/// Mail's archive event, recorded in "old".
async fn record_in_old(daemon: &TestDaemon) {
    let record = Record {
        space: space("old"),
        ..mail_thread_archived().expect("fixture")
    };
    let reply = daemon
        .queue()
        .handle(&Caller::Router, MemoryRequest::Record(record))
        .await;
    assert!(matches!(reply, MemoryReply::Recorded(_)), "{reply:?}");
}

async fn archived_in(daemon: &TestDaemon, space: SpaceId) -> usize {
    let query = TimelineQuery {
        before: None,
        limit: Count(50),
        filter: TimelineFilter {
            actors: ActorFilter::Everyone,
            apps: vec![],
            kinds: vec![KindPattern::parse("thing.archived").expect("kind")],
            trust: TrustFilter::Any,
            range: None,
        },
    };
    match daemon
        .queue()
        .handle(&Caller::ShellUi, MemoryRequest::Timeline(space, query))
        .await
    {
        MemoryReply::Timeline(page) => page.entries.len(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_removal_no_one_asked_about_keeps_memories_and_history() {
    let scratch = tempfile::tempdir().expect("scratch");
    let (dirs, keys) = (dirs_in(scratch.path()), SharedKeys::default());
    let daemon = daemon(&dirs, &keys);
    fill_old(&daemon).await;
    record_in_old(&daemon).await;
    let gone = DesktopSpace::parse("old").expect("space");
    daemon
        .on_change(&gone, porter_core::SpaceChange::Removed)
        .await;
    assert!(!dirs.space(&space("old")).exists());
    assert_eq!(archived_in(&daemon, mail_home()).await, 1, "history kept");
    assert!(dirs.facts(&shell_home()).exists(), "memories moved");
}

#[tokio::test]
async fn a_choice_the_shell_noted_stands_when_the_registry_announces_the_removal() {
    let scratch = tempfile::tempdir().expect("scratch");
    let (dirs, keys) = (dirs_in(scratch.path()), SharedKeys::default());
    let first = daemon(&dirs, &keys);
    fill_old(&first).await;
    record_in_old(&first).await;
    first.persist();
    // The shell asked to delete the history and the daemon died before it moved anything.
    Removals::load(dirs.memory().join("removals.toml")).begin(
        &space("old"),
        Removal {
            memories: MemoryFate::MoveToApps,
            history: HistoryFate::Delete,
        },
    );
    drop(first);

    let second = daemon(&dirs, &keys);
    let gone = DesktopSpace::parse("old").expect("space");
    second
        .on_change(&gone, porter_core::SpaceChange::Removed)
        .await;
    assert!(!dirs.space(&space("old")).exists());
    assert_eq!(
        archived_in(&second, mail_home()).await,
        0,
        "history deleted"
    );
    assert!(dirs.facts(&shell_home()).exists(), "memories moved");
}

#[test]
fn a_note_from_before_the_history_choice_reads_as_keep_everything() {
    let scratch = tempfile::tempdir().expect("scratch");
    let path = scratch.path().join("removals.toml");
    std::fs::write(&path, "removed = [\"old\"]\n").expect("write");
    assert_eq!(
        Removals::load(path).open(),
        vec![(space("old"), Removal::KEEP_ALL)]
    );
}
