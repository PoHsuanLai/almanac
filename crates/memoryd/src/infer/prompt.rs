//! The consolidation prompt and the parse of the model's answer: pure, so they are tables.
//!
//! The model is asked for one JSON object of simple hunks (`promote`, `supersede`, `flag`) that
//! cite what they come from; memoryd builds the real `Hunk`s from them: the fact's id from the
//! run and its text, its date from the run, its author as memory itself, and its label as the
//! join of the labels of everything it cites (`almanac_service::cited_label`), which is also what
//! `check_draft` holds every promotion to. A hunk that cites nothing in the input, or whose text
//! is not fact text, is dropped; an answer that is not the JSON object at all is `Unparseable`.

use almanac_core::{
    Actor, DataClass, Fact, FactId, FactText, Hunk, Label, Link, SystemPart, UserText, Validity,
};
use almanac_service::{ConsolidateError, ConsolidationInput, Draft, cited_label};
use serde::Deserialize;

/// The most hunks one answer may hold; the rest are ignored.
const MAX_HUNKS: usize = 64;

/// The data class a request carries when its input mixes several: the most sensitive present, in
/// this order (the grant and the on-device floor are per class, so the request takes the
/// strictest of them). Nothing classed at all is the app's own data.
const BY_SENSITIVITY: [DataClass; 11] = [
    DataClass::Voice,
    DataClass::Mail,
    DataClass::Contacts,
    DataClass::Calendar,
    DataClass::Notes,
    DataClass::Files,
    DataClass::Photos,
    DataClass::Clipboard,
    DataClass::Screen,
    DataClass::AppOwn,
    DataClass::Public,
];

/// The class a request over `labels` is sent as.
pub fn class_of<'a>(labels: impl IntoIterator<Item = &'a Label>) -> DataClass {
    let present: Vec<&Label> = labels.into_iter().collect();
    BY_SENSITIVITY
        .into_iter()
        .find(|class| present.iter().any(|l| l.classes.contains(class)))
        .unwrap_or(DataClass::AppOwn)
}

fn line<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The prompt for one run: what to answer, then the active facts and the events since the last
/// run, one JSON object per line. Event bodies are not in it (`InputEvent` has none).
pub fn render_prompt(input: &ConsolidationInput) -> String {
    let mut out = String::from(
        "You keep a person's memory notes tidy. Reply with ONE JSON object and nothing else:\n\
         {\"hunks\":[...]}\n\
         A hunk is one of\n\
         {\"kind\":\"promote\",\"text\":\"one sentence worth remembering\",\"from\":[LINK,...]}\n\
         {\"kind\":\"supersede\",\"old\":\"FACT-ID\",\"text\":\"the corrected sentence\",\"from\":[LINK,...]}\n\
         {\"kind\":\"flag\",\"facts\":[\"FACT-ID\",...],\"note\":\"why they look wrong or stale\"}\n\
         A LINK is {\"kind\":\"event\",\"v\":EVENT} or {\"kind\":\"fact\",\"v\":\"FACT-ID\"} or \
         {\"kind\":\"thing\",\"v\":THING}, copied exactly from the lists below. Every promote and \
         supersede must cite at least one. Use an empty list when nothing should change.\n\n\
         Facts:\n",
    );
    for fact in &input.facts {
        out.push_str(&line(&serde_json::json!({
            "id": fact.id.to_string(),
            "text": fact.text.as_str(),
        })));
        out.push('\n');
    }
    out.push_str("\nEvents since the last run:\n");
    for event in &input.events {
        out.push_str(&line(&serde_json::json!({
            "event": event.event,
            "kind": event.kind.as_str(),
            "things": event.things.iter().map(|t| serde_json::json!({
                "thing": t.thing,
                "title": t.title,
            })).collect::<Vec<_>>(),
        })));
        out.push('\n');
    }
    out
}

#[derive(Deserialize)]
struct Answer {
    hunks: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ModelHunk {
    Promote {
        text: String,
        from: Vec<Link>,
    },
    Supersede {
        old: FactId,
        text: String,
        from: Vec<Link>,
    },
    Flag {
        facts: Vec<FactId>,
        note: String,
    },
}

/// The JSON object inside an answer, whether or not the model fenced it or said something first.
fn object_in(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (start < end).then(|| &text[start..=end])
}

/// The id of the `index`th fact of a run: the date of the run and 80 bits of a hash of the run,
/// the position and the text (memoryd's ids carry no ambient randomness; the run id has its own).
fn id_for(input: &ConsolidationInput, index: usize, text: &str) -> FactId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"QCONS1");
    hasher.update(input.run.to_string().as_bytes());
    hasher.update(&u64::try_from(index).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(text.as_bytes());
    let mut random = [0u8; 10];
    random.copy_from_slice(&hasher.finalize().as_bytes()[..10]);
    let ms = u64::try_from(input.now.0)
        .unwrap_or_default()
        .saturating_mul(1000);
    FactId::mint(ms, random)
}

fn fact_from(
    input: &ConsolidationInput,
    index: usize,
    text: &str,
    from: Vec<Link>,
    replaces: Vec<FactId>,
) -> Option<Fact> {
    Some(Fact {
        id: id_for(input, index, text),
        text: FactText::parse(text).ok()?,
        recorded: input.now,
        by: Actor::System {
            part: SystemPart::Memory,
        },
        label: cited_label(input, &from)?,
        links: from,
        supersedes: replaces,
        valid: Validity::Unstated,
    })
}

fn hunk_from(input: &ConsolidationInput, index: usize, hunk: ModelHunk) -> Option<Hunk> {
    match hunk {
        ModelHunk::Promote { text, from } => {
            let fact = fact_from(input, index, &text, from, Vec::new())?;
            let to = almanac_service::fact::lands(&fact.label);
            Some(Hunk::Promote { fact, to })
        }
        ModelHunk::Supersede { old, text, from } => {
            let new = fact_from(input, index, &text, from, vec![old.clone()])?;
            Some(Hunk::Supersede { old, new })
        }
        ModelHunk::Flag { facts, note } => (!facts.is_empty()).then(|| Hunk::Flag {
            facts,
            note: UserText::new(note),
        }),
    }
}

/// The draft the model's answer says. Hunks that cannot be built are dropped one by one.
pub fn parse_draft(answer: &str, input: &ConsolidationInput) -> Result<Draft, ConsolidateError> {
    let json = object_in(answer).ok_or(ConsolidateError::Unparseable)?;
    let parsed: Answer = serde_json::from_str(json).map_err(|_| ConsolidateError::Unparseable)?;
    let hunks = parsed
        .hunks
        .into_iter()
        .take(MAX_HUNKS)
        .enumerate()
        .filter_map(|(index, value)| {
            let hunk: ModelHunk = serde_json::from_value(value).ok()?;
            hunk_from(input, index, hunk)
        })
        .collect();
    Ok(Draft { hunks })
}
