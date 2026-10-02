# Findings

Open items and standing facts. An entry names the condition that closes it. At the freeze
(porter 4623e18) there are **59 `todo!()` bodies** in library and daemon code (listed below, one
row per crate or file with the count); the tests contain none, and 16 contract tests are
`#[ignore]`d with the todo that blocks them.

## Stubs behind frozen interfaces

| Where | Count | Closes when |
| --- | --- | --- |
| almanac-seal `Oo7Keys::{get, create, destroy}` | 3 | fill wave 1 (D): oo7 0.6 is pinned and a dependency of the `oo7` feature; items with attributes `xdg:schema = org.quire.Memory.SpaceKey`, `space`; `Exists`/`Missing`/`Locked` per `KeyError` |
| eventlog `SqliteLog::{open, head, checkpoint, page, touching, scan, append, erase_bodies, prune_before}` | 9 | fill wave 1 (A): `SCHEMA_V1` is frozen and tested as SQL; `PRAGMA key`, `secure_delete=ON`, WAL, `wal_checkpoint(TRUNCATE)` after an erase; the `contract` test in `tests/pages.rs` must pass for `SqliteLog` (un-ignore `sqlite_and_memory_logs_agree`, `wrong_key_is_locked`) |
| memfiles `PlainDir::{list, read, write_atomic, remove}`, `SealedDir::{list, read, write_atomic, remove}` | 8 | fill wave 1 (B): `vault_contract` over `PlainDir` and `SealedDir` (un-ignore both tests); atomic = temporary file, fsync, rename; sealed files use `almanac_seal::seal` with `Aad::file(space, path)` |
| memfiles `Store::{topics, read, append, stage, pending, settle, derived_from, remove, write_primer}` | 9 | fill wave 1 (B): the rows of `store` in memory.md section 5 (`append_is_add_only`, `untrusted_fact_lands_in_pending`, `confirm_requires_witness`, `derived_from_is_transitive`); `settle(Keep)` declassifies with `prov::declassify`, itself a stub in porter until its fill |
| recall `Fts5::{create, upsert, remove, search, clear}` | 5 | fill wave 1 (C): `SCHEMA_V1` and `match_expression` are frozen; FTS5 is verified compiled in (`fts5_is_compiled_in_and_the_schema_and_expression_work`) |
| recall `ExactScan::{in_memory, upsert, remove, nearest, clear}` | 5 | fill wave 1 (C): BLOB encoding and `nearest_exact` are frozen and tested; no `unsafe`, no extension (QUESTIONS P5) |
| recall `Index::{rebuild, upsert, remove, search}` | 4 | fill wave 1 (C): un-ignore `rebuild_equals_incremental`, `search_degrades_to_lexical`, `remove_drops_from_both_indexes`; the state moves by `recall::step` |
| recall-fastembed `FastembedEmbedder::{load, embed}` | 2 | fill wave 1 (C), by hand with network (`cargo check -p recall-fastembed`; ort downloads binaries): the crate is excluded from the gate |
| almanac-watch `InotifyWatch::{new, watch, unwatch, next}` | 4 | fill wave 1 (D): notify 8.2, rename halves paired by cookie; un-ignore `inotify_sees_create_rename_delete_in_scratch_dir` (scratch directory only) |
| almanac-service `plan_forget` | 1 | fill wave 2: the closure of memory section 4.3 over `FactGraph` and a `LogRead`; `Plan::digest` and `plan_step` are built |
| almanac-service `check_draft` | 1 | fill wave 2, after porter fills `prov::Label::join` (the check cites the join of the sources' labels) |
| almanac-service `MemoryService::{handle, export}` | 2 | fill wave 2: authorise with `allowed`, route, run the machines, apply their effects; un-ignore the seven tests in `almanac-fake/tests/service.rs` and `in_process_end_to_end` |
| almanac-dbus codec `encode_request`, `decode_reply` | 2 | fill wave 3: one match from `MemoryRequest` to member and JSON arguments; a bus error name maps back to `Refusal` (`MemoryError::refusal`) |
| almanac-client `DbusTransport::call` | 1 | fill wave 3, with the codec |
| memoryd `InferdEmbedder::embed`, `InferdConsolidator::draft` | 2 | fill wave 3, blocked on porter's `DbusTransport::open` and session fills; the embedder's `DataClass` is the document's, `Usage::Background` for indexing |
| memoryd `SystemBackend::open_index` | 1 | fill wave 3: open `index.db` with `PRAGMA key` from `Purpose::Index`, create `recall::SCHEMA_V1` on a new file |
| memoryd `main` (serving the bus, systemd unit, Landlock) | 0 todo; skeleton | fill wave 3: serve the three interfaces, the unit with `PrivateNetwork=yes`, `ProtectSystem=strict`, `ReadWritePaths=` for the memory directories only, `ProtectHome=read-only` for watching |
| daemons serve their bus | | the items above |

Total: 3 + 9 + 8 + 9 + 5 + 5 + 4 + 2 + 4 + 1 + 1 + 2 + 2 + 1 + 2 + 1 = 59.

## Built at the freeze (pinned by tests)

almanac-core (ids and grammars, `admit`, `allowed`'s inputs, every wire and stored form round
trip, kind tags, `Dirs`), almanac-seal (derivation goldens, seal/unseal, `MemoryKeys`),
eventlog (header bytes golden, chain, `verify_chain`, `MemoryLog`, schema as SQL), memfiles
(topic file golden, trailer grammar, `VaultPath`, `MemoryVault`, `Primer`), recall (`fuse_rrf`,
`chunk`, `nearest_exact`, the vector BLOB, the index state machine, `FakeEmbedder`),
almanac-service (the authorisation matrix, the Space, fact, plan and consolidation machines,
`Plan::digest`, retention, the export writer and event lines, the config files, timeline rows),
almanac-watch (`join`), almanac-dbus (skeletons, proxies, introspection XML, `MemoryError`),
almanac-client (`Memory`, `Absent`, `InProcess`), almanac-fake, memoryd (`xdg`, `SystemClock`).

## The companion amendment (2026-10-03)

Interface changes made before any fill wave, from QUESTIONS "Persistent companion" and "One
message model", `research-persistent-agent.md` sections C, D and E, and `research-rig.md`
section 7 item 9. It builds against the amended porter (branch `m-amend-comp`: `prov::Message`,
`AgentRef`, `Address`, `AgentRole::Worker`). Types, traits, signatures, docs, pure tables and
tests; no new `todo!()` (the count and the list above are unchanged; three stub bodies,
`FastembedEmbedder::embed`, `InferdEmbedder::embed` and the `Index` methods, only have longer
doc text).

**Rig item 9 (recall).** `Embedder::embed(texts, role, urgency)` with `EmbedRole { Query,
Document }`; `EmbedderCard` gained `max_batch: MaxBatch` and `prompts: PromptPrefixes { query,
document }` (recall names no porter type, so memoryd maps them to porter-infer `EmbedRequest.role`
and porter-core `EmbedCap.{max_batch, prompts}`); `EmbedderCard::{prefixed, batch_sizes,
space_vs}` are pure and table-tested (a different prefix or model is a different vector space, a
different batch size is not); `EmbedError` gained `Busy` and `Failed { class, why }` with
`retry_class() -> RetryClass { Retry, Fatal }` (`Unavailable` and `Busy` retry; `Refused` and
`TooLong` are fatal). Only one side applies the prefix: inferd does, from `EmbedCap.prompts`;
`FastembedEmbedder` does it itself. The card carries the prefixes so the index can tell when it
went stale.

**Messages: an `EventBody::Message` variant, not a text field on `AreaPayload`.** The research
(G4) asked for both a searchable-text hook on `AreaPayload` and a `RecallOver` option. We chose
the typed variant, and `RecallOver::{Messages, Episodes}` beside the existing `Events`, because:
(1) the message is `prov`'s type and almanac may name it (an `AreaPayload` exists only to keep
other areas' types out of almanac); (2) with a typed body the searchable text and the label of
each part are pure functions of the body (`EventBody::index_texts`), so the index is rebuildable
from the log and the text cannot drift from the stored value; with an owner-supplied
`AreaPayload.text` the owner could index text its JSON does not contain, and the label of that
text would be unchecked; (3) cascade-forget gets the entities a message names
(`EventBody::thing_refs`) with no extra field; (4) `AreaPayload` stays frozen, so docket's and
cua's payloads are unaffected. The cost is a variant: `kind()` is `companion.message`, the
`wire.rs` request/reply samples and the auth matrix gained rows, and `EventBody` boxes both new
payloads (`Message`, `Episode`) to keep the enum small. `AreaPayload` has no searchable text; if
docket later wants its own payloads searchable it adds a field then.

**Episodes** (`episode.rs`): `Episode { id, agent: AgentRef, kind: EpisodeKind { Task, Side },
parent, space, started, ended, outcome: EpisodeOutcome, skeleton, narrative }`. `Skeleton`
(trusted by construction, no model) holds `asked: Vec<MessageText>`, typed `StepLine`s (action,
target things, effect, `StepOutcome`, undo handle), `touched` things (for cascade-forget) and
typed `ResultLine`s (count, thing or outcome ref; text only by reference). `Narrative` is the
model-written text with its own `Label` and the `ModelRole` that wrote it. The type is almanac's
(not docket's, as research C.2 first drew it) because almanac stores and recalls it and may name
only `prov` types; docket builds the skeleton and the idle pass writes the narrative. Events are
immutable, so a narrated episode is a second `Episode` event with the same id and skeleton
(`Episode::narrates`), and the service indexes only the newest event per id. Rollups stay plain
topic files under `journal/` (Q10): no `Hunk`, no layer.

**Recent and Inject.** `MemoryRequest::Recent(SpaceId, RecentQuery { since, kinds, trust, limit, bodies })`
answered with `MemoryReply::Recent(Vec<RecentEntry>)` (summary, effect, label, text, body) and
`MemoryRequest::Inject(InjectQuery { space, text, budget: Tokens, k, over, trust })` answered
with `Hits` cut by `fit_budget` (in ranked order, an item that does not fit is skipped; the
estimate is characters over 4 until the assembler measures the real tokenizer). Allowed for
`Router` and `ShellUi`; `ReadScope` gained `Inject` and `Recent`; D-Bus `Recall.Inject` and
`Recall.Recent` (28 methods became 30) and `Memory::{inject, recent}` in the client.

**Upstream asks (m-asks).** `RecentQuery.bodies: BodyMode { Without, Json }` and
`RecentEntry.body: Option<JsonText>` (None unless asked and present; the label always travels in
the entry); the D-Bus method takes the query as JSON, so the XML is unchanged. `almanac-client`'s
`almanac-service` dependency is behind the default-off feature `in_process` (`InProcess`).

**What changed that tests pin (and what was added, not changed).** `RuleSet::standard()` gained
`days("companion.*", 30)`; `is_audit_class` now includes `Message` and `Episode` bodies;
`wire.rs` appended `Inject`/`Recent` (request indexes 28, 29) and `Recent` (reply index 20), nothing
renumbered; the introspection test expects 30 methods and the XML gained the two members;
`recall`'s card-literal test gained the two fields and the embed calls gained the role.

**For the almanac fill (service) waves.** `Record` admission checks that `Record.label` is at
least as restrictive as the join of `index_texts()` labels and, for a message, that `Record.actor`
matches `from` (`Message::sender_matches`) and that `Message::check` passes; a skeleton with an
untrusted label is refused (`Invalid`). `Propose` into the `desktop` Space must call
`prov::desktop_admits` and refuse anything but `Admit` (Q9). The `things` rows come from
`thing_refs()`. A `Message` whose `to.space` differs from the Space it is recorded in is
recorded in the receiving Space with the sender's label (it never grants a read; taint travels).
`Inject` over `Both` merges facts and event documents by `fuse_rrf`, then `fit_budget`; the
`trust` filter applies to the document label, not the event's.

### Docket-bound list (for the docket freeze brief)

Everything `research-persistent-agent.md` section D assigns to docket, which does not exist yet.
Nothing below is built; the interfaces it will use are.

1. **Companion identity over tasks and Sessions** (G1). One identity (`AgentRef::Companion`),
   many tasks, each its own docket `Session` for taint, budget and `TaskPolicy`; the launcher
   returns to the front task; companiond owns the front pointer; `parent` on
   `SessionRecord::Opened`. `TaskId` now lives in `prov` (SPEC section 2 names docket-core).
2. **Persisting the user's turns** (G2). `SessionRecord::Asked` carries the turn and the target
   agent; the user's direct turn to a subagent is a `prov::Message` from `AgentRef::User` recorded
   verbatim as `Trusted` by the router (retention row `companion.*`, 30 days, in design/22).
3. **The working-set assembler** (G11). `assemble(&Budget, &Sources) -> PlannerView` in
   `agent-loop`: sections in the stable-first order of research C.3, budgets in
   `companion.budget.*`, masking of old outcomes, split-instead-of-summarise on overflow;
   `PlannerView` gains `primer`, `profile`, `roster`, `episodes`, `recalled`. It reads recent
   episodes with `Recent`, the recall section with `Inject` (`trust: TrustedOnly`), the active
   Space plus `desktop`, and logs each injection as `Memory.Read`.
4. **The roster** (G12). `Roster { entries }`, `RosterLine { agent: AgentRef, space, state,
   goal: Reveal<String>, last: StepLine, told: Option<first 80 chars of the user's turn> }`,
   derived by companiond from its tasks, `Cua1` signals and `Recent` messages; a line for another
   Space shows presence only, no goal text (Q8); exposed to sill's Runs filter.
5. **Side-episode capture** (C.6). When the person's side conversation with a worker or run ends
   (row closed, or 2 minutes idle) write an `Episode { kind: Side }` from the user's turns and
   the subagent's typed steps since; a goal change narrows the subagent's `TaskPolicy` through
   the `PolicyWriter` path (widening confirms, S4).
6. **Episode builder and idle pass** (G3, G6, G10). Deterministic skeleton at task end, recorded
   by the router (option (a): a docket `Intents1.Session.Note(episode)` member, no new
   `Caller`); the narrative at idle (`Usage::Background`, readerd when the task read untrusted
   content, `ModelRole::Consolidator`), as a second `Episode` event; fact candidates to `Propose`.
7. **The task-spawn action** (G13). `companion.task.start { goal, kind }` in the built-in
   provider manifest, `Effect::Read`, child `TaskPolicy` never wider than the parent's; records
   `Area{Companion}` `task.started` with `Cause::Event` of the spawning step.
8. **Message delivery** (new with the one message model). The router stamps `from` from the
   caller and checks `Message::sender_matches` and `Message::check`; mints `MessageId` and
   `ThreadId`; delivers a `Request` to the receiver as input evaluated under the receiver's own
   `TaskPolicy` and the gating pipeline (no authority); joins the message's label into the
   receiver's taint; a cross-Space message lands in the receiving Space's task as an inbound
   item and shows on the roster; a computer-use run's final result is a `Report` (no separate
   return type exists).
9. **Completion notifications** (C.5.5). A one-line event in the front thread's current task
   when a worker or run reports `Done`, `Failed` or `Cancelled`; the orb shows Waiting only when
   the result needs the person.
10. **Restart rebuild** (G16). companiond rebuilds the roster and the front task from
    `Area{Companion}`, `Area{Cua}`, message and episode events (via `Recent`) on start.
11. **Settings and rows** (D.2 G2, G11): `companion.budget.*`, the retention row for
    `companion.*`, and the Space-scope rule that desktop-scope reads join through
    `Confidentiality::join` so they do not widen a Space task.

## Open

- **Spec conflicts and their resolutions** (SPEC.md wins over memory.md; items below are where the two
  left room or disagreed):
  - `Caller` is `App | Router | Cuad | ShellUi` (SPEC 3.4); the `Companion` column of memory.md 3.9
    moved to `Router`. Reads (`Search`, `Facts`, `Related`, `Provenance`, `Primer`) and `Propose`
    and `PlanForget` are `Router` or `ShellUi`; `Cuad` may only `Record` `Area(Cua)` bodies by a
    `Cua` run actor. There is no `Caller::Companion`, so `Refusal::OutsideSpace` and the
    "companion cannot read another Space" test are the router's (docket's) check; memoryd keeps
    the refusal for requests that name a Space the caller class may not use.
  - The Space is part of every request that names one. memory.md 3.9 writes `Related(ThingRef)` and
    `Provenance(SpacePath)` without it although the D-Bus members take `space`; the enum carries it
    (`Related(SpaceId, ThingRef)`, `Provenance(SpaceId, SpacePath)`), and `RecallQuery`/`FactQuery`
    carry theirs as fields.
  - `Verdict` is `Settlement { Keep(prov::ConfirmReceipt), Discard }` everywhere
    (`MemoryRequest::Settle`, `Store::settle`); `Confirmation` is `prov::Witness`/`ConfirmReceipt`.
  - `Actor`, `Effect`, `Label`, `SpaceId`, `EntityId`, `RunId` and `SessionId` are porter's `prov`
    and `porter-core`; `AgentTier` is `prov::AgentRole`; `ThingRef/ThingKind/ThingKey` are aliases of
    `EntityId/EntityKind/EntityKey`; `ActionName` is `prov`'s and not re-declared.
  - `EventBody` has no `Action`, `Session`, `Cua`, `Policy` or `Consent`: other areas' payloads are
    `Area(AreaPayload)` (an `AreaTag`, the owner's `KindTag`, `JsonText`, and the `things` for
    cascade-forget). `UndoRef` stays on `TimelineEntry` (always `None` until docket supplies the token
    in a payload).
  - `RuleSet.defaults` is `Vec<KindRetention { kind, retention }>`, not `Vec<(KindPattern,
    Retention)>`: tuples of mixed types do not survive `memory.toml` readably.
  - `ChainReport`, `Break`, `Head`, `Checkpoint` and `Link32`/`Digest32` live in `almanac-core`
    (not `eventlog`) because `MemoryReply::Verified` and `SpaceStatus` carry them; `eventlog` uses
    them. `SpaceStatus.index` is the wire form `IndexView`, mapped from `recall::IndexState`
    (recall is generic and cannot name almanac-core); likewise `RecallWhy` vs `recall::HitWhy`.
  - `seal` returns `Result<Vec<u8>, SealError>` (a new `TooLarge` variant) where memory.md writes
    `Vec<u8>`: the cipher can refuse a huge plaintext and `expect` is not allowed.
    `KeyError` gained `Exists`; `LogError` gained `BadDigest` and `NoSuchEntry`; `LogRead` gained
    `checkpoint()` (verification needs where the retained entries start).
  - `NewHeader` carries the body digest (`NewHeader::of(&Record, recorded, &digest_key)`) so a
    header-only entry keeps the digest of the body it never stored.
  - The topic file's trailer carries the actor, label, links as JSON values (percent-escaped for
    `%`, `;` and `>`) rather than memory.md's abbreviated `by: user/org.quire.Mail; trust: user`
    sketch: a `Fact` must round-trip losslessly, and `Label` and `Actor` have many forms. The keys
    `valid` and `origin` are refused, not interpreted (reserved).
  - D-Bus: `Refusal::{OutsideSpace, NoSuchFact, NotPending}` also have error names
    (`org.quire.Memory1.Error.*`), so the mapping to `Refusal` is 1:1 (memory.md listed seven).
    `Export` is `(s options, h out) -> s manifest`; `RecordBatch` and `Propose` have two out
    arguments.
  - `memoryd` does not link `almanac-client` (it has no use for it); its porter edges are
    `porter-core`, `porter-infer` and `porter-client` (`InferdEmbedder` and `InferdConsolidator`).
  - `sqlite-vec` is not a dependency: exact scan is the default (QUESTIONS P5, no `unsafe`);
    file capture is inotify on Space roots (P4); fanotify is later.
