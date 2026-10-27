//! The consolidation prompt and the parse of the model's answer: what the prompt lists and
//! fences, and how each kind of hunk is built, checked and dropped.

use almanac_core::*;
use almanac_fake::{
    ana_topic_text, consolidation_event, consolidation_event_link, consolidation_input,
};
use almanac_tidy::{ConsolidateError, Draft, InputTopic, check_draft, parse_draft, render_prompt};

#[test]
fn the_prompt_lists_the_facts_and_the_events_and_says_what_to_answer() {
    let prompt = render_prompt(&consolidation_input());
    assert!(prompt.contains("ONE JSON object"));
    assert!(prompt.contains("\"kind\":\"promote\""));
    assert!(prompt.contains(&format!("\"id\":\"{}\"", fact_id())));
    assert!(prompt.contains("Ana is the CFO."));
    let listed: Vec<serde_json::Value> = prompt
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let wanted = serde_json::to_value(consolidation_event(1)).expect("json");
    assert!(
        listed.iter().any(|l| l.get("event") == Some(&wanted)),
        "an event line carries its reference exactly as a link needs it: {listed:?}"
    );
    assert!(prompt.contains("thing.archived"));
}

#[test]
fn the_prompt_fences_its_data_and_leaves_out_what_does_not_fit() {
    let mut big = consolidation_input();
    big.topics.push(InputTopic {
        topic: TopicPath::parse("people/huge").expect("topic"),
        text: UserText::new("x".repeat(100_000)),
    });
    let prompt = render_prompt(&big);
    let run = big.run.to_string();
    for name in ["facts", "topics", "events"] {
        assert!(
            prompt.contains(&format!("=====BEGIN DATA {name} {run}")),
            "{name}"
        );
        assert!(
            prompt.contains(&format!("=====END DATA {name} {run}=====")),
            "{name}"
        );
    }
    assert!(
        !prompt.contains("people/huge"),
        "a file past the budget is left out"
    );
    assert!(prompt.len() < 60_000, "{}", prompt.len());
}

#[test]
fn the_prompt_shows_the_topic_files_and_how_to_tidy_one() {
    let prompt = render_prompt(&consolidation_input());
    assert!(prompt.contains("\"kind\":\"tidy\""));
    let shown: Vec<serde_json::Value> = prompt
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|l: &serde_json::Value| l.get("topic").is_some() && l.get("text").is_some())
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["topic"], "people/ana");
    assert_eq!(shown[0]["text"].as_str(), Some(ana_topic_text().as_str()));
}

fn tidy_answer(topic: &str, after: &str) -> String {
    serde_json::json!({"hunks": [{"kind": "tidy", "topic": topic, "after": after}]}).to_string()
}

#[test]
fn a_tidy_of_a_shown_file_becomes_a_hunk_with_the_text_the_model_saw() {
    let reworded = ana_topic_text().replace("title: ana\n", "title: Ana\n");
    assert_ne!(reworded, ana_topic_text());
    let input = consolidation_input();
    let draft = parse_draft(&tidy_answer("people/ana", &reworded), &input).expect("draft");
    let [Hunk::Tidy(tidy)] = draft.hunks.as_slice() else {
        panic!("{:?}", draft.hunks)
    };
    assert_eq!(tidy.topic, TopicPath::parse("people/ana").expect("topic"));
    assert_eq!(tidy.before.as_str(), ana_topic_text());
    assert_eq!(tidy.after.as_str(), reworded);
    let checked = check_draft(&input, draft);
    assert!(checked.dropped.is_empty(), "{:?}", checked.dropped);
}

