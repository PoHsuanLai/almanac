# Findings

Open items and standing facts. An entry names the condition that closes it. At the freeze
(porter 4623e18) there were 59 `todo!()` bodies; fill waves 1 and 2 (eventlog, memfiles, recall, seal,
watch, almanac-service) removed 53 and the bus and daemon wave (F2 memoryd, below) removed the last
6, so **0 remain**. No contract test is `#[ignore]`d except the by-hand fastembed one.

## Stubs behind frozen interfaces

None. What still waits on other repos is under "Open" below (porter's transports, docket, cua).

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

## Service fill (wave F2, almanac-service)

Decisions the fill made where the frozen text left room:

- **Spaces are provisioned on first use** (key created if `Missing`, sealed vault, replica derived
  from the Space's digest subkey, so two machines differ). `MemoryService::register(meta)` tells it
  about `spaces.toml` entries; `metas()` and `rules()` let memoryd persist what the service made or
  changed. A locked key buffers `Record`s (at most `BUFFER_LIMIT`, flushed when the Space opens);
  every other request answers `SpaceLocked`.
- **A Space is checked out for the length of a request** (Index writes hold `&mut` across the
  embedding and `almanac-service` may not link tokio); a second request for the same Space meanwhile
  is `Busy`. memoryd serialises per Space or retries.
- **Search runs the index halves itself** (`Index::lexical`, `VectorIndex::nearest`, `fuse_rrf`):
  `Index::search` holds a shared borrow of SQLite connections across the query embedding, which makes
  the future `!Send`. The facet filter (`over`, trust) is the `kind`/`trust` columns of `docs`.
- **Fact ids and run ids** take their random bits from a keyed hash of the Space's digest subkey, a
  counter and the time (CONVENTIONS 4: nothing ambient).
- **Proposals**: the shell's are the person's own words (Trusted); the router's are planner text
  (Untrusted, so they wait in `pending/`); both are joined with the labels of what they cite. `desktop`
  refuses anything `desktop_admits` does not `Admit` (`Invalid`).
- **A `Message` is recorded in the receiving Space** (`to.space`); `Invalid` for a forged sender, a
  malformed message, an untrusted skeleton or a label less restrictive than its documents.
- **Closed by the memoryd wave** (was "not done by the service yet"): Tidy, Stamp, ExternalEdit
  and Flag hunks, the retention sweep and its trigger, `AnchorFinalHead`, persisted marks and
  `FactView.used`; and the eventlog `things` rows now come from `thing_refs()`. See the next
  section for what each means.
- **ContentDigest** is plain unkeyed `blake3` of the file's bytes (documented on the type and on
  `FileWhyClaim.content`).
- `FakeEmbedder::with_max_batch` sets the card's `max_batch`.

## Bus and daemon fill (wave F2, memoryd)

**The bus.** `almanac-dbus` has one member table, both ways: `encode_request`/`decode_request` and
`decode_reply`/`encode_reply` (`codec/`), `invoke` (the caller's half, over the proxies) and the
served objects (`serve.rs`, the daemon's half, over a `Serve` handler). The frozen unit-struct
skeletons stay as the introspection source; `tests/served.rs` pins that the objects memoryd
serves introspect the same, and both equal `dbus/org.quire.Memory1.xml`. The XML gained `Control.Sweep`
(30 methods became 31). Conventions the codec fixes: a Space argument is its plain id and every other
typed argument is JSON; a Space argument that differs from the Space inside the body is `Invalid`;
`Primer` answers the markdown, `RecordBatch`'s count is a decimal, and a `Record` or `RecordBatch`
that admission dropped answers an empty first output (so the client sees `Recorded::NoMemory`);
`RunConsolidation` answers nothing (the draft is read with `Consolidation`); a refusal is the bus
error of its name, one to one. `DbusTransport::call` maps a missing daemon
(`ServiceUnknown`, `NameHasNoOwner`) to `Absent`; `Export` needs a stream, so it has
`DbusTransport::export(options, fd)` and `call(Export)` is a `Bus` error.

**Who is calling.** `Peers` turns the sender's unique name into a `Caller`. `ProcPeers` asks the bus
for the connection's pid, reads `/proc/<pid>/exe` and looks it up in
`<config>/quire/memory-callers.toml` (`router`, `shell`, `cuad` as lists of executables, `[apps]`
mapping an `AppName` to its executables; the fixed roles win over an app entry). A missing or
unreadable file allows nobody. This is advisory for unsandboxed processes (porter R12); a sandbox
identity (Flatpak, a cgroup scope) would be another `Peers`. `TablePeers` is the test seam.

**One queue per Space.** `Serialised` wraps the service: a request that names Spaces holds their
queues in id order (a message holds the receiver's), one that cannot name them (`Forget`, `Settle`,
`Revert`, `Export`, the rules, the timer's sweep) holds all of them, `Spaces` and `Rules` hold none.
`Busy` never reaches a client; the cost is that `RunConsolidation` (a model call) holds its Space's
queue for as long as it runs.

**memoryd's seams.** `SystemBackend<K, E, C>` takes any key store, embedder and consolidator (the
defaults are the real ones), so a test keeps SQLCipher and the sealed vault real and swaps the rest.
`open_index` opens `index.db` with `PRAGMA key` from `Purpose::Index`, creates `recall::SCHEMA_V1`
and sets `user_version` on a new file, and gives FTS5 and the vectors a connection each.
`Backend::random` (OS randomness, mixed into every id the service mints) and
`Backend::remove_space` (the Space's directory, its index directory and its edit copies) are the
two new seam methods. `SqliteLog::open_for` takes the digest subkey, so `append` refuses a body that
does not match its header (`BadDigest`); `open` (no Space, no key) does not check.

**The models.** `InferdEmbedder` asks for `Need::Embeddings` of the card's length with
`Usage::Background` for indexing, maps `EmbedRole` and `Urgency`, and treats a reply of the wrong
count or length as fatal (another vector space). It carries one `DataClass` (default `Mail`, the
on-device floor), because `Embedder::embed` has no per-document class. `InferdConsolidator` sends
`Task::Extract` in the background with the most sensitive class among the labels it reads; the
model answers `{"hunks": [...]}` of `promote`, `supersede` and `flag` that cite links, and memoryd
builds the facts (label = the join of what they cite, `almanac_service::cited_label`, which
`check_draft` also uses; id from a hash of the run and the text; date from the run). A hunk that
cites nothing in the input is dropped, an answer that is not the object is `Unparseable`.
`ConsolidationInput` gained `now`.

**Hunks the service applies.** Tidy replaces a topic file by its reworded form only if the file is
still the `before` the draft saw, the topic is the same and every fact keeps its id, author, label,
links and date (only its text may change); anything else is not applied. Stamp turns a bullet the
person wrote into a fact (author the shell's user, label trusted); a run adds a Stamp for every
unstamped bullet by itself. ExternalEdit indexes the file again (the person's edit stands; facts
the old text had that are gone leave the index). Flag keeps the model's note in `flags/<run>.json`
and changes no fact. Every file a run rewrote is kept as a pre-image and `Revert` puts it back
before it re-indexes.

**Sweep.** `MemoryRequest::Sweep(space)` (the shell's) and memoryd's timer (one minute after start,
then daily) erase the bodies whose retention (the rule that would admit them now, else the kind's
default) has run out and drop their index documents; `WhileSourceExists` counts a thing or file as
gone once a later event (or the event itself) says deleted. Then the longest prefix of entries whose
bodies are gone and whose headers are a year old is pruned behind a checkpoint
(`memory.checkpoint` in the audit).

**Space deletion.** After the plan applies, the final head goes into the `desktop` Space's log as
`MemoryOp::SpaceDeleted { space, head }` (best effort: a locked desktop log does not stop the
person's deletion), the key is destroyed, the stores are closed and `remove_space` removes the
directories.

**Marks** are in the Space's vault (`meta/marks.json`, sealed like every file there).
**`FactView.used` and `last_used`** are counted from the router's `Memory.Read` audit entries that
list the fact; they last as long as those bodies (90 days).

**memoryd main** serves the session bus with `ProcPeers`, persists `spaces.toml` and `memory.toml`
after each request that changed them, emits `Recorded`, `Forgotten`, `ConsolidationReady`,
`PendingChanged` (when a proposal lands pending) and `StatusChanged` (after pause, resume and
rebuild), and runs on the multi-thread runtime (an export writes to the caller's stream blocking).
`dbus/memoryd.service` and `dbus/org.quire.Memory1.service` are the user unit (`PrivateNetwork`,
`ProtectSystem=strict`, `ProtectHome=read-only`, `ReadWritePaths` for the memory directories only)
and the activation file. (Landlock came with the w4 fill, below.)

**The inferd link is `AnyTransport::Dbus(DbusTransport::over(connection))`** (`memoryd::inferd_link`,
asks 71 and 80 closed). `main` builds the session-bus connection first and gives the same
transport to the embedder and the consolidator; porter-client is built with its `dbus` feature.
Nothing is called at start: inferd is found, and started by activation, at the first `open`, so a
daemon that starts before inferd or without it still starts. While inferd is unreachable `open`
answers `Unreachable`: the embedder answers `Unavailable` (recall is lexical-only and the index
retries), and a consolidation run is refused as `Busy` and retried the next night. Both halves are
tested on a private bus (`memoryd/tests/inferd_bus.rs`): a fake `org.quire.Inference1` up, none, and
one that appears after the first failure. `NoInference` and `NoSession` are gone. `default_card()` is
the card of the default embedding model (768 numbers, nomic prefixes) and must match what inferd
serves.

**Embedding by data class (ask 70/80).** `recall::Doc.class` is a `ClassTag` (the data-class slug;
recall names no porter type; empty means "no class"). `almanac-service` sets it from the strictest
class of the document's label (`class_of`, which now also ranks `Prompt` after `Voice`; the tag is
`class_tag`/`class_from_tag`). `recall::Embedder::embed_classed` (defaulted to `embed`) receives
each batch with its texts' classes, and `InferdEmbedder` partitions the batch by class, opens one
session per class for that call (sessions are not kept), and returns the vectors in input order.
Strictest-class pinning: one index is one model, so a class can only decide whether the model may
receive the text, not which model embeds it. The card's model must satisfy the strictest class the
Space holds (on this computer, in practice); a cloud model would have its `Mail` and `Voice`
documents refused (`Refused`, Fatal). Queries and untagged or unknown-tag texts go as the
embedder's pin (`Mail` for `InferdEmbedder::new`). A refused class fails the whole batch at the
embedder; `recall::Index` retries it class by class and only that class's documents go without a
vector (w4 fill).

**Terminals (ask 91).** `ActorKind::Cli` is audit class (`is_audit_class`: a terminal cannot tell the
person from an agent typing in it, so its acts are audited like the companion's and a pause or a
`Never` rule keeps the header). It is no app: `app_of` (admission) and `involves_app` (forget) name
it explicitly and match nothing by actor, so an app rule or an app forget reaches a terminal's
events only through their things; `Forget` by Space or Kind reaches them. `ActorFilter::Terminal`
(slug `terminal`) is the timeline's own bucket for it (`You` is the person only; `Mcp` got its own bucket in the w4 fill).

**Interface ask 26 (cua `HandedBack.user_events`): decided, and done on this side.** Option taken:
`cua-bus` depends on `almanac-core` and carries `almanac_core::EventRef`; no memory call at record
time. Why: `almanac-core` is already pure (its tree is `porter-core`, `prov`, serde, thiserror),
cua is above almanac in the repo order and cuad already depends on it, so the edge costs cua-bus
nothing it does not already link; a memory call at record time would need `Caller::Cuad` to read
events, which the authorisation matrix refuses on purpose (cuad handles untrusted screen text).
`scripts/check-boundary.sh` now also forbids cua's own effects list (`hyper-util rustls pipewire
wayland-* reis atspi rmcp cedar-policy`) from `almanac-core`, so the edge cannot break cua's
boundary later. What cua does (not in this repo): add `almanac-core` to cua-bus's `Cargo.toml` and to
its row in `check-boundary.sh`, and give `HandedBack` `user_events: Vec<EventRef>`. The router, not
cuad, reads the person's events during the takeover (`Recent`, a `Router` call) and hands them over.

### Closed by the w4 fill (branch w4-almanac; asks 73-77, 93, 94)

- **A restart adopts the index** (73). `recall::Index::sync(docs, embedder)` is what the service
  runs when a Space opens (it replaced the rebuild). `index.db`'s `meta` now records the embedder's
  vector space (`meta.space`, `EmbedderCard::space_key()`: model, length, metric and both prefixes;
  an older file without the column gets it when the card is first recorded). A file whose recorded
  space is the embedder's is adopted: documents with the same text and facets keep their vectors,
  new, changed and vectorless ones (an embedder that was away, a refused class) are embedded, and
  documents the truth no longer has are removed from both halves. No recorded space, or another
  one, is the full `rebuild`. `rebuild` and a first `upsert` record the space. An embedder that
  fails at open no longer stops the Space opening (the index state says why; the next start
  retries). Additions: `VectorIndex::ids()` (a trait method; `ExactScan` is its only
  implementation), `Fts5::{recorded_space, record_space, stored}`, `StoredDoc`,
  `FakeEmbedder::with_document_prefix`. Tests: `recall/tests/adopt.rs` (restart embeds nothing,
  only the changed, a changed facet, another model or prefix, an old file, vectorless documents,
  a text-less document) and `memoryd/tests/restart.rs` (a counting embedder over the real SQLCipher
  index across a restart, a deleted cache, a changed model).
- **A refused class fails only its own documents** (93). `recall::Index` retries a batch the
  embedder refused (`EmbedError::Refused`) class by class (`Doc.class`); a class that refuses again
  leaves its documents without a vector (lexical only, retried at the next start) and the call
  succeeds. When every document was refused the state is `LexicalOnly(EmbedderRefused)`; other
  failures still fail the batch. The trait is unchanged: `InferdEmbedder` still answers `Refused`
  for the batch, and the index does the isolation.
- **`ActorFilter::Mcp`** (94), slug `mcp`, the timeline's own bucket for `Actor::Mcp` (an
  outside agent over MCP). A wire change: the enum has one more variant.
- **Topic text for the consolidator, Tidy proposals and ExternalEdit** (74).
  `ConsolidationInput.topics: Vec<InputTopic { topic, text }>` holds every topic file as the
  service would write it. The prompt lists them and asks for `{"kind":"tidy","topic","after"}`;
  `parse_draft` turns one into `Hunk::Tidy` with the text the model was shown as `before`, and
  drops it when the topic was not shown, the file is unchanged or `almanac_service::
  tidy_is_acceptable` refuses it (a tidy keeps every fact's id, author, label, links and date).
  The ExternalEdit baseline is `meta/baseline.json` in the Space's vault (sealed): each topic
  file's text as the service last left it. Before a request that may write topic files (`Propose`,
  `Settle`, `Revert`, `Forget`, `RunConsolidation`; also when a Space opens) the service compares
  the files with it (`guard_topics`), and after it refreshes the baseline except for topics that
  already differed (`accept_topics`), so the service's own writes are never edits and its write
  after the person's edit does not absorb it. A run adds one `ExternalEdit { before: baseline,
  after: file }` per differing topic (beside the Stamp hunks it already adds), and applying it
  makes the file the baseline. A topic file the person deleted drops its facts from the index and
  the baseline. Forgetting a fact removes it from the baseline too (a pending edit's `before`
  must not keep forgotten text).
- **Flags are readable** (75): `FactView.flagged: Vec<FlagNote { run, note }>` (`#[serde(default)]`,
  oldest run first), read from `flags/<run>.json`. Forgetting a fact removes its id from every
  note and deletes a note about nothing left (the note may quote the fact).
- **`StatusChanged` and `PendingChanged` the service raises** (76). `MemoryService::take_events()`
  returns `ServiceEvent::{PendingChanged, StatusChanged, Locked}` raised since the last call:
  `PendingChanged` after `Settle` and after pending facts age out (14 days; ageing now also runs in
  the daily `Sweep`, so it happens without anyone reading `Pending`); `Locked` when a Space is found
  without its key, once until it opens again (`StatusChanged` when it does). `MemoryService::
  check_keys()` is the key check's work (the minute timer then, the lock signal since the f4 fill): an open Space whose key the store no longer gives is
  closed and announced, a locked one whose key is back is opened (which flushes its buffered
  records). memoryd's `Daemon` drains the events after every request and after its timers
  (`flush_events`, `check_keys`, `sweep_all`) and sends the signals; a locked Space's
  `StatusChanged` carries `locked_status()`. The Secret Service's own lock signal would be a
  better trigger than polling (built in the f4 fill).
- **Landlock** (77): `memoryd/src/sandbox.rs`. The policy (`policy_for(dirs, bus_address)`, pure,
  a table test) is read-write on the four memory directories (data, cache, `<config>/quire`,
  `<runtime>/quire/memory`), read-only on `/usr /lib /lib64 /bin /etc /proc /sys /dev/urandom`,
  `ResolveUnix` on the session bus socket only (`unix:path=` of `DBUS_SESSION_BUS_ADDRESS`, else
  `<runtime>/bus`), and no TCP. `main` is no longer `#[tokio::main]`: it makes the directories,
  applies the policy (best effort: older kernels get what they have, none gets a warning on
  standard error and the unit file's restrictions) and only then builds the runtime, because
  Landlock restricts the calling thread and the threads made after it. The `landlock` crate does
  the syscalls, so there is no `unsafe` here. SQLite no longer opens temporary files
  (`temp_store = MEMORY`, index and event log). Tested on a scratch tree by a thread that applies
  the policy (real enforcement, `Full` on this kernel: writes outside denied, another unix socket
  and TCP denied, the bus socket allowed; a kernel without Landlock skips), and the binary was run
  once under it against a private pathname bus. Dropping the introspection-only skeleton structs
  (the optional half of ask 77) is not done: the served objects equal them by test and the XML is
  generated from them.

### Closed by the f4 fill (branch f4-almanac; ask 115 and docket's `Spaces` ask)

- **The key check listens** (115): `memoryd/src/keyring.rs`. `LockChanges::on(connection)` matches
  `org.freedesktop.DBus.Properties.PropertiesChanged` under `/org/freedesktop/secrets`;
  `is_lock_change` (pure) keeps only a `Locked` change of `org.freedesktop.Secret.Collection`.
  `Daemon::follow_keyring` calls `check_keys` per lock change; `main` subscribes before it serves
  and the minute timer is gone. The match is by path, not sender, so another bus client can cause
  one needless (idempotent) check. Tested over a private bus with a fake Secret Service
  (`memoryd/tests/keyring.rs`: lock then unlock close and reopen the Space and each says
  `StatusChanged`; an unrelated property change is not a lock change).
- **A changed index state is announced** (115): an open Space remembers the index state the bus last
  heard (what it opened with); when a request leaves it different (the embedder went away or came
  back, a rebuild finished) the service raises `StatusChanged` once. A `Rebuild` request also has its
  own follow-up signal, so a rebuild that changes the state says it twice (harmless).
- **`Spaces` is the router's as well** (docket's ask): `allowed` gives `R::Spaces` to `Router` and
  `ShellUi`; the caller matrix has its own row.
- Checked and already closed by the w4 fill: asks 73, 74, 75, 76, 91 (Cli/Terminal bucket), 93, 94
  (Mcp bucket).

### Closed by the f4-almanac-2 lane (docket's acceptance asks 2 and 3)

- **`desktop` is always there** (docket FINDINGS "The f4-e2e acceptance" 2; SPEC 3.1, QUESTIONS P6):
  the service never refused an unregistered Space (a Space is provisioned on first use), so a
  record for `desktop` already worked; what was missing was the guarantee. `PlanForget(desktop,
  Space)` is now refused (`Invalid`, "the desktop Space cannot be deleted"): it holds memory outside
  every Space and the final head of each deleted Space. Tests: `memoryd/tests/desktop.rs`.
- **File keys for the packaged binary, TEST ONLY** (docket ask 3): feature `test-keys` (memoryd,
  and almanac-seal's `FileKeys`), off by default and never in a release or dist build.
  `MEMORYD_KEYS=file:<path>` selects a sealed file of Space keys; the daemon says so on standard
  error and adds the file's directory to its Landlock policy. The wrapping key is a constant in
  the source: the file keeps keys from sitting as plain text and protects nothing else, so it is
  never a production downgrade. Without the feature the variable is ignored and, if set, a startup
  line says `ignoring it and using the Secret Service` (`memoryd/src/keysel.rs`, pure `select`;
  tests for both builds, `memoryd/tests/binary.rs` runs the feature-built binary on a private
  bus with `env_clear` and scratch HOME/XDG, claims `org.quire.Memory1` and takes a `Record` into
  `desktop`; the feature-off twin runs in a default-feature build).
- **Landlock breaks caller identity (found, worked around in test builds only)**: under its own
  Landlock domain memoryd cannot read `/proc/<pid>/exe` of a process outside the domain (the
  kernel's ptrace access check), so `ProcPeers` answers `NotAllowed` for every caller. The binary
  test only passed with the sandbox off. Test builds therefore also honour `MEMORYD_SANDBOX=off`
  (same `test-keys` feature). The production fix is open: resolve the peer by something Landlock
  does not gate (for example the unit's cgroup, or `SO_PEERCRED` pids matched to a systemd unit),
  or apply Landlock only after a different identity source exists. Until then the packaged daemon
  under Landlock refuses every caller outside the jail.

### Open after the f4 fill

- **`landlock` is not in quire's pinned dependency block** (`docs/workspace-deps.toml`): almanac's
  `Cargo.toml` names `landlock = "0.4"` (0.4.7 in the lockfile, MIT OR Apache-2.0) ahead of it. The
  line joins quire's file first (CONVENTIONS "dependencies"). Quire's session owns it.
- **The lock signal against a real Secret Service** is unproven: gnome-keyring and KWallet differ in
  which object carries `Locked` (the collection is the documented one). Needs a real session.