- **Answered questions this freeze applies** (QUESTIONS Rec column): Me1 sealed per file by default
  with a per-Space `plain` choice (`VaultKind`); Me2 strict cascade (the closure includes every
  fact linked to a forgotten thing); Me3 headers outlive a forget (bodies are erased, the chain
  commits to a keyed digest); Me4 consolidation auto-applies Tidy, Stamp, Supersede and trusted
  Promote hunks with pre-images kept (`RunEffect::ApplyHunks`); Me5 the retention table and the
  14-day pending TTL (`RuleSet::standard`, `PENDING_TTL_DAYS`); Me6 apps do not query memory in v1
  (the matrix); Me7 plain tar export, the digest subkey only when ticked (`VerificationKey`);
  Me8 not memory's (the cua area).
- **Physical erasure** on copy-on-write storage is only guaranteed by destroying a Space's key;
  per-item forget is logical deletion plus SQLite `secure_delete`. The UI states this limit
  (memory.md conflict 2).
- **porter's `prov::Label` constructors and `Label::join` are stubs** until porter's fill, so
  almanac's tests build labels from their fields and `check_draft` is a stub with them.
- **`recall-fastembed` is excluded from clippy and test** (ort downloads binaries at build
  time): check it by hand with network, `cargo check -p recall-fastembed`. It is in the lockfile,
  so `cargo deny check licenses` covers its tree.
- **Cross-repo dependencies are sibling paths** (`../porter/crates/{porter-core, prov, porter-infer,
  porter-client}`), as porter does for stoker; porter itself path-patches `../stoker`. Pinned git
  revs replace them in fill wave 1, stage by stage (quire `CONSUMING.md` section 1).
