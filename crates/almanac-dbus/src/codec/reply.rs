//! A reply as the outputs of a bus member, and back.

use super::{Call, CodecError, json, parse};
use crate::error::MemoryError;
use almanac_core::{Count, MemoryReply};

fn one(outputs: &[String]) -> Result<&str, CodecError> {
    match outputs {
        [only] => Ok(only),
        _ => Err(CodecError::Shape),
    }
}

fn one_json<T: serde::de::DeserializeOwned>(outputs: &[String]) -> Result<T, CodecError> {
    parse(one(outputs)?)
}

/// The reply of `call` from the body's values (the `s` outputs, in order). A member that returns
/// nothing answers `Ok`; `Record` and `RecordBatch` answer `Ok` when their first output is empty
/// (admission kept nothing). A refusal never gets here: it is the bus error, which the caller
/// maps with `MemoryError::refusal`.
pub fn decode_reply(call: &Call, outputs: &[String]) -> Result<MemoryReply, CodecError> {
    use MemoryReply as R;
    match call.member {
        "Record" => match one(outputs)? {
            "" => Ok(R::Ok),
            event => parse(event).map(R::Recorded),
        },
        "RecordBatch" => match outputs {
            [first, count] if first.is_empty() && count == "0" => Ok(R::Ok),
            [first, count] => Ok(R::RecordedBatch(
                parse(first)?,
                Count(
                    count
                        .parse()
                        .map_err(|_| CodecError::Json(format!("count {count:?}")))?,
                ),
            )),
            _ => Err(CodecError::Shape),
        },
        "ExplainFile" | "Mark" | "Settle" | "RunConsolidation" | "Revert" | "SetRule"
        | "ApplyConsolidation" | "RemoveRule" | "Pause" | "Resume" | "Rebuild" => {
            outputs.is_empty().then_some(R::Ok).ok_or(CodecError::Shape)
        }
        "Search" | "Inject" => one_json(outputs).map(R::Hits),
        "Facts" => one_json(outputs).map(R::Facts),
        "Recent" => one_json(outputs).map(R::Recent),
        "Related" => one_json(outputs).map(R::Related),
        "Provenance" => one_json(outputs).map(R::Provenance),
        "Primer" => one(outputs).map(|text| R::Primer(text.to_owned())),
        "Propose" => match outputs {
            [id, state] => Ok(R::Proposed(parse(id)?, parse(state)?)),
            _ => Err(CodecError::Shape),
        },
        "Spaces" => one_json(outputs).map(R::Spaces),
        "Status" => one_json(outputs).map(R::Status),
        "Timeline" => one_json(outputs).map(R::Timeline),
        "PlanForget" => one_json(outputs).map(R::Plan),
        "Forget" => one_json(outputs).map(R::Forgot),
        "Pending" => one_json(outputs).map(R::Pending),
        "Consolidation" => one_json(outputs).map(R::Consolidation),
        "Rules" => one_json(outputs).map(R::Rules),
        "Verify" => one_json(outputs).map(R::Verified),
        "Export" => one_json(outputs).map(R::Exported),
        "Sweep" => one_json(outputs).map(R::Swept),
        other => Err(CodecError::Json(format!("no member {other}"))),
    }
}

fn invalid(e: CodecError) -> MemoryError {
    MemoryError::Invalid(e.to_string())
}

fn single<T: serde::Serialize>(value: &T) -> Result<Vec<String>, MemoryError> {
    json(value).map(|text| vec![text]).map_err(invalid)
}

/// The outputs the member `call` returns for `reply`: the daemon's half. A refusal is the bus
/// error of that name; a reply that is not the kind the member answers with is `Invalid`.
pub fn encode_reply(call: &Call, reply: &MemoryReply) -> Result<Vec<String>, MemoryError> {
    use MemoryReply as R;
    let nothing = Ok(Vec::new());
    match (call.member, reply) {
        (_, R::Refused(why)) => Err(MemoryError::from(why)),
        ("Record", R::Ok) => Ok(vec![String::new()]),
        ("Record", R::Recorded(event)) => single(event),
        ("RecordBatch", R::Ok) => Ok(vec![String::new(), "0".to_owned()]),
        ("RecordBatch", R::RecordedBatch(first, count)) => {
            Ok(vec![json(first).map_err(invalid)?, count.0.to_string()])
        }
        (
            "ExplainFile" | "Mark" | "Settle" | "Revert" | "SetRule" | "RemoveRule" | "Pause"
            | "Resume" | "Rebuild",
            R::Ok,
        ) => nothing,
        // The draft of a run is read with `Consolidation`; the member itself returns nothing.
        ("RunConsolidation" | "ApplyConsolidation", R::Ok | R::Consolidation(_)) => nothing,
        ("Search" | "Inject", R::Hits(v)) => single(v),
        ("Facts", R::Facts(v)) => single(v),
        ("Recent", R::Recent(v)) => single(v),
        ("Related", R::Related(v)) => single(v),
        ("Provenance", R::Provenance(v)) => single(v),
        ("Primer", R::Primer(text)) => Ok(vec![text.clone()]),
        ("Propose", R::Proposed(id, state)) => Ok(vec![
            json(id).map_err(invalid)?,
            json(state).map_err(invalid)?,
        ]),
        ("Spaces", R::Spaces(v)) => single(v),
        ("Status", R::Status(v)) => single(v),
        ("Timeline", R::Timeline(v)) => single(v),
        ("PlanForget", R::Plan(v)) => single(v),
        ("Forget", R::Forgot(v)) => single(v),
        ("Pending", R::Pending(v)) => single(v),
        ("Consolidation", R::Consolidation(v)) => single(v),
        ("Rules", R::Rules(v)) => single(v),
        ("Verify", R::Verified(v)) => single(v),
        ("Export", R::Exported(v)) => single(v),
        ("Sweep", R::Swept(v)) => single(v),
        (member, _) => Err(MemoryError::Invalid(format!(
            "{member} cannot answer with that reply"
        ))),
    }
}
