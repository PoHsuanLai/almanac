# Conventions

almanac follows the program's shared conventions, whose text lives in quire's `CONVENTIONS.md`
(identical in quire, sill, shell-host, palmrest and detent): types, traits, effects, errors,
tests, comments, change discipline, borrowing. Read it first. `ARCHITECTURE.md` here adds the
crate map, the one-home table, the traits, the recipes and the repo rules; where the two
disagree, `ARCHITECTURE.md` wins for almanac. porter's `CONVENTIONS.md` is the model this one
copies; the rules below are the same ones with almanac's reasons.

What almanac adds or decides differently, each with its reason:

1. **Closed sets are enums without `Word`.** almanac does not depend on quire's `ds-core`:
   memory must build for mailo on macOS and Windows and for other desktops, below the design
   system. A unit enum's stable slug is its serde `snake_case` form (the same slug in files, on
   the bus and in kind tags): the `slug_enum!` macro in `almanac-core` declares the enum, its
   serde names and `slug()`/`ALL` from one list, so they cannot differ. Enums with data
   (`FileChange`, `MemoryOp`) keep one `slug()` match beside the type. Labels a person reads
   belong to the UI that draws them (sill, detent): memoryd never sends prose.
2. **Async seams use `-> impl Future<Output = ...> + Send`** (return-position `impl Trait`),
   so implementations write `async fn` and every future can cross a multi-threaded runtime.
   Closed sets of implementations are enums implementing the trait (memoryd's `SpaceVault`),
   or a `Backend` trait's associated types; never `dyn`.
3. **`todo!()` bodies exist only while an interface is frozen and its behaviour is not
   built.** Each one is listed in `FINDINGS.md` with the work that removes it. This departs
   from the shared rule "no `todo!()` stub on master"; the freeze decides when it lands.
4. **Nothing ambient below the daemons.** The clock (`almanac_service::Clock`), the key store
   (`KeyStore`), the vault (`Vault`), the embedder, the consolidator, the nonce
   (`almanac_seal::Nonce`), the random bits of a fact id and every directory (`Dirs`) are passed
   in; only `memoryd` reads the system clock, the environment or the bus. Tests never touch
   a bus, a keyring, the network or the real XDG directories: scratch directories
   (`almanac-fake::Scratch`) and in-memory seams.
5. **Everything stored or sent is a pinned form.** A format (the event header bytes, the
   sealed file, the topic file and its trailer, the export tar, the key derivation contexts,
   the plan digest) has a golden test; changing one is a format bump, not a refactor.
6. **User text never shows in `Debug`.** `UserText`, `FactText`, `ThingView` and key types
   redact; tests assert it.
7. **No `bool` in state.** A field that says "yes or no" is an enum (`Lock`, `Desktop`, `Power`,
   `SourceState`, `VerificationKey`); two-state machines are enums with names.
8. **One integration-test executable per crate.** Cargo links every file directly under `tests/`
   into its own executable, and each one statically links the crate's whole dependency graph
   (SQLCipher, zbus, tokio and the rest), so a build directory grows with the number of files.
   Integration tests are therefore modules of `tests/it/main.rs` (`mod <topic>;`), shared
   helpers are `tests/it/support/` modules (`use crate::support::...`), and goldens stay in
   `tests/golden/`. A separate target (`tests/<name>.rs` plus `[[test]]` in the crate's
   `Cargo.toml`, with a comment in `tests/it/main.rs`) needs a stated reason: it changes the
   environment (`set_var`, a panic hook, the current dir), holds a process-wide singleton the
   others must not share, needs its own `required-features`, or has `harness = false`. A new
   `tests/*.rs` without that reason is a mistake. Dependencies build without debug info
   (`[profile.dev.package."*"]` in `.cargo/config.toml`).