#[test]
fn a_tidy_that_is_not_one_is_dropped() {
    let input = consolidation_input();
    let text = ana_topic_text();
    let cases = [
        ("an unchanged file", "people/ana", text.clone()),
        (
            "a topic it was not shown",
            "people/bo",
            text.replace("CFO", "COO"),
        ),
        (
            "a topic that is not a path",
            "../x",
            text.replace("CFO", "COO"),
        ),
        (
            "a file that lost its fact",
            "people/ana",
            text.lines()
                .filter(|l| !l.contains("fact:") && !l.contains("CFO"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        ("not a topic file", "people/ana", "just words".to_owned()),
    ];
    for (name, topic, after) in cases {
        let draft = parse_draft(&tidy_answer(topic, &after), &input).expect("draft");
        assert!(draft.hunks.is_empty(), "{name}: {:?}", draft.hunks);
    }
}

fn fact_id() -> FactId {
    FactId::mint(1, [1; 10])
}

#[test]
fn a_promotion_cites_events_and_takes_the_join_of_their_labels() {
    let answer = format!(
        r#"{{"hunks":[{{"kind":"promote","text":"Ana sent the Lisbon budget.","from":[{},{}]}}]}}"#,
        consolidation_event_link(1),
        consolidation_event_link(2)
    );
    let input = consolidation_input();
    let draft = parse_draft(&answer, &input).expect("draft");
    let [Hunk::Promote { fact, to }] = draft.hunks.as_slice() else {
        panic!("{:?}", draft.hunks)
    };
    assert_eq!(fact.text.as_str(), "Ana sent the Lisbon budget.");
    assert_eq!(fact.recorded, input.now);
    assert_eq!(
        fact.by,
        Actor::System {
            part: SystemPart::Memory
        }
    );
    assert_eq!(
        fact.label.integrity,
        Integrity::Untrusted,
        "the stranger's mail taints it"
    );
    assert_eq!(*to, Lands::Pending, "so it waits for the person");
    assert_eq!(
        fact.links,
        vec![
            Link::Event(consolidation_event(1)),
            Link::Event(consolidation_event(2))
        ]
    );
    let checked = check_draft(&input, draft);
    assert!(checked.dropped.is_empty(), "{:?}", checked.dropped);
}

#[test]
fn a_trusted_promotion_lands_active_and_ids_are_deterministic() {
    let answer = format!(
        r#"```json
{{"hunks":[{{"kind":"promote","text":"Ana archived the thread.","from":[{}]}}]}}
```"#,
        consolidation_event_link(1)
    );
    let a = parse_draft(&answer, &consolidation_input()).expect("draft");
    let b = parse_draft(&answer, &consolidation_input()).expect("draft");
    assert_eq!(a, b, "the same run and text give the same fact");
    let [Hunk::Promote { to, fact }] = a.hunks.as_slice() else {
        panic!("{:?}", a.hunks)
    };
    assert_eq!(*to, Lands::Active);
    let other = answer.replace("archived the thread", "archived another one");
    let c = parse_draft(&other, &consolidation_input()).expect("draft");
    let [
        Hunk::Promote {
            fact: other_fact, ..
        },
    ] = c.hunks.as_slice()
    else {
        panic!("{:?}", c.hunks)
    };
    assert_ne!(fact.id, other_fact.id);
}

#[test]
fn a_supersede_and_a_flag_are_built_and_pass_the_check() {
    let answer = format!(
        r#"{{"hunks":[
          {{"kind":"supersede","old":"{old}","text":"Ana is the CEO now.","from":[{from}]}},
          {{"kind":"flag","facts":["{old}"],"note":"the title changed"}}
        ]}}"#,
        old = fact_id(),
        from = consolidation_event_link(1)
    );
    let input = consolidation_input();
    let draft = parse_draft(&answer, &input).expect("draft");
    assert_eq!(draft.hunks.len(), 2);
    assert!(
        matches!(&draft.hunks[0], Hunk::Supersede { old, new } if *old == fact_id() && new.supersedes == vec![fact_id()])
    );
    assert!(matches!(&draft.hunks[1], Hunk::Flag { facts, .. } if facts == &vec![fact_id()]));
    assert!(check_draft(&input, draft).dropped.is_empty());
}

#[test]
fn hunks_that_cannot_be_built_are_dropped_one_by_one() {
    let bad_cite = serde_json::to_string(&Link::Event(consolidation_event(99))).expect("json");
    let answer = format!(
        r#"{{"hunks":[
          {{"kind":"promote","text":"cites nothing","from":[]}},
          {{"kind":"promote","text":"cites a stranger","from":[{bad_cite}]}},
          {{"kind":"promote","text":"","from":[{good}]}},
          {{"kind":"obliterate","everything":true}},
          {{"kind":"flag","facts":[],"note":"nothing flagged"}},
          {{"kind":"promote","text":"kept","from":[{good}]}}
        ]}}"#,
        good = consolidation_event_link(1)
    );
    let draft = parse_draft(&answer, &consolidation_input()).expect("draft");
    assert_eq!(draft.hunks.len(), 1, "{:?}", draft.hunks);
    assert!(matches!(&draft.hunks[0], Hunk::Promote { fact, .. } if fact.text.as_str() == "kept"));
}

#[test]
fn an_answer_that_is_not_the_object_is_unparseable() {
    for answer in [
        "",
        "I would promote nothing.",
        "{\"hunks\": 3}",
        "{\"nothing\":[]}",
        "}{",
    ] {
        assert_eq!(
            parse_draft(answer, &consolidation_input()),
            Err(ConsolidateError::Unparseable),
            "{answer:?}"
        );
    }
    assert_eq!(
        parse_draft("{\"hunks\":[]}", &consolidation_input()),
        Ok(Draft { hunks: vec![] })
    );
}

#[test]
fn at_most_sixty_four_hunks_are_read() {
    let flag = format!(r#"{{"kind":"flag","facts":["{}"],"note":"x"}}"#, fact_id());
    let answer = format!("{{\"hunks\":[{}]}}", vec![flag; 80].join(","));
    assert_eq!(
        parse_draft(&answer, &consolidation_input())
            .expect("draft")
            .hunks
            .len(),
        64
    );
}
