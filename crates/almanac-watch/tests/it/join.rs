//! The file-why join table (portable: no watcher involved).

use almanac_core::*;
use almanac_watch::*;

fn path(p: &str) -> SpacePath {
    SpacePath::parse(p).expect("path")
}

fn app(name: &str) -> AppName {
    AppName::parse(name).expect("app")
}

fn observed(p: &str, change: FileChange, at: i64, content: Option<u8>) -> Observed {
    Observed {
        path: path(p),
        change,
        inode: 77,
        content: content.map(|b| ContentDigest([b; 32])),
        at: Stamp(at),
        by_app: None,
    }
}

fn why(p: &str, at: i64, content: u8) -> WhyAt {
    WhyAt {
        claim: FileWhyClaim {
            space: SpaceId::parse("work").expect("space"),
            path: path(p),
            content: ContentDigest([content; 32]),
            cause: ThingRef {
                app: app("org.quire.Mail"),
                kind: ThingKind::parse("mail.message").expect("k"),
                key: ThingKey::parse("m1").expect("k"),
            },
            verb: Verb::Downloaded,
            by: Actor::User {
                via: app("org.quire.Mail"),
            },
        },
        at: Stamp(at),
    }
}

const W: JoinWindow = JoinWindow::PROPOSED;
const F: &str = "/home/u/Downloads/receipt.pdf";

fn explained(j: &Joined) -> bool {
    matches!(j.event.why, FileWhy::Explained { .. })
}

#[test]
fn join_table() {
    // (name, pending, now, settled count, explained flags of the settled, waiting observed, waiting whys)
    type Row = (&'static str, Pending, i64, Vec<bool>, usize, usize);
    let cases: Vec<Row> = vec![
        (
            "observed and why together",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![why(F, 10_500, 1)],
            },
            10_600,
            vec![true],
            0,
            0,
        ),
        (
            "the why comes first",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_500, Some(1))],
                whys: vec![why(F, 9_000, 1)],
            },
            10_600,
            vec![true],
            0,
            0,
        ),
        (
            "exactly at the window edge",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, None)],
                whys: vec![why(F, 12_000, 1)],
            },
            12_000,
            vec![true],
            0,
            0,
        ),
        (
            "no content on the watcher's side still matches",
            Pending {
                observed: vec![observed(F, FileChange::Modified, 10_000, None)],
                whys: vec![why(F, 10_100, 9)],
            },
            10_200,
            vec![true],
            0,
            0,
        ),
        (
            "different content is not the same write",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![why(F, 10_100, 2)],
            },
            20_000,
            vec![false, true],
            0,
            0,
        ),
        (
            "a different path",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![why("/home/u/other.pdf", 10_100, 1)],
            },
            20_000,
            vec![false, true],
            0,
            0,
        ),
        (
            "outside the window",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![why(F, 12_001, 1)],
            },
            20_000,
            vec![false, true],
            0,
            0,
        ),
        (
            "observed alone, window not elapsed",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![],
            },
            11_000,
            vec![],
            1,
            0,
        ),
        (
            "observed alone, window elapsed",
            Pending {
                observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
                whys: vec![],
            },
            12_001,
            vec![false],
            0,
            0,
        ),
        (
            "why alone, window not elapsed",
            Pending {
                observed: vec![],
                whys: vec![why(F, 10_000, 1)],
            },
            11_000,
            vec![],
            0,
            1,
        ),
        (
            "why alone, window elapsed",
            Pending {
                observed: vec![],
                whys: vec![why(F, 10_000, 1)],
            },
            12_001,
            vec![true],
            0,
            0,
        ),
        ("nothing", Pending::default(), 1, vec![], 0, 0),
    ];
    for (name, pending, now, flags, waiting_o, waiting_w) in cases {
        let step = join(pending, Stamp(now), W);
        let got: Vec<bool> = step.settled.iter().map(explained).collect();
        assert_eq!(got, flags, "{name}");
        assert_eq!(
            (step.waiting.observed.len(), step.waiting.whys.len()),
            (waiting_o, waiting_w),
            "{name}"
        );
    }
}

#[test]
fn an_explained_event_carries_the_apps_claim() {
    let step = join(
        Pending {
            observed: vec![observed(F, FileChange::Created, 10_000, Some(1))],
            whys: vec![why(F, 10_100, 1)],
        },
        Stamp(10_200),
        W,
    );
    let event = &step.settled[0].event;
    assert_eq!(
        event.actor,
        Actor::User {
            via: app("org.quire.Mail")
        }
    );
    assert_eq!(event.file.inode, 77);
    assert_eq!(event.file.content, ContentDigest([1; 32]));
    assert!(matches!(
        &event.why,
        FileWhy::Explained {
            verb: Verb::Downloaded,
            ..
        }
    ));
}

#[test]
fn each_why_explains_one_observation_oldest_first() {
    let pending = Pending {
        observed: vec![
            observed(F, FileChange::Created, 10_000, None),
            observed(F, FileChange::Modified, 10_900, None),
        ],
        whys: vec![why(F, 10_950, 1)],
    };
    let step = join(pending, Stamp(20_000), W);
    let flags: Vec<bool> = step.settled.iter().map(explained).collect();
    assert_eq!(
        flags,
        [true, false],
        "the oldest observation takes the why; the next is unexplained"
    );
}

#[test]
fn unexplained_changes_are_unknown_unless_the_process_resolved() {
    let mut anonymous = observed(F, FileChange::Created, 1_000, Some(1));
    let mut known = observed("/home/u/b.txt", FileChange::Created, 1_001, Some(2));
    known.by_app = Some(app("org.mozilla.firefox"));
    anonymous.by_app = None;
    let step = join(
        Pending {
            observed: vec![anonymous, known],
            whys: vec![],
        },
        Stamp(9_000),
        W,
    );
    assert_eq!(step.settled[0].event.actor, Actor::Unknown);
    assert_eq!(
        step.settled[1].event.actor,
        Actor::App {
            app: app("org.mozilla.firefox")
        }
    );
}

#[test]
fn a_rename_records_an_alias_so_memory_follows_the_file() {
    let renamed = observed(
        "/home/u/new.pdf",
        FileChange::Renamed {
            from: path("/home/u/old.pdf"),
        },
        1_000,
        Some(1),
    );
    let step = join(
        Pending {
            observed: vec![renamed],
            whys: vec![],
        },
        Stamp(9_000),
        W,
    );
    assert_eq!(
        step.settled[0].alias,
        Some(Alias {
            from: path("/home/u/old.pdf"),
            to: path("/home/u/new.pdf")
        })
    );
    assert!(matches!(
        step.settled[0].event.change,
        FileChange::Renamed { .. }
    ));
}

#[test]
fn settled_events_are_oldest_first() {
    let pending = Pending {
        observed: vec![
            observed("/a", FileChange::Created, 3_000, None),
            observed("/b", FileChange::Created, 1_000, None),
        ],
        whys: vec![why("/c", 2_000, 1)],
    };
    let step = join(pending, Stamp(99_000), W);
    let paths: Vec<&str> = step
        .settled
        .iter()
        .map(|j| j.event.file.path.as_str())
        .collect();
    assert_eq!(paths, ["/b", "/c", "/a"]);
}

#[test]
fn a_why_alone_maps_its_verb_to_a_change() {
    let cases = [
        (Verb::Downloaded, FileChange::Created),
        (Verb::Saved, FileChange::Modified),
        (Verb::Deleted, FileChange::Deleted),
        (Verb::Viewed, FileChange::Closed),
    ];
    for (verb, change) in cases {
        assert_eq!(change_for(verb), change, "{verb:?}");
    }
}
