//! A request as a bus call, and back.

use super::{Args, Call, CallArg, CodecError, Iface, json};
use almanac_core::{MemoryRequest, Record, SpaceId, UnixSeconds};

fn call(interface: Iface, member: &'static str, args: Vec<CallArg>) -> Call {
    Call {
        interface,
        member,
        args,
    }
}

fn space(id: &SpaceId) -> CallArg {
    CallArg::Text(id.to_string())
}

fn body<T: serde::Serialize>(value: &T) -> Result<CallArg, CodecError> {
    json(value).map(CallArg::Text)
}

fn space_and<T: serde::Serialize>(id: &SpaceId, value: &T) -> Result<Vec<CallArg>, CodecError> {
    Ok(vec![space(id), body(value)?])
}

/// The call that carries `request`.
pub fn encode_request(request: &MemoryRequest) -> Result<Call, CodecError> {
    use Iface::{Control, Recall, Record as Rec};
    use MemoryRequest as R;
    Ok(match request {
        R::Record(r) => call(Rec, "Record", space_and(&r.space, r)?),
        R::RecordBatch(rs) => {
            let first = rs.first().map_or_else(String::new, |r| r.space.to_string());
            call(Rec, "RecordBatch", vec![CallArg::Text(first), body(rs)?])
        }
        R::ExplainFile(c) => call(Rec, "ExplainFile", vec![body(c)?]),
        R::Mark(m) => call(Rec, "Mark", vec![body(m)?]),
        R::Search(q) => call(Recall, "Search", space_and(&q.space, q)?),
        R::Facts(q) => call(Recall, "Facts", space_and(&q.space, q)?),
        R::Inject(q) => call(Recall, "Inject", space_and(&q.space, q)?),
        R::Recent(s, q) => call(Recall, "Recent", space_and(s, q)?),
        R::Related(s, t) => call(Recall, "Related", space_and(s, t)?),
        R::Provenance(s, p) => call(Recall, "Provenance", space_and(s, p)?),
        R::Primer(s) => call(Recall, "Primer", vec![space(s)]),
        R::Propose(s, d) => call(Recall, "Propose", space_and(s, d)?),
        R::Spaces => call(Control, "Spaces", vec![]),
        R::Status(s) => call(Control, "Status", vec![space(s)]),
        R::Timeline(s, q) => call(Control, "Timeline", space_and(s, q)?),
        R::PlanForget(s, scope) => call(Control, "PlanForget", space_and(s, scope)?),
        R::Forget(token) => call(Control, "Forget", vec![body(token)?]),
        R::Pending(s) => call(Control, "Pending", vec![space(s)]),
        R::Settle(fact, verdict) => call(Control, "Settle", vec![body(fact)?, body(verdict)?]),
        R::Consolidation(s) => call(Control, "Consolidation", vec![space(s)]),
        R::RunConsolidation(s) => call(Control, "RunConsolidation", vec![space(s)]),
        R::Revert(run) => call(Control, "Revert", vec![body(run)?]),
        R::ApplyConsolidation(run) => call(Control, "ApplyConsolidation", vec![body(run)?]),
        R::DiscardConsolidation(run) => call(Control, "DiscardConsolidation", vec![body(run)?]),
        R::Rules => call(Control, "Rules", vec![]),
        R::SetRule(rule) => call(Control, "SetRule", vec![body(rule)?]),
        R::RemoveRule(id) => call(Control, "RemoveRule", vec![body(id)?]),
        R::Pause(s, until) => {
            let until = u64::try_from(until.0)
                .map_err(|_| CodecError::Json("a time before the epoch".into()))?;
            call(Control, "Pause", vec![space(s), CallArg::Seconds(until)])
        }
        R::Resume(s) => call(Control, "Resume", vec![space(s)]),
        R::Verify(s) => call(Control, "Verify", vec![space(s)]),
        R::Rebuild(s) => call(Control, "Rebuild", vec![space(s)]),
        R::Export(options) => call(Control, "Export", vec![body(options)?]),
        R::Sweep(s) => call(Control, "Sweep", vec![space(s)]),
    })
}

fn parse_space(text: &str) -> Result<SpaceId, CodecError> {
    SpaceId::parse(text).map_err(|e| CodecError::Json(format!("space {text:?}: {e}")))
}

/// The Space argument and the body that follows it; a body that names its own Space must name
/// the argument's.
fn space_then<T: serde::de::DeserializeOwned>(
    args: &mut Args<'_>,
    own: impl Fn(&T) -> Option<&SpaceId>,
) -> Result<(SpaceId, T), CodecError> {
    let id = parse_space(args.text()?)?;
    let value: T = args.json()?;
    match own(&value) {
        Some(named) if named != &id => Err(CodecError::Json(format!(
            "the space argument {id} differs from the body's {named}"
        ))),
        _ => Ok((id, value)),
    }
}

