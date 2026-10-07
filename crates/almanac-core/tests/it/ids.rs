//! Id grammars: each accepts its forms and refuses the rest.

use almanac_core::*;

#[test]
fn ids_parse_only_their_grammar() {
    let long_topic = "a/b/c/d/e";
    let cases: Vec<(&str, &str, bool)> = vec![
        ("fact", "01j9zk3m0q8h2v6x4c1b7n5t2a", true),
        ("fact", "01J9ZK3M0Q8H2V6X4C1B7N5T2A", false),
        ("fact", "81j9zk3m0q8h2v6x4c1b7n5t2a", false),
        ("fact", "01j9zk3m0q8h2v6x4c1b7n5t2", false),
        ("fact", "01j9zk3m0q8h2v6x4c1b7n5t2u", false),
        ("topic", "people/sam-lee", true),
        ("topic", "prefs", true),
        ("topic", long_topic, false),
        ("topic", "/people", false),
        ("topic", "people//sam", false),
        ("topic", "People/sam", false),
        ("topic", "people/-sam", false),
        ("kind", "thing.archived", true),
        ("kind", "archived", false),
        ("kind", "Thing.archived", false),
        ("pattern", "mail.*", true),
        ("pattern", "*", true),
        ("pattern", "files.file", true),
        ("pattern", ".*", false),
        ("pattern", "mail.*.x", false),
        ("path", "/home/u/a b.txt", true),
        ("path", "", false),
        ("path", "a\nb", false),
        ("fact text", "One paragraph.", true),
        ("fact text", "two\nlines", false),
        ("fact text", "has <!-- comment", false),
        ("fact text", "ends --> here", false),
        ("fact text", "   ", false),
    ];
    for (what, text, ok) in cases {
        let parsed = match what {
            "fact" => FactId::parse(text).is_ok(),
            "topic" => TopicPath::parse(text).is_ok(),
            "kind" => KindTag::parse(text).is_ok(),
            "pattern" => KindPattern::parse(text).is_ok(),
            "path" => SpacePath::parse(text).is_ok(),
            _ => FactText::parse(text).is_ok(),
        };
        assert_eq!(parsed, ok, "{what} {text:?}");
    }
    assert!(FactText::parse(&"x".repeat(2049)).is_err());
}

#[test]
fn minted_fact_ids_sort_by_time_and_parse() {
    let early = FactId::mint(1_000, [1; 10]);
    let late = FactId::mint(2_000, [0; 10]);
    assert!(early < late);
    for id in [&early, &late] {
        assert_eq!(FactId::parse(id.as_str()).as_ref(), Ok(id));
    }
    assert_ne!(FactId::mint(1_000, [1; 10]), FactId::mint(1_000, [2; 10]));
}

#[test]
fn kind_patterns_cover_and_rank() {
    let p = |t: &str| KindPattern::parse(t).expect("pattern");
    assert!(p("mail.*").covers("mail.thread"));
    assert!(!p("mail.*").covers("mail"));
    assert!(!p("mail.*").covers("mailbox.thread"));
    assert!(p("files.file").covers("files.file"));
    assert!(!p("files.file").covers("files.file.x"));
    assert!(p("*").covers("anything.at_all"));
    assert!(p("thing.*").specificity() > p("*").specificity());
    assert!(p("files.file").specificity() > p("files.*").specificity());
}

#[test]
fn digests_and_replicas_are_hex_on_the_wire() {
    let digest = Digest32([0xab; 32]);
    let json = serde_json::to_string(&digest).expect("json");
    assert_eq!(json, format!("\"{}\"", "ab".repeat(32)));
    assert_eq!(
        serde_json::from_str::<Digest32>(&json).expect("back"),
        digest
    );
    assert!(serde_json::from_str::<Digest32>("\"AB\"").is_err());
    let replica = ReplicaId([1; 16]);
    assert_eq!(
        serde_json::to_string(&replica).expect("json"),
        format!("\"{}\"", "01".repeat(16))
    );
}

#[test]
fn user_text_never_shows_in_debug() {
    let view = crate::support::view("org.quire.Mail", "mail.thread", "k", "Secret plans");
    assert!(!format!("{view:?}").contains("Secret plans"));
    assert!(!format!("{:?}", FactText::parse("hidden words").expect("text")).contains("hidden"));
    assert!(!format!("{:?}", UserText::new("hidden")).contains("hidden"));
}
