use almanac_core::{EventRef, FactId, IndexPart, ReplicaId, Seq, SpaceId};
use almanac_search::docs::{DocRef, event_doc_id, fact_doc_id};

fn event(seq: u64) -> EventRef {
    EventRef {
        space: SpaceId::parse("work").expect("space"),
        replica: ReplicaId([9; 16]),
        seq: Seq(seq),
    }
}

#[test]
fn a_fact_document_id_names_its_fact() {
    let id = FactId::parse("01j9zk3m0q8h2v6x4c1b7n5t2a").expect("fact id");
    assert_eq!(
        DocRef::parse(&fact_doc_id(&id)),
        Some(DocRef::Fact(id.clone()))
    );
}

#[test]
fn an_event_document_id_names_its_replica_and_sequence() {
    let at = event(7);
    for id in [event_doc_id(&at), IndexPart::Narrative.doc_id(&at)] {
        assert_eq!(
            DocRef::parse(&id),
            Some(DocRef::Event {
                replica: at.replica,
                seq: at.seq
            })
        );
    }
}

#[test]
fn an_id_that_names_nothing_parses_to_nothing() {
    for id in ["", "x:1", "f:not-a-fact", "e:zz:1", "e:00"] {
        assert_eq!(DocRef::parse(id), None, "{id}");
    }
}
