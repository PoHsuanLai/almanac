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
