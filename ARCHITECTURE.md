# Architecture

almanac is the desktop's memory: a typed, append-only, hash-chained **event log** per Space
(which is also the audit log), the companion's **memory files** as plain markdown (add-only,
dated, linked to their sources), a rebuildable **recall index** (FTS5 plus exact-scan vectors,
fused), and **memoryd**, the daemon that owns them and serves `org.quire.Memory1`. The design is
the area spec (`memory.md`) as consolidated by the program spec (`SPEC.md`, which wins where they
differ: crate map section 1, one-home owners section 2, cross-area signatures section 3.4,
daemons and D-Bus section 4). This file is the map of the code that freezes those interfaces.
`CONVENTIONS.md` holds the rules; `FINDINGS.md` the open items and every `todo!()`.

Reading order: section 1 (find the crate), section 3 (find the home), section 4 (find the
trait), section 5 (what is built), section 6 (copy the recipe).

## 1. Crates and allowed edges

| Crate | Purpose | I/O |
| --- | --- | --- |
| `almanac-core` | the vocabulary: ids and their grammars (`FactId`, `TopicPath`, `KindTag`, `KindPattern`, `SpacePath`), things (`ThingRef` = `prov::EntityId`), `Record`, `EventBody` (with `Message` and `Episode`), `AreaPayload`, `Episode` and its skeleton, `InjectQuery`/`RecentQuery`, facts, remember rules and the pure `admit`, the wire (`MemoryRequest`, `MemoryReply`, `Refusal`, `Caller`), the timeline, forget-plan, draft and status views, the export manifest, `Dirs`; re-exports the porter and `prov` names the other crates use | none |
| `almanac-seal` | `SpaceKey`, purpose subkeys (`derive`), `DbKey`, `seal`/`unseal` for files, the `KeyStore` seam, `ProvidedKeys` (Space keys derived from a master key the app provides: the portable key source), `MemoryKeys` (feature `testing`), `Oo7Keys` (feature `oo7`) | none (oo7 behind its feature) |
| `eventlog` | the header's canonical bytes, `link`, `verify_chain`, `LogRead`/`LogWrite`, `SqliteLog` (SQLCipher), `MemoryLog` (feature `testing`), the schema | rusqlite |
| `memfiles` | the topic file format (`parse_topic`, `render_topic`, the trailer), `VaultPath`, the `Vault` seam, `PlainDir`/`SealedDir`, `MemoryVault` (feature `testing`), `Store`, `Primer` | the filesystem through `Vault` |
| `recall` | generic: `Embedder`, `VectorIndex`, `Fts5`, `ExactScan`, `Index`, `fuse_rrf`, `chunk`, the index state machine, `FakeEmbedder` (feature `testing`). Knows nothing of almanac | rusqlite |
| `recall-fastembed` | `FastembedEmbedder` (in-process ONNX). Excluded from clippy and test: ort downloads binaries | ort through fastembed |
| `almanac-service` | memoryd's core over seams: `allowed`, the Space, fact, forget-plan and consolidation machines, retention, timeline rows, the export writer, the config files, `MemorySettings` and the lenient settings reader (`settings`: the key table held to `dist/settings/almanac.settings.toml`, `Locator`), `Backend` and `MemoryService` | none (seams are passed in) |
| `almanac-watch` | `FileWatch`, `WatchError`, the pure `join` of observed changes and app-supplied reasons; `InotifyWatch` (notify 8.2) behind the `linux` feature | inotify (feature `linux`) |
| `almanac-dbus` | `org.quire.Memory1` (`Record`, `Recall`, `Control`) as zbus proxies and skeletons (the introspection source), `MemoryError`, the argument codec (`encode_request`/`decode_request`, `encode_reply`/`decode_reply`), `invoke` (the caller's half) and the served objects over a `Serve` handler (the daemon's half) | zbus |
| `almanac-client` | the app-facing `Memory` over a `Transport`: `Absent` (no-op on other desktops), `DbusTransport` (feature `dbus`), `InProcess` (feature `in_process`, off by default: only it links `almanac-service`, SQLCipher and OpenSSL) | through its transport |
| `almanac-local` | the portable on-disk `Backend` for an app that hosts its own memory: `LocalBackend` (SQLCipher log and index, sealed or plain files, `ProvidedKeys`, an app-passed clock, embedder and consolidator, `NoEmbedder`/`NoConsolidator` by default), `Root` (the one directory the app gives), `open`, `create_space`, `save_spaces` (`spaces.toml`); no XDG, bus, inotify or Landlock | none (seams are passed in) |
| `almanac-fake` | test-only: `fake_service`, `FakeBackend`, `FixedClock`, `SteppedClock`, `SharedVault`, `ScriptedConsolidator`, `Scratch`, the five fixtures | none |
| `memoryd` | the daemon and its library: `SystemBackend` (over any key store, embedder and consolidator), `SystemClock`, `InferdEmbedder`, `InferdConsolidator`, the XDG roots, `Serialised` (a queue per Space), `Peers` (who is calling), `Daemon` (the bus handler), the Landlock `sandbox` policy, `keysel` (which key store: the Secret Service, or with the test-only `test-keys` feature a sealed file named by `MEMORYD_KEYS=file:<path>`; the same feature lets `MEMORYD_SANDBOX=off` skip Landlock, see FINDINGS; `procroot`: the test-only `test-proc-root` feature's `MEMORYD_PROC_ROOT=<dir>`, a fixture `/proc` for callers); `callers` (the callers file and porter's caller mapped onto almanac's); the binary applies the sandbox, then serves the session bus; `SettingsWatch` (the directory watch on `almanac/settings.toml`; `apply_next` puts each change in force on the service), the nightly consolidation timer, `Policy.read_dirs`, `--write-schema` | everything |

## 1a. Portable core and desktop extras

Quire's rule (design/36-PORTABLE-CORE.md): everything except the desktop itself is
cross-platform, and the desktop's features are additive. For almanac:

| Portable core (builds with `--no-default-features`) | Desktop extra |
| --- | --- |
| `almanac-core`, `almanac-seal` (`ProvidedKeys`), `eventlog`, `memfiles`, `recall`, `almanac-service`, `almanac-local` (the app's disk `Backend`), `almanac-client` with `in_process`, `almanac-watch`'s seam and `join` | `almanac-dbus`, `memoryd` (the one shared memory daemon: Secret Service keys, Landlock, the bus, file watching, the nightly timer), `almanac-client`'s `dbus` feature, `almanac-seal`'s `oo7` feature, `almanac-watch`'s `linux` feature (`InotifyWatch`) |
| test helpers: `almanac-fake`; `recall-fastembed` (ONNX, optional) | |

An app on another desktop hosts the service itself: `Memory::over(InProcess::new(service,
caller))` over a `Backend` whose keys are `ProvidedKeys` (the app gives one 32-byte master key,
from its own sign-in or keychain; each Space's key derives from it) and whose other seams are
the portable ones above. `ProvidedKeys` persists nothing, so `destroy` bars the Space only for
the life of the store: erasing a Space for good is the app discarding the key it provided.

Feature layout. Two kinds of feature, as design/36 names them. A **platform feature** says what
the OS provides and keeps its name: `almanac-watch` `linux` (default; `InotifyWatch`, the `notify`
dependency), `almanac-client` `dbus` (the D-Bus transport), `almanac-seal` `oo7` (Secret Service
keys), plus `in_process` (hosting the service in the app; off by default because it links SQLCipher
and OpenSSL). An **app-level desktop switch** says "this app is on the Quire desktop" and is
`quire-desktop`; it is declared only by a crate that holds a D-Bus client, by implication, never as a
rename: `almanac-client` `quire-desktop = ["dbus"]`, default on. No other almanac crate needs the
alias: `almanac-dbus`, `memoryd` are the desktop itself, `almanac-watch` and `almanac-seal` hold no
D-Bus client (their platform features are what a non-Linux OS lacks). An app on macOS or Windows
depends on `almanac-client` with `default-features = false, features = ["in_process"]`. Default
builds are unchanged. `scripts/check-portable.sh` checks the left column (including
`almanac-local`): `cargo check --no-default-features` on each, no `zbus`, `inotify`, `notify`,
`landlock`, `oo7` or `secret-service` in its `cargo tree`, and a `cargo check --target` for each
installed macOS/Windows rustup target. `check-boundary.sh` checks `almanac-client`'s boundary rule
without its defaults (the rule is about what an app without the desktop switch links).

Allowed direct edges (checked by `scripts/check-boundary.sh`; dev-dependencies are outside it):

| Crate | May depend on |
| --- | --- |
| `almanac-core` | `porter-core`, `prov` (porter, by sibling path) |
| `almanac-seal` | `almanac-core` |
| `eventlog`, `memfiles` | `almanac-core`, `almanac-seal` |
| `recall` | nothing of ours |
| `recall-fastembed` | `recall` |
| `almanac-service` | `almanac-core`, `almanac-seal`, `eventlog`, `memfiles`, `recall` |
| `almanac-watch`, `almanac-dbus` | `almanac-core` |
| `almanac-client` | `almanac-core`; `almanac-service` with feature `in_process`; `almanac-dbus` with feature `dbus` |
| `almanac-fake` | `almanac-core`, `almanac-seal`, `eventlog`, `memfiles`, `recall`, `almanac-service` |
| `almanac-local` | `almanac-core`, `almanac-seal`, `eventlog`, `memfiles`, `recall`, `almanac-service` |
| `memoryd` | every crate above except `almanac-client`, `almanac-fake`, `recall-fastembed`; and `porter-core`, `porter-dbus` (its `callers`: who is calling), `porter-infer`, `porter-client` |

almanac never depends on stoker, docket or cua: their payloads (policy, consent, computer-use
steps, sessions) are `EventBody::Area` in their owners' serde form, and `check-boundary.sh`
fails if any of their crates enters the tree. Other crates reach porter's names through
`almanac-core`'s re-exports, so the edges stay this exact.

External boundaries (default features): `almanac-core` never reaches `zbus zvariant tokio
reqwest hyper oo7 ort fastembed rusqlite notify toml`; `almanac-seal`, `memfiles`: the same
minus `toml`/with `rusqlite` forbidden; `eventlog`, `recall`, `almanac-service`, `almanac-fake`,
`almanac-client` (default features) never reach the effect list (`zbus ... fastembed`) or `notify`; `almanac-watch`
may reach `notify` and never `rusqlite`; `almanac-dbus` reaches `tokio` only through zbus's
`tokio` feature. `oo7` is behind `almanac-seal`'s `oo7` feature only; `zbus` behind
`almanac-client`'s `dbus` feature only; `rusqlite` and `openssl-sys` behind its `in_process` feature only.

## 2. Modules

| Crate | Modules |
| --- | --- |
| `almanac-core` | `slug`, `text` (ids, `Digest32`/`Link32`/`ContentDigest`/`PlanDigest`, `UserText`, `JsonText`) < `ids`, `thing`, `file`, `area`, `op`, `chain` < `event` < `fact`, `rules`, `space` < `admit` < `query`, `timeline`, `views`, `export` < `request`, `reply`, `caller`, `dirs` |
| `almanac-seal` | `keys` (`SpaceKey`, `Purpose`, `derive`, `SubKey`, `DbKey`), `seal`, `store`, `memory`, `oo7` |
| `eventlog` | `header` < `chain`, `filter`, `traits` < `memory`, `sqlite` |
| `memfiles` | `vault`, `trailer` < `topic` < `memory`, `dirs`, `store` |
| `recall` | `doc`, `vector`, `fuse`, `state`, `embed` < `fts`, `exact`, `fake` < `index` |
| `almanac-service` | `clock`, `auth`, `retention`, `space`, `fact`, `forget`, `consolidation`, `timeline`, `export`, `config`, `settings`, `marks`, `baseline`, `events` < `backend` < `service` < `open`, `record`, `facts`, `search`, `run`, `hunks`, `edits`, `sweep`, `erase`, `control`, `dispatch` |
| `almanac-watch` | `observed` < `join` < `inotify` |
| `almanac-dbus` | `names`, `error`, `record`, `recall`, `control` (skeletons and proxies), `codec` (`request`, `reply`), `invoke`, `serve`, `introspect` |
| `almanac-client` | `transport` < `memory` |
| `almanac-fake` | `clock`, `consolidator`, `fixtures`, `scratch` < `backend` |
| `memoryd` | `xdg`, `clock`, `infer` (`embed`, `consolidate`, `prompt`), `peers`, `signals`, `sandbox`, `settings_watch`, `keysel` < `backend`, `queue` < `daemon` < `main` |

## 3. One home per concept

| Concept | Home |
| --- | --- |
| who acted, effects, labels, ids of sessions and runs, the confirmation receipt | porter `prov` (re-exported by `almanac-core`) |
| `SpaceId`, `AppName`, `AppId`, `Count`, `Bytes`, `UnixSeconds`, `DataClass` | `porter-core` |
| a thing's identity | `prov::EntityId`; `ThingRef`, `ThingKind`, `ThingKey` are aliases in `almanac-core::thing` |
| an event's typed body, its kind tag | `almanac-core::event::EventBody::kind` (the one match) |
| another area's payload | the owner's type; here only `almanac-core::area::AreaPayload` |
| id and path grammars | `almanac-core::ids`, `text` |
| admission, retention defaults, "do not remember" | `almanac-core::admit`, `rules` (the only place) |
| who may ask what | `almanac-service::auth::allowed` (the only place) |
| the wire (requests, replies, refusals) | `almanac-core::{request, reply}` |
| the header's bytes, the chain | `eventlog::header`, `eventlog::chain` |
| keys, derivation contexts, the sealed-file format | `almanac-seal` |
| the topic file and its trailer | `memfiles::topic`, `memfiles::trailer` |
| where files live in a Space | `memfiles::vault` (`VaultPath`) and `almanac-core::dirs` (`Dirs`) |
| search, fusion, chunking, the index state, adopting an index file at start (`Index::sync`) | `recall` |
| the Space, fact, plan and run machines | `almanac-service::{space, fact, forget, consolidation}` |
| topic files edited outside the service (the baseline) | `almanac-service::{baseline, edits}` |
| what the bus hears that no reply says (`ServiceEvent`) | `almanac-service::events`, drained by `Daemon::flush_events` |
| what memoryd may touch (Landlock) | `memoryd::sandbox` |
| the export layout and event lines | `almanac-service::export` |
| the file-why join | `almanac-watch::join` |
| D-Bus names, members, error names | `almanac-dbus::{names, record, recall, control, error}` |
| the system clock, the environment | `memoryd` (`clock.rs`, `xdg.rs`) |

## 4. Traits (the seams) and closed enums

```rust
// almanac-seal: the Secret Service, or MemoryKeys (tests); FileKeys (feature `test-keys`, TEST ONLY).
pub trait KeyStore: Send + Sync {
    fn get(&self, space: &SpaceId) -> impl Future<Output = Result<SpaceKey, KeyError>> + Send;
    fn create(&self, space: &SpaceId) -> impl Future<Output = Result<SpaceKey, KeyError>> + Send;
    fn destroy(&self, space: &SpaceId) -> impl Future<Output = Result<(), KeyError>> + Send;
}

// eventlog: SqliteLog, MemoryLog.
pub trait LogRead {
    fn head(&self) -> Result<Head, LogError>;
    fn checkpoint(&self) -> Result<Checkpoint, LogError>;
    fn page(&self, q: &PageQuery) -> Result<Vec<Entry>, LogError>;
    fn touching(&self, thing: &ThingRef, role: RoleFilter) -> Result<Vec<Seq>, LogError>;
    fn scan(&self, from: Seq) -> Result<Vec<Entry>, LogError>;
}
pub trait LogWrite: LogRead {
    fn append(&mut self, header: NewHeader, body: Option<EventBody>) -> Result<Entry, LogError>;
    fn erase_bodies(&mut self, seqs: &[Seq]) -> Result<Count, LogError>;
    fn prune_before(&mut self, cut: Seq) -> Result<Checkpoint, LogError>;
}

// memfiles: PlainDir, SealedDir, MemoryVault.
pub trait Vault: Send + Sync {
    fn list(&self, dir: &VaultPath) -> Result<Vec<VaultPath>, VaultError>;
    fn read(&self, p: &VaultPath) -> Result<Vec<u8>, VaultError>;
    fn write_atomic(&self, p: &VaultPath, bytes: &[u8]) -> Result<(), VaultError>;
    fn remove(&self, p: &VaultPath) -> Result<(), VaultError>;
}

// recall: FakeEmbedder, FastembedEmbedder, memoryd's InferdEmbedder.
pub trait Embedder: Send + Sync {
    fn card(&self) -> &EmbedderCard;
    /// `role` is Query or Document (asymmetric models); at most `card().max_batch` texts.
    fn embed(&self, texts: &[String], role: EmbedRole, urgency: Urgency)
        -> impl Future<Output = Result<Vec<Vector>, EmbedError>> + Send;
    /// Texts that carry a data class (`Doc.class`, a `ClassTag`); defaults to `embed` without it.
    fn embed_classed(&self, texts: &[Classed], role: EmbedRole, urgency: Urgency) -> impl Future<..> + Send;
    // EmbedderCard { model, dims, max_tokens, max_batch: MaxBatch, prompts: PromptPrefixes, metric }
    // EmbedError::retry_class() -> RetryClass { Retry, Fatal }
}
// recall: ExactScan now; a sqlite-vec backend later (only if the no-unsafe rule is relaxed).
pub trait VectorIndex: Send { fn card(&self) -> &EmbedderCard; fn upsert(..); fn remove(..); fn nearest(..); fn clear(..); }

// almanac-service: ScriptedConsolidator, memoryd's InferdConsolidator; the system or a fixed clock.
pub trait Consolidator: Send + Sync {
    fn draft(&self, input: ConsolidationInput) -> impl Future<Output = Result<Draft, ConsolidateError>> + Send;
}
pub trait Clock: Send + Sync { fn now(&self) -> UnixSeconds; }
// The service's seams, bundled as associated types: FakeBackend, memoryd's SystemBackend.
pub trait Backend: Send + Sync { type Keys; type Log; type Files; type Vectors; type Embedder; type Consolidator; type Clock; /* + random, remove_space, open_log, open_files, open_index */ }

// almanac-watch: InotifyWatch (feature `linux`; fanotify later).
pub trait FileWatch: Send { fn watch(..); fn unwatch(..); fn next(&mut self) -> impl Future<Output = Option<Observed>> + Send; }

// almanac-client: InProcess (feature in_process), Absent, DbusTransport.
pub trait Transport: Send + Sync { fn call(&self, request: MemoryRequest) -> impl Future<Output = Result<MemoryReply, TransportError>> + Send; }
```

Closed sets stay enums: `EventBody`, `MemoryOp`, `FileChange`, `RuleScope`, `RememberMode`,
`Retention`, `Admission`, `DropReason`, `MemoryRequest`/`MemoryReply`/`Refusal`, `Caller`,
`Link`, `FactState`, `Settlement`, `Hunk`, `RunState`, `SpaceState`, `IndexView`, `Break`,
`ChainReport`, `ExportedBody`, memoryd's `SpaceVault`.

## 5. What is frozen, what is built

Frozen means: the types, trait signatures, file formats and D-Bus signatures below are the
interface other work builds on; a change is a format bump (section 6) or a SPEC edit. Every
`todo!()` of the freeze is filled (fill waves 1 to 3); nothing is stubbed.

| Piece | State |
| --- | --- |
| ids, grammars, `UserText`, wire and stored forms, kind tags, `Dirs` layout | built; every variant round-trip and JSON pinned tests |
| `admit`, `default_retention`, globs, `allowed` | built, table-tested |
| event header canonical bytes, `link`, genesis, keyed body digest, `verify_chain` | built; golden bytes |
| `MemoryLog`, `SqliteLog` | built; one contract, `SqliteLog` also over a file (SQLCipher, keyed digest checked on append) |
| key derivation (contexts pinned), `seal`/`unseal`, `DbKey`, `MemoryKeys`, `Oo7Keys` | built; golden derivation |
| topic file format, trailer grammar, `parse_topic`/`render_topic`, `VaultPath`, `MemoryVault`, `PlainDir`, `SealedDir`, `Store`, `Primer` | built; golden file |
| `fuse_rrf`, `chunk`, `nearest_exact`, the vector BLOB, `Fts5`, `ExactScan`, `Index` (rebuild, upsert, `sync` that adopts a file, per-class refusal), `FakeEmbedder`, `FastembedEmbedder` | built, tested |
| the Space, fact, plan and consolidation machines, `Plan::digest`, retention, timeline rows | built, table-tested |
| every hunk (Promote, Supersede, Tidy, Stamp, ExternalEdit, Flag), revert, the retention sweep, marks, use counts | built; `almanac-fake/tests/hunks.rs` |
| the export writer, `EventLine`, config files (`memory.toml`, `spaces.toml`) | built; golden tar layout and event lines |
| `plan_forget`, `check_draft`, `MemoryService` | built |
| the file-why `join`, `InotifyWatch` | built |
| D-Bus skeletons, proxies, `dbus/org.quire.Memory1.xml`, `MemoryError` | frozen, introspection tested (the skeletons and the served objects both) |
| D-Bus codec, `invoke`, the served objects, `DbusTransport` | built; `almanac-dbus/tests/codec.rs` (every wire sample, both directions) |
| `Memory`, `Absent`, `InProcess` | built, tested |
| `fake_service`, fixtures, scratch dirs | built |
| `SystemBackend`, `SystemClock`, XDG roots, `InferdEmbedder`, `InferdConsolidator`, `inferd_link` | built; the models are tested over a scripted inferd session and over a fake inferd on a private bus |
| `Serialised`, `Peers`, `Daemon`, the `memoryd` binary | built; `memoryd/tests/bus.rs` is the end-to-end test on a private bus |
| the baseline and `ExternalEdit` hunks, topic text in `ConsolidationInput`, flags in `FactView`, `ServiceEvent`, `check_keys`, the Landlock sandbox, adopting an index at start | built; `almanac-fake/tests/edits.rs`, `recall/tests/adopt.rs`, `memoryd/tests/{restart,sandbox,bus}.rs` |

## 6. File formats (frozen)

**Layout** (all paths from the injected `Dirs`; the data root is `$XDG_DATA_HOME/quire/memory`):

```
spaces.toml                         SpacesFile: one SpaceMeta per Space (id, created, replica, vault, format)
system/events.db                    desktop-level log: Space created or deleted, checkpoint anchors
<space>/events.db (+ -wal)          eventlog, SQLCipher, key = derive(space key, Eventlog)
<space>/facts/INDEX.md              the primer, regenerated by consolidation, at most 200 lines
<space>/facts/<topic>.md            topic files (sealed or plain per vault)
<space>/pending/<fact-id>.md        facts derived from untrusted text, awaiting the person
<space>/procedures/<app>/<name>.md  CUA procedures (the cua area's format; the same trailer)
<space>/consolidation/<run>.toml    one run: its hunks, skipped hunks, cut and outcome (proposed, applied, reverted, discarded, superseded); written when drafted, rewritten on each outcome, never deleted
<space>/meta/marks.json             "do not remember" marks (vault file)
<space>/meta/baseline.json          each topic file's text as the service last left it (an edit is a difference)
<space>/flags/<run>.json            the notes a run's Flag hunks left, by fact
$XDG_CACHE_HOME/quire/memory/<space>/index.db   recall index, SQLCipher, deletable; meta.space records the vector space it holds, so a restart adopts it
$XDG_CONFIG_HOME/quire/memory.toml              RuleSet and defaults (written only by memoryd)
$XDG_RUNTIME_DIR/quire/memory/edit/<space>/     decrypted edit copies on tmpfs
```

**Event header (v1)**: `"QEL1" | seq u64be | replica [16] | occurred i64be | recorded i64be |
lp(actor json) | lp(kind tag) | effect u8 | lp(label json) | lp(cause json) | body_digest [32] |
prev_link [32]`, `lp(x) = u32be length ‖ bytes`; `link = blake3(header bytes)`; genesis
`prev = blake3("QEL1 genesis" ‖ space ‖ replica)`; `body_digest = blake3::keyed_hash(derive(key,
Digest), body json)`. The header holds no thing id and no text. Golden:
`crates/eventlog/tests/golden/header_v1.{hex,link}`.

**Sealed file**: `"QMEM" 0x01 ‖ nonce [24] ‖ XChaCha20-Poly1305`, associated data
`lp(space) ‖ lp(vault path)`; key `derive(key, Files)`; subkey contexts `"quire-memory 1
<eventlog|index|files|digest>"`. Golden: `crates/almanac-seal/tests/golden/derive.txt`.

**Topic file**: front matter (`format: quire-memory 1`, `topic`, `title`), dated headings derived
from each fact's `at` in the injected time zone, one `- text` bullet per fact followed by its
trailer `  <!-- fact: <id>; at: <utc>; by: <json>; label: <json>[; from: <json>][; supersedes:
<json>] -->`. A bullet with no trailer is the person's edit (`Unstamped`); other lines are
`Verbatim`; `valid` and `origin` are reserved. `render(parse(s)) == s` for any `s` that render
produced. Golden: `crates/memfiles/tests/golden/people-sam-lee.md`.

**Export**: a tar stream under `quire-memory-export-1/`: `manifest.json`, `<space>/events.jsonl`
(one `EventLine` per entry: header fields, link, body or `"erased"`), `<space>/facts|procedures|
pending/**.md` plain, `rules.toml`. Never the index, never the keys. Golden:
`crates/almanac-service/tests/golden/events.jsonl` and the layout test.

**D-Bus** `org.quire.Memory1` at `/org/quire/Memory1`: interfaces `.Record`, `.Recall`,
`.Control` (28 methods, 5 signals, property `Version u = 1`); bodies are the serde JSON of
`almanac-core` types in `s` arguments; errors `org.quire.Memory1.Error.<Refusal variant>`.
`dbus/org.quire.Memory1.xml` is checked against the skeletons by `almanac-dbus`'s
introspection test; the failure prints the new text.

## 7. Recipes

**Add an event body variant** (a format change when it alters kind tags): its variant in
`EventBody` and its arm in `kind()` and `things()`; a sample in `tests/wire.rs` (the exhaustive
index function breaks until it has one); its row in `allowed` (who may record it); its default
retention in `RuleSet::standard`; a fixture in `almanac-fake` if specs name it. Another area's
payload is never a variant: it is an `AreaPayload` with its owner's `KindTag`. The two
exceptions are types almanac may name because they live in `prov` or in almanac itself:
`Message` (prov's one message model) and `Episode`. Their text is a pure function of the body
(`EventBody::index_texts`), so the index stays rebuildable from the log.

**Messages and episodes in the log.** `Record.actor` is the stamped sender and `Record.label` is
the message's own label (for an episode, the join of its parts'); only the router records them,
except that cuad may record a run's own `Message` (the run is the sender). Both are audit
class: a pause or a `Never` rule keeps the header and drops the body, and retention is
`companion.*`, 30 days. Recall indexes a message as one document (`e:<replica>:<seq>`, facet
`message`) and an episode as up to two: its trusted skeleton (`e:`, facet `episode`) and its
narrative (`n:`, facet `narrative`), each with its own label, so a tainted narrative never
taints the skeleton's hit. A narrated episode is a second `Episode` event with the same id and
skeleton (`Episode::narrates`); recall indexes only the newest event per id. A message carries
no authority and grants no read in another Space: `Search`, `Inject` and `Recent` take the
Space of the invocation.

**The router's reads for the working set.** `Inject(InjectQuery)` is automatic top-k recall cut
to a token budget (answered with `Hits`, ranked, labelled; `fit_budget` is the arithmetic) and
`Recent(SpaceId, RecentQuery)` is recent activity, newest first, with labels and text
(`Recent` reply); `RecentQuery.bodies: BodyMode { Without, Json }` adds `RecentEntry.body` (the owner's
serde JSON), which always travels in the entry beside its label, so a daemon can rebuild from the
log on restart. Both are `Router` or `ShellUi`, both audited as `Memory.Read`
(`ReadScope::Inject`, `Recent`); `Timeline` stays the shell's alone.

**A durable, resumable stream (the session log).** A writer that rebuilds its state from the log
(docket's `companion.session.*` entries) uses three additions to the wire. `Entries(SpaceId,
EntriesQuery)` pages one stream oldest first: `kinds` (patterns, so a prefix is `companion.session.*`),
`about: Option<ThingRef>` (events whose body names that thing as `Subject`: the writer names its
stream, for example the session, as the Subject of every payload it records), `after: Option<Cursor>`
(strictly after that event), `limit` and `bodies`; the reply is `EntriesPage { entries, next }` with
`next` set only when more matched, so a reader resumes by passing `next` as `after`, or the last
entry's `summary.event.seq` once it caught up. `RecordDurable(Record)` answers `Durable(Ack { event })`
only after the log commit (`SqliteLog` runs `synchronous=FULL`), never buffers a locked Space and
never drops silently: refusals are `SpaceLocked`, `SpaceFull`, `Unavailable` and `NotKept(DropReason)`
(paused, a `Never` rule, a marked thing). `event.seq` is the Space log's sequence number, strictly
increasing; memoryd's own audit events take numbers between a writer's. `Recallable::{Yes, No}`
(`almanac-core::recallable`) is a rule over the kind: `companion.session.*` is `No`, which keeps
the entry out of the index (search, `Inject`), consolidation input, `Related` and so out of the primer
and every fact; `Recent`, `Entries`, `Timeline` and export return it. The member names on the bus are
`Record.RecordDurable` and `Recall.Entries`; `Memory::record_durable` and `Memory::entries` are the client.

**Add a rule scope**: its variant in `RuleScope` with a `specificity` (the order is
`Thing > Path > Kind > App > Actor > Space`), its arm in `admit::applies`, a row in
`admit_table` and the round-trip sample; `memory.toml` needs no change (it is serde).

**Add a request**: the variant in `MemoryRequest` and its reply in `MemoryReply`; its arm in
`MemoryRequest::space`, in `allowed` (the match is exhaustive) and in `MemoryService::handle`;
the D-Bus member in `almanac-dbus` (proxy, skeleton and served object), then regenerate and review
`dbus/org.quire.Memory1.xml`; a method on `Memory`; the arms of `encode_request`, `decode_request`,
`decode_reply`, `encode_reply` and `invoke`; the sample in `almanac-core/tests/common/samples.rs`
(the wire and codec tests read it); its queue claim in `memoryd::claim_of`.

**Add a vector backend**: a `VectorIndex` implementation in its own crate (never in `recall`
if it needs `unsafe` or an extension: the rule is no `unsafe` anywhere); the contract tests of
`recall/tests/recall.rs`; a `Backend::Vectors` choice in memoryd.

**Add a vault**: a `Vault` implementation, the `vault_contract` test over it, a variant in
memoryd's `SpaceVault` and in `VaultKind`.

**Add a state machine step**: a row in that machine's table test first (`machines.rs`); the
effects are values, so the service applies them and the table needs no I/O.

**Change a stored format**: a new version in the format's magic or `format:` line, a migration
in the opener, the golden updated in the same commit (review the diff of the golden).

## 8. Test harness

`almanac-fake` is the harness: `fake_service(ScriptedConsolidator::default())` builds a
`MemoryService` over `MemoryKeys`, `MemoryLog`, `MemoryVault`, an `Index<ExactScan>` over
in-memory SQLite, `FakeEmbedder` and `FixedClock(NOW)`; `Scratch` gives scratch XDG roots;
`mail_thread_archived`, `file_saved_from_attachment`, `companion_forwarded`, `cua_run_step` and
`policy_ask` are the fixtures; `FakeBackend::vault_of` and `advance_clock` let a test edit files
behind the service's back and move time. Tests never touch the real session bus, a keyring, the
network or the real XDG directories: `memoryd/tests/common` starts a private `dbus-daemon` on a
socket in a scratch directory, scratch XDG roots and an in-memory key store, and
`memoryd/tests/bus.rs` calls the daemon through `almanac-client`'s `DbusTransport`.
`almanac-fake/tests/service.rs` and `almanac-client/tests/memory.rs` are the model for tests over
the fakes.

## 9. Repo rules

- **Gate** (check every exit code; `recall-fastembed` is excluded because ort downloads binaries):

  ```bash
  set -euo pipefail
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features --exclude recall-fastembed -- -D warnings
  cargo test --workspace --all-features --exclude recall-fastembed
  ./scripts/check-boundary.sh
  cargo deny check licenses
  echo "GATE GREEN"
  ```

  A network-allowed dev check, run by hand: `cargo check -p recall-fastembed`.
- **No `unsafe`** anywhere (`unsafe_code = "deny"`). That is why vectors are exact scan and not
  `sqlite-vec` (QUESTIONS P5).
- **Dependencies** come from quire's pinned block (`docs/workspace-deps.toml` there), copied
  verbatim, only the lines almanac names; a new one joins that file first. porter's crates are
  sibling path dependencies until the pinned git revs of fill wave 1.
- **The wire is serde.** Every stored or wire type has a round-trip test; enums with data are
  adjacently tagged (`kind`/`v`).
- **D-Bus signatures change with their XML.** `almanac-dbus/tests/introspection.rs` fails until
  `dbus/org.quire.Memory1.xml` equals the skeletons' introspection, and `tests/served.rs` until
  the served objects (what memoryd runs) introspect the same.
- **Keys and bodies never cross a transport in the clear.** No wire type holds a `SpaceKey`;
  the digest subkey leaves only in an export the person asked to include it in.
- **Floats** appear only in `recall::Vector` (embeddings are floats end to end).
- **Physical erasure is a key operation.** Forgetting one item erases bodies and uses SQLite
  `secure_delete`; only destroying a Space's key guarantees the bytes are gone on
  copy-on-write storage, and the UI says so.
