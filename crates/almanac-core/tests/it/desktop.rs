//! The desktop scope admits only what the person stated.

use almanac_core::*;
use std::collections::BTreeSet;

fn space(id: &str) -> SpaceId {
    SpaceId::parse(id).expect("space id")
}

fn label(integrity: Integrity, confidentiality: Confidentiality, sources: &[Source]) -> Label {
    Label {
        integrity,
        confidentiality,
        classes: BTreeSet::new(),
        sources: sources.iter().cloned().collect(),
    }
}

fn private(spaces: &[&str]) -> Confidentiality {
    Confidentiality::Private(spaces.iter().map(|s| space(s)).collect())
}

#[test]
fn the_desktop_scope_admits_only_what_the_person_stated() {
    use DesktopRefusal::*;
    let user = &[Source::User];
    let cases = [
        (
            label(Integrity::Trusted, Confidentiality::Public, user),
            Ok(()),
        ),
        (
            label(Integrity::Trusted, private(&["desktop"]), user),
            Ok(()),
        ),
        (
            label(Integrity::Trusted, private(&["work"]), user),
            Err(SpaceBound),
        ),
        (
            label(Integrity::Trusted, Confidentiality::Secret, user),
            Err(Secret),
        ),
        (
            label(Integrity::Untrusted, Confidentiality::Public, user),
            Err(Untrusted),
        ),
        (
            label(
                Integrity::Untrusted,
                Confidentiality::Secret,
                &[Source::Mail],
            ),
            Err(Secret),
        ),
        (
            label(
                Integrity::Trusted,
                Confidentiality::Public,
                &[Source::App(AppName::parse("org.quire.Mail").expect("app"))],
            ),
            Err(NotUserStated),
        ),
        (
            label(
                Integrity::Trusted,
                Confidentiality::Public,
                &[Source::User, Source::Model(ModelRole::Consolidator)],
            ),
            Err(NotUserStated),
        ),
        (
            label(Integrity::Trusted, Confidentiality::Public, &[]),
            Err(NotUserStated),
        ),
    ];
    for (l, want) in cases {
        assert_eq!(desktop_admits(&l), want, "{l:?}");
    }
}
