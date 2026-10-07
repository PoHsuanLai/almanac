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
- **Cross-repo dependencies are git deps at pinned revs** (`porter-core`, `prov`, `porter-infer`,
  `porter-client`, `porter-dbus` from `https://github.com/PoHsuanLai/porter`), so a plain `git clone`
  builds with no sibling checkout (quire `CONSUMING.md` section 1). The URL spelling and the rev
  match porter's own pins of stoker and quire, so cargo sees one copy of each crate. For work that
  spans repos, override the pin locally with a `[patch]` that is never committed: put it in a
  `.cargo/config.toml` in a directory ABOVE the checkout (cargo merges the config of every parent
  directory; this repo's own `.cargo/config.toml` is tracked), for example

  ```toml
  [patch."https://github.com/PoHsuanLai/porter"]
  porter-core = { path = "/path/to/porter/crates/porter-core" }
  prov        = { path = "/path/to/porter/crates/prov" }
  # one line per porter crate almanac names; add the same for stoker/quire crates if the
  # local porter checkout pins a different rev than the one in Cargo.lock
  ```

  A local `[patch]` rewrites `Cargo.lock` entries for the patched crates; commit the lock only
  from a build without the override (`cargo update -p porter-core --precise <sha>` after a bump).
  The gate builds outside the jail (git deps are fetched there); the jail only runs the archived
  tests.

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

**Who is calling.** `Peers` turns the sender's unique name into a `Caller`. `ProcPeers` is porter's
`ProcCallers` (bus pid, then `/proc/<pid>/cgroup` alone; no `exe`, which a Landlock domain cannot
read) with `callers::caller_for` mapping porter's caller onto almanac's: role `cua` is `Cuad`,
role `sheet_host` of app `org.quire.Shell` is `ShellUi`, role `agent` of app `org.quire.Intents` is
`Router`, every other identified process is `App`. The table is `/etc/quire/memory-callers.toml`
with `<config>/quire/memory-callers.toml` laid over it (user rows win; a missing file is empty, a
bad one stops the daemon): porter's `[[caller]]` rows of `app`, optional `unit`, `role`.
`dbus/memory-callers.toml` is the packaged system file. `TablePeers` is the test seam.

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
- **Landlock broke caller identity** (CLOSED by the f4-almanac-callers lane): under its own
  Landlock domain memoryd cannot read `/proc/<pid>/exe` of a process outside the domain, so the
  exe-based `ProcPeers` refused every caller. Callers are now identified by porter's
  `ProcCallers` from `/proc/<pid>/cgroup` (service unit, `app-*.scope`, Flatpak scope; anything
  else, such as a terminal's child, is `NotAllowed`). The ruleset reads `/proc` files only
  (`Policy::read_files`: `ReadFile`, no listing, no execute) and `/proc` is no longer a readable
  tree. almanac now depends on `porter-dbus` (check-boundary row). `MEMORYD_SANDBOX=off` stays
  test-only, but `memoryd/tests/binary.rs` runs with the sandbox ON.
- **`MEMORYD_PROC_ROOT=<dir>` for docket-accept (TEST ONLY, feature `test-proc-root`, separate from `test-keys`; the acceptance build enables both)**: memoryd reads
  `<dir>/<pid>/cgroup` instead of `/proc/<pid>/cgroup`, and `<dir>` joins the sandbox's
  read-files rule. A startup line says `TEST BUILD: reading callers from the proc root <dir>, not /proc`. Without the
  feature the variable is ignored and a line says `ignoring it and reading /proc`. A harness
  writes, for each test process pid (the bus's pid of the connection), a file
  `<dir>/<pid>/cgroup` containing `0::/system.slice/intentd.service` (router),
  `0::/system.slice/sill.service` (shell), `0::/system.slice/cuad.service` (cuad) or
  `0::/user.slice/app.slice/app-org.example.Mail-1.scope` (an app), plus a callers file with
  `[[caller]]` rows for the units (the format above). Files may be written after the daemon
  starts: it reads at call time. Unit names checked: intentd, companiond, readerd, cuad,
  voiced, actions-mcp (docket/cua `dist/*.service`), sill (`sill/dist/sill.service`), inferd.
  `~/desktop` has no unit files. `voiced.service` and `actions-mcp.service` have no row (neither
  calls memoryd yet).

### Open after the f4 fill

- **`landlock` is not in quire's pinned dependency block** (`docs/workspace-deps.toml`): almanac's
  `Cargo.toml` names `landlock = "0.4"` (0.4.7 in the lockfile, MIT OR Apache-2.0) ahead of it. The
  line joins quire's file first (CONVENTIONS "dependencies"). Quire's session owns it.
- **The lock signal against a real Secret Service** is unproven: gnome-keyring and KWallet differ in
  which object carries `Locked` (the collection is the documented one). Needs a real session.


## f4-settings: the settings schema and its reader

The Settings app (detent) builds its Intelligence page from the schema each daemon ships (design/22
section 9.2). almanac's covers the `memory.*` keys of section 3.28 that memoryd reads, and memoryd
reads them.

1. **`dist/settings/almanac.settings.toml` has 9 rows**, none `agent = "settable"` (`memory.*` is never
   agent-settable). **On the Intelligence page** (section 5, Memory): `memory.files.at_rest` (toggle),
   `memory.consolidation.when` (segmented: nightly, manual, never), `memory.consolidation.apply` (toggle: auto, review). **Advanced:** `memory.retention.
   {search,file_unexplained,session,audit_body,audit_header}_days` (1..=3650) and `memory.pending_ttl_days`
   (1..=365).
2. **Left out of the schema, because nothing could read it:**
   - ~~`memory.consolidation.apply`~~ is built (see "consolidate-apply" below).
   - **`memory.join_window_ms`** (Advanced). memoryd does not run the file watch yet (`Watcher` is exported,
     nothing starts it, so `almanac_watch::join` never runs and `JoinWindow::PROPOSED` is used by nobody).
     Closes when memoryd wires the watch: `JoinWindow(ms)` from this setting, 100..=10000.
3. **The reader is `almanac_service::settings`** (`read(text, base)`, `Locator`, `MemorySettings`). The file is
   `$XDG_CONFIG_HOME/almanac/settings.toml` (then `$XDG_CONFIG_DIRS`; the first file that reads wins whole). A
   value of the wrong type, out of range or not a word of its key falls back **to the base value of that key**
   (`MemorySettings::default()`, the shipped defaults) and is logged (`memoryd: settings: memory.pending_ttl_days:
   outside 1..=365; using the previous value`). A file that is not TOML keeps every base value; a key the file
   stops setting is its base again. Unknown keys (including `memory.join_window_ms`) are listed and ignored.
4. **memoryd follows the file live** (`settings_watch.rs`: a `notify` watch on `$XDG_CONFIG_HOME/almanac`, 30 ms
   debounce, the whole file read again, `MemoryService::apply_settings`). Every request builds its `Cx` from the
   settings in force, so a change applies to the next request:
   - retention: `RuleSet::with_retention(&RetentionDays)` lays the person's days over the rules' defaults
     (`search.*`; `session.*` and `cua.*`; `policy.*`, `consent.*` and `memory.*`) **for the request only**:
     `service.rules()`, which memoryd writes to `memory.toml`, is still what `SetRule` made it, so the settings
     never become sticky in `memory.toml`. Unexplained file changes: `admit_with` and `default_retention_with`
     take the days (the old `admit` and `default_retention` pass the shipped 7). Audit headers:
     `header_expired_after(days, ..)` (the old `header_expired` passes 365). The `companion.*`, `thing.*` and
     `file.*` defaults are not settings and are untouched.
   - `memory.pending_ttl_days`: `age_pending(now, ttl)` compares the fact's age with it. The fact machine's own
     `Age` arm still says 14 days (its table test stands); the service no longer asks it.
   - `memory.files.at_rest`: the vault of a Space made **after** the change. A Space that exists keeps the way
     it was made (its `spaces.toml` entry); design/22 says "per Space", and a per-Space override is not built.
   - `memory.consolidation.when`: `Never` makes `RunConsolidation` answer `Invalid("consolidation is turned
     off ...")`; `Manual` is today's behaviour; `Nightly` is a new daily timer in memoryd
     (`Daemon::consolidate_all`, first run an hour after start, each Space, announced like a requested run),
     which reads the setting afresh each night. **The nightly run does not ask whether the desktop is idle or
     on AC power** (`RunEvent::Tick { Idle, Ac }` is not consulted): memoryd has no idle or power source yet.
     Closes when sill's idle state reaches memoryd.
5. **The sandbox names the settings directory** (`Policy.read_dirs`, `<config>/almanac`): `prepare` makes it at
   start and Landlock gives it read rights only, so a settings file the Settings app writes later is covered
   (a rule can only name a path that exists when it is applied). memoryd never writes it.
6. **`memoryd --write-schema DIR`** writes `almanac.settings.toml` for a local install. `notify` is now a
   dependency of memoryd as well as almanac-watch (`scripts/check-boundary.sh` comment updated); the watch
   code has the same shape as docket's intentd and cua's cuad (`SettingsWatch`): one shared crate replaces the
   three copies when porter has a place for it.
7. **Tests.** The schema is held structurally to the key table (ranges, words, on-page rows, no `agent` mark) and
   parses with the Settings app's loader (`ds_settings::Schema::from_toml`, run by hand from a scratch
   project: a dev-dependency on ds-settings would unify zbus's executor features); every schema key is read
   (table test), every key has a bad-value fallback case; `almanac-fake/tests/settings.rs` changes the
   settings under a serving service (pending ttl, new Space vault, sweep retention, `when`);
   `almanac-core/tests/retention_days.rs`; `memoryd/tests/settings.rs` changes the file under a running
   service and waits on the watch's own event.


## consolidate-apply: applying a proposed consolidation run

Todo table (this lane): `todo!()` bodies 0 before, 0 after. The apply step itself (every hunk kind,
pre-images, `Revert`) was built in Me4; what was open was the review path, the interface ask of
f4-settings item 2. Closed here.

| Item | State |
|---|---|
| `MemoryRequest::ApplyConsolidation(RunId)` (ShellUi only; codec, `org.quire.Memory1.ApplyConsolidation(run)`, introspection XML, proxy, serve, invoke, auth row, queue claim `Everything`) | done |
| `memory.consolidation.apply` (`ConsolidateApply::{Auto,Review}`, words `auto`, `review`; schema row on the Intelligence page, key table, `MemorySettings.apply`) | done |
| `Open::run_consolidation` under `Review` keeps the checked hunks, applies nothing, leaves the run `Proposed` and the cut where it was | done |
| `Open::apply_consolidation` runs the machine's `Proceed` step (`ApplyHunks`, `LogConsolidated`) over the kept hunks | done |
| `Grounds` (pure): a Promote or Supersede whose cited event (body present), cited fact or replaced active fact is gone is skipped | done |
| A `Proposed` run survives a restart | done in "proposal-file" below |
| `ConsolidationReady` on apply | done in "proposal-file" below |

Behaviour: under `Review` a run stops at `Proposed` (its `DraftView` is readable with `Consolidation`
and `ConsolidationReady` fires, as for any run). `ApplyConsolidation(run)` applies only the last run
and only while it is `Proposed` (else `Invalid`); each hunk is checked again against the Space as it is
then (Tidy and ExternalEdit already compare `before`; Promote and Supersede through `Grounds`), so a
proposal cannot bring back what was forgotten in between. The result is an ordinary applied run, so
`Revert` works exactly as in auto mode. Nothing is deleted at any point: a draft never removes a
fact, a supersede only changes the derived state (the old fact stays in its file), and `Revert` is
refused for a run that was only proposed. A newer run replaces an unapplied proposal (the old one
changed nothing).

Owner questions (1)-(3) and the apply-time signal are answered in "proposal-file" below.


## proposal-file: a proposal is a file

The owner's four answers to the "consolidate-apply" questions, built. `todo!()` bodies 0 before, 0 after.

1. **A proposal is a file** (spec: `<space>/consolidation/<run>.toml`, in the Space's vault, so sealed
   when the vault is). It is written (the vault's atomic temporary file + rename) when the run is
   drafted, before the run is reported: a Review run whose file cannot be written fails and applies
   nothing. Fields: `format = 1`, `run`, `state` (`proposed | applied | reverted | discarded |
   superseded`), `drafted`, `settled` (when it left `proposed`), `cut` and `head` (the log cut the run
   was drafted from and the head it read to), `erased`, `[[hunks]]` (the serde form of `Hunk`) and
   `[[skipped]]` (`hunk` + `reason`). Golden: `almanac-fake/tests/golden/run_proposed.toml`.
   A hunk is its own pre-image for the apply-time re-check (Tidy and ExternalEdit carry `before`,
   Promote and Supersede carry the facts and links `Grounds` looks up), so no separate pre-image
   table is stored for a proposal (nothing has been changed yet).
2. **Outcomes are recorded, files are kept.** Proposed -> Applied (then Reverted), Discarded or
   Superseded; each rewrites the same file. Every run, auto-applied ones too, has a file. The state
   changes are steps of the run machine (`RunEvent::{Discard, Supersede}`; `RunState` gains
   `Discarded` and `Superseded`). The pre-images of files an applied run rewrote are **not** written
   to the file (they hold whole topic files; a later forget would have to scrub them, and a revert
   after a restart is not asked for): `Revert` still works only in the daemon run that applied.
3. **`MemoryRequest::DiscardConsolidation(RunId)`**, shell only (ShellUi), queue claim `Everything`,
   bus `org.quire.Memory1.DiscardConsolidation(run)` with `run` a `RunId` JSON, no output (the
   `Consolidation(space)` view then shows `state: discarded`). Anything but a still-proposed last run
   is `Invalid`.
4. **A newer run marks a proposal `superseded`** in its file (written after the new file, so a crash
   in between leaves two `proposed` files; start resolves them: the newest by `drafted`, `head`, run
   id is the Space's proposal, the rest are marked superseded). Applying or discarding a superseded
   or discarded run is `Invalid` ("no such run" or "cannot apply a run that is superseded").
5. **At start** (`Open::restore_proposal`, when a Space opens) the newest `proposed` file becomes the
   Space's last run with its `cut`, so `Consolidation(space)` reads it and `ApplyConsolidation` works
   after a restart; the apply-time `Grounds` check and the Tidy/ExternalEdit `before` comparison still
   guard staleness. Because a Space only opens on first use, `ApplyConsolidation` and
   `DiscardConsolidation` open the known Spaces until one holds the run. A torn or foreign file (a
   truncated TOML, bytes that are not TOML, another `format`, a temporary file) is not a record and
   is skipped, never overwritten or deleted.
6. **The applied view lists what was applied.** `DraftView.hunks` is the applied hunks only, and the
   new `DraftView.skipped: Vec<SkippedHunk { hunk, reason }>` (`#[serde(default)]`) the rest, with
   `SkipReason::{EventGone, FactGone, ReplacedFactGone, FileChanged}` (`Grounds::why_not`; a Tidy or
   Stamp that no longer fits its file is `FileChanged`). A proposal itself has `skipped = []`. The
   same holds for an auto-applied run.
7. **`ConsolidationReady` on apply and discard.** The service knows the Space (it is the one that
   holds the run), so it raises `ServiceEvent::ConsolidationChanged(space, run)` and memoryd's
   `flush_events` emits `ConsolidationReady(space, run)`; `RunConsolidation` still emits it from the
   reply. `Revert` does not emit it.
8. **Forget scrubs the files.** A forget that removes facts or event bodies takes out of every run
   file the hunks that carry their text (hunks citing a forgotten event or fact, a Promote or
   Supersede of a forgotten fact, a Flag on one; and, when any fact went, every Tidy, Stamp and
   ExternalEdit hunk, because they hold whole topic text) and adds the count to `erased`. The record
   stays; the text goes.

Open: (a) closed in "forget-revert" below; (b) after a restart the
log cut is `Seq(0)` again for the first run unless a proposal is loaded (as before); (c) hunks the
checks dropped before the proposal (`check_draft`) are not recorded anywhere.


## forget-revert: forget wins over Revert

Closes open question (a) of "proposal-file". `todo!()` bodies 0 before, 0 after.

The last applied run keeps whole topic files as they were (`LastRun.pre_images`) so `Revert` can
restore them; a forget could therefore have been undone by a revert. The rule (`revert_guard.rs`,
pure), the simplest that is obviously safe: any forget that erases anything (facts or event bodies)
while the run's pre-images are held drops **all** of them and sets `LastRun.revert` to
`RevertGuard::Forgotten`. `Revert` of that run is then refused with `Refusal::Invalid("a memory this
run touched was forgotten since")`; nothing is partially restored, and the run stays `Applied`. No
text is matched, so a forgotten event whose text lives only in a pre-image is covered too. A
`Revert` with no forget in between works as before.

Wire: none. The refusal is the existing `Invalid(String)`; a shell may match that reason text to
say so. The same forget already scrubbed the served `DraftView` and the run files (`must_go`); the
`LastRun` held nothing else with text (`added` and `superseded` are ids). Tests:
`almanac-fake/tests/forget_revert.rs` (six).

The earlier gap (a forgotten event whose text lives only in a pre-image) is closed by the rule above.

## Portable core (design/36)

- Built: `almanac-watch` feature `linux` (default) holds `InotifyWatch`; `almanac-seal` `ProvidedKeys` is the
  portable key store (a master key the app provides); `scripts/check-portable.sh` gates the core crates
  with `--no-default-features` and cross-target checks. `almanac-seal`/`almanac-watch` `pure-hash` (off by
  default) builds blake3 without C for a cross check.
- Built: `almanac-local` (below), the portable disk `Backend`; `almanac-client` `quire-desktop = ["dbus"]`
  (default on) is the app-level desktop switch (ARCHITECTURE 1a names the two kinds of feature).
- Open: `InotifyWatch` pairs renames by inotify cookie; a notify-backed watcher for macOS/Windows needs its
  own translation (FSEvents and ReadDirectoryChanges report renames differently).
- Open: cross-checking `eventlog`, `recall`, `almanac-service` and `almanac-client` needs the target's C
  toolchain for SQLCipher and OpenSSL; only a macOS/Windows CI runner can check them.

## almanac-local (design/36)

`almanac-local` is what an app that hosts its own memory (mailo on macOS or Windows) opens instead of
memoryd's `SystemBackend`: `LocalBackend` over `SqliteLog` (SQLCipher `events.db`), `SealedDir` or `PlainDir`
by the Space's `VaultKind`, `ExactScan` over FTS5 (`index.db`) and `ProvidedKeys`. It reads no environment,
no XDG directory, no `/proc`, no wall clock; it uses no D-Bus, inotify or Landlock. The app then serves
`Memory::over(InProcess::new(Arc::new(almanac_local::open(backend, rules)?), caller))`.

What an app must provide:
- **A root directory** (`Root::new(path)`, e.g. its application-support directory). Below it: `data/quire/memory`
  (`spaces.toml`, each Space's `events.db` and sealed files), `cache/quire/memory/<space>/index.db` (rebuildable).
- **A master key**: `ProvidedKeys::new(SpaceKey)`, 32 bytes from its own sign-in or keychain, the same on every
  run. A different one is not an error at open: the first request on a Space answers `Refusal::SpaceLocked`
  (`LogError::Locked` underneath, mapped by the service's `backend_refusal` as is a locked key store; a sealed file read is `VaultError::Sealed`). `ProvidedKeys` persists nothing, so
  erasing a Space for good is the app discarding the key it provided (`destroy` bars it only for this process).
- **A clock** (`almanac_service::Clock`): `almanac_local::WallClock` (one `SystemTime::now`) is the ready-made
  one; `LocalBackend` takes the clock as a parameter, so tests pass a stepped one and only `WallClock`'s own
  smoke test reads the time. It lives here, not in `almanac-service`, which stays free of ambient reads
  (`almanac-local` already reads OS randomness for ids); memoryd keeps its own `SystemClock`.
- **Optionally an embedder** (`with_embedder`; a generic parameter, not a trait object, because `Embedder`
  returns `impl Future`). The default `NoEmbedder` always answers `Unavailable`, so the index stays lexical-only
  (FTS5 keyword search) and the exact scan has nothing to scan. The embedder's card names the vector space; a
  different card means a rebuild.
- **Optionally a consolidator** (`with_consolidator`); the default answers `Unavailable`, so a consolidation
  run fails and changes nothing.
- **Optionally a watcher.** The backend takes no `FileWatch`: file-change capture is the app's own loop
  (`almanac_watch::FileWatch` and `join`, then `Record`/`ExplainFile` through the client). There is no portable
  watcher yet (see Open, above).

The app calls `create_space(&service, id, VaultKind)` once per Space (it writes `spaces.toml`); `open` registers
the Spaces in that file at every start, as memoryd does. The rule set is the app's (`RuleSet::standard()` or
its own), and it keeps it: there is no `memory.toml`. Settings reach the service by `apply_settings`.
Cross-target: the crate depends on SQLCipher with vendored OpenSSL, so only a macOS or Windows runner can
check it (the same note as `eventlog`; `check-portable.sh` reports it).

Closed asks (almanac-local follow-ups): a locked log or key store opening a Space is `Refusal::SpaceLocked`
(it was `Invalid("event log: the event log is locked")`; the wire text changes only for that case, the
`SpaceLocked` error name already exists in memoryd's codec), and the portable `WallClock`.

## Recent bodies: an Area payload is the owner's form (fixed)

`Recent` with `BodyMode::Json` returned an `Area` payload as the whole envelope
(`{"kind":"area","v":{..,"json":"<owner form>"}}`), though `RecentEntry.body` documents the owner's form.
It now returns `AreaPayload.json` unchanged; `Message` and `Episode` keep their serde form, as documented.
No other read path carries bodies (search hits and recall carry text, not bodies). docket's master reads both
forms, so the change is compatible. Tests: `recent_with_bodies_returns_an_area_payload_in_the_owners_form`
in almanac-fake (`service.rs`) and almanac-local (`disk.rs`).

## session-log: Entries, RecordDurable and not-for-recall (docket's asks A1, A3, A4)

docket's durable sessions (`agent-spec/acp-sessions.md` section 5) append `SessionEntry` events under
`companion.session.*` and rebuild from them. Taint is a write-ahead entry, so docket must know an entry is
durable before it reveals text, and must fail closed when almanac cannot say so.

**A1, paging one session.** New request `Entries(SpaceId, EntriesQuery)` and reply `Entries(EntriesPage)`; bus
member `Recall.Entries`; client `Memory::entries`. `Recent` is untouched (its query is built by struct literal
in docket, so a new field would not be additive, and it is newest-first by time, the wrong shape for a replay).
The stream is chosen by kind patterns plus `about: Option<ThingRef>` and not by a session id inside the body,
because the log already indexes a payload's `things` (`LogRead::touching`): the lookup is by an indexed key,
needs no JSON parsing of an owner-defined body, and the same key lets one forget of the thing erase a session
as a unit (the base of ask A2). docket records each entry with the session as a `Subject` `ThingView`
(app `org.quire.Companion`, kind `companion.session`, key the session id). Order is append sequence; the cursor
is the last event seen (`after`, exclusive), and `next` is set only when more matched, so a page boundary is
exact. Without `about` the read scans from the cursor (every session's entries in order), and with it the read
touches only that session's rows. Bodies follow `BodyMode` and always travel with their label. Audited as the
same `Memory.Read` scope as `Recent` (`ReadScope::Recent`); same callers (`Router`, `ShellUi`). Erased bodies
drop out of `about` reads (their `things` rows are erased with them) and come back as headers without a body in
kind-only reads.

**A3, the durable ack.** `Record` was fire and forget: `Recorded(EventRef)` or `Ok`, where `Ok` hid a buffered
record (locked Space) and a dropped one (paused, a rule, a mark). New request `RecordDurable(Record)` with reply
`Durable(Ack { event })`, bus member `Record.RecordDurable`, client `Memory::record_durable`; an absent daemon is
the client's `Transport(Absent)`, not a swallowed no-op. The ack is sent after `LogWrite::append` returns, and
`SqliteLog` now sets `synchronous=FULL` explicitly (SQLite's default in WAL mode is `FULL`, but the durability
story is now written down and tested, not inherited). New typed refusals: `SpaceFull` (a failed append with `LogError::Full`), `Unavailable` (any other storage
failure, or a Space that would not open for a reason other than a lock) and `NotKept(DropReason)`; a locked
Space is the existing `SpaceLocked` and is never buffered on this path. The typing is in `append_refusal`, so a
plain `Record` that hits a full or failing log now also answers `SpaceFull` or `Unavailable` where it answered
`Invalid(text)`; opening a Space is unchanged. The sequence is the Space log's own (strictly increasing; audit
events interleave), so a reader pages with `Entries` and does not count. **A durable append refuses whenever
the body would not be stored whole**, including for audit-class records: `admit` keeps only a header for a
session entry (an `Area` payload is audit class) in a paused Space or under a `Never` or `HeaderOnly` rule, and
`Record` answers `Recorded` for it, which would have been a false ack of a lost taint entry. The pure
`withheld(record, rules, state, marks) -> Option<DropReason>` (core, with a test that it agrees with `admit`)
names the reason, `DropReason` gained `HeaderOnlyRule(RuleId)` for the rule mode that has no reason of its own,
and `RecordDurable` answers `NotKept(reason)` for any of them. Finding for docket: a `Never` or `HeaderOnly`
rule that covers `companion.session.*` (or a pause) stops session logging, and docket sees it as a refusal and
fails closed. `RecordBatch` has no durable form yet.

**A4, not for recall, per kind and not per event.** `Recallable::{Yes, No}` with `Recallable::of_kind` and
`of_body`: `companion.session.*` is `No`. A rule over the kind, not a field on `Record`, because (1) `Record` is
built by struct literal in docket, so a field is a breaking change; (2) a per-event flag is a promise each writer
must remember to make and a bug in one code path puts model-derived text into recall, while a kind rule is
enforced by almanac for every writer and every future `companion.session.<slug>`; (3) the index and consolidation
are rebuilt from the log alone, and a function of the kind is the only classification the log already stores
(the chained header holds the kind; a body flag would be erasable); (4) it costs no wire or storage change.
The limit: a stream outside the prefix cannot opt in without a new prefix in `NOT_FOR_RECALL`. Enforced in
`event_docs` (so record-time indexing, rebuild, sweep and forget all agree; search and `Inject` read the index),
in the consolidation input (so no draft names a session event), and in `Related` (a session names its thing, and
a related-events read is a recall read). The primer and facts follow from consolidation. There is no profile
read in almanac. `Recent`, `Entries`, `Timeline` and export return the entries. Not enforced: `Propose` can still
cite a session event as a fact's link (the caller is the router or shell and names the event deliberately).

Tests: `almanac-core/tests/companion.rs` (the kind rule), `almanac-fake/tests/session_log.rs` (paging with
interleaved events, cursor resume, bodies, monotonic ack, refusal when paused, not-for-recall in search, inject,
related, consolidation, and present in Recent, Entries and export), `almanac-fake/tests/open_refusals.rs`
(locked, full, failing log), `almanac-local/tests/disk.rs` (an acked append survives a reopen and pages back),
`almanac-dbus` codec and introspection tests (new members, errors), `almanac-core/tests/wire.rs`.