fn plain<T: serde::de::DeserializeOwned>(args: &mut Args<'_>) -> Result<(SpaceId, T), CodecError> {
    space_then(args, |_| None)
}

fn only_space(mut args: Args<'_>) -> Result<SpaceId, CodecError> {
    let id = parse_space(args.text()?)?;
    args.end()?;
    Ok(id)
}

fn space_body<T: serde::de::DeserializeOwned>(
    mut args: Args<'_>,
) -> Result<(SpaceId, T), CodecError> {
    let pair = plain(&mut args)?;
    args.end()?;
    Ok(pair)
}

fn one_body<T: serde::de::DeserializeOwned>(mut args: Args<'_>) -> Result<T, CodecError> {
    let value = args.json()?;
    args.end()?;
    Ok(value)
}

/// The request a call carries: the daemon's half of the table. An unknown member, a wrong
/// argument count or kind, a body that is not the member's type, or a Space argument that
/// disagrees with the body's own are errors.
pub fn decode_request(call: &Call) -> Result<MemoryRequest, CodecError> {
    use Iface::{Control, Recall, Record as Rec};
    use MemoryRequest as R;
    let mut args = Args::of(call);
    match (call.interface, call.member) {
        (Rec, "Record") => {
            let (_, record) = space_then(&mut args, |r: &Record| Some(&r.space))?;
            args.end()?;
            Ok(R::Record(record))
        }
        (Rec, "RecordBatch") => {
            let named = args.text()?;
            let records: Vec<Record> = args.json()?;
            args.end()?;
            match records.first() {
                Some(first) if first.space.as_str() != named => Err(CodecError::Json(format!(
                    "the space argument {named:?} differs from the first record's {}",
                    first.space
                ))),
                _ => Ok(R::RecordBatch(records)),
            }
        }
        (Rec, "ExplainFile") => one_body(args).map(R::ExplainFile),
        (Rec, "Mark") => one_body(args).map(R::Mark),
        (Recall, "Search") => {
            let (_, q) = space_then(&mut args, |q: &almanac_core::RecallQuery| Some(&q.space))?;
            args.end()?;
            Ok(R::Search(q))
        }
        (Recall, "Facts") => {
            let (_, q) = space_then(&mut args, |q: &almanac_core::FactQuery| Some(&q.space))?;
            args.end()?;
            Ok(R::Facts(q))
        }
        (Recall, "Inject") => {
            let (_, q) = space_then(&mut args, |q: &almanac_core::InjectQuery| Some(&q.space))?;
            args.end()?;
            Ok(R::Inject(q))
        }
        (Recall, "Recent") => space_body(args).map(|(s, q)| R::Recent(s, q)),
        (Recall, "Related") => space_body(args).map(|(s, t)| R::Related(s, t)),
        (Recall, "Provenance") => space_body(args).map(|(s, p)| R::Provenance(s, p)),
        (Recall, "Primer") => only_space(args).map(R::Primer),
        (Recall, "Propose") => space_body(args).map(|(s, d)| R::Propose(s, d)),
        (Control, "Spaces") => args.end().map(|()| R::Spaces),
        (Control, "Status") => only_space(args).map(R::Status),
        (Control, "Timeline") => space_body(args).map(|(s, q)| R::Timeline(s, q)),
        (Control, "PlanForget") => space_body(args).map(|(s, scope)| R::PlanForget(s, scope)),
        (Control, "Forget") => one_body(args).map(R::Forget),
        (Control, "Pending") => only_space(args).map(R::Pending),
        (Control, "Settle") => {
            let fact = args.json()?;
            let verdict = args.json()?;
            args.end()?;
            Ok(R::Settle(fact, verdict))
        }
        (Control, "Consolidation") => only_space(args).map(R::Consolidation),
        (Control, "RunConsolidation") => only_space(args).map(R::RunConsolidation),
        (Control, "Revert") => one_body(args).map(R::Revert),
        (Control, "ApplyConsolidation") => one_body(args).map(R::ApplyConsolidation),
        (Control, "DiscardConsolidation") => one_body(args).map(R::DiscardConsolidation),
        (Control, "Rules") => args.end().map(|()| R::Rules),
        (Control, "SetRule") => one_body(args).map(R::SetRule),
        (Control, "RemoveRule") => one_body(args).map(R::RemoveRule),
        (Control, "Pause") => {
            let id = parse_space(args.text()?)?;
            let until = i64::try_from(args.seconds()?)
                .map_err(|_| CodecError::Json("a time past the range".into()))?;
            args.end()?;
            Ok(R::Pause(id, UnixSeconds(until)))
        }
        (Control, "Resume") => only_space(args).map(R::Resume),
        (Control, "Verify") => only_space(args).map(R::Verify),
        (Control, "Rebuild") => only_space(args).map(R::Rebuild),
        (Control, "Export") => one_body(args).map(R::Export),
        (Control, "Sweep") => only_space(args).map(R::Sweep),
        (_, member) => Err(CodecError::Json(format!("no member {member}"))),
    }
}
