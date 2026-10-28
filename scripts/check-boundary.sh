#!/usr/bin/env bash
# Crate layers, mechanically enforced (ARCHITECTURE.md section 1 is the table; this is its
# mechanical form).
#
#   scripts/check-boundary.sh               run every check
#   scripts/check-boundary.sh --long-files  also report source files over 500 lines (no failure yet)
#   scripts/check-boundary.sh --self-test   seed violations and assert each is caught
#
# Every crate is one tier:
#   P  pure: no std::fs/env/process/net, no clock reads, no sleeps, no tokio, hyper, reqwest, zbus
#      or rustix, and no UI words.
#   C  client/IO: may do IO, but has no UI words (Help, Sheet, SettingRow, ds_settings).
#   A  app/daemon: wiring only. Its libraries read no environment variable outside main.rs.
# A crate depends on its own tier or a lower one (P < C < A), never up.
#
# `Label` is not a UI word here: prov::Label is the taint label, not copy shown to a person.
#
# `cargo tree -i <dep>` exits 101 when the dependency is absent, which is precisely the state
# we want. Checking the exit status would therefore fail whenever the boundary holds, so we
# check for OUTPUT instead: any line naming the dependency is a leak.
set -uo pipefail
cd "$(dirname "$0")/.."

# LAYERS: crate -> tier. Every workspace member has exactly one row.
LAYERS=(
  "almanac-core: P"
  "almanac-store: P"
  "memfiles-format: P"
  "almanac-settings: P"
  "almanac-tidy: P"
  "almanac-search: P"
  "almanac-service: P"
  "almanac-seal: P"
  "almanac-keyfile: C"
  "almanac-host: C"
  "eventlog: C"
  "memfiles: C"
  "recall: C"
  "almanac-watch: C"
  "almanac-infer: C"
  "almanac-dbus: C"
  "almanac-dbus-serve: C"
  "almanac-client: C"
  "almanac-fake: C"
  "almanac-local: C"
  "memoryd: A"
)

# KNOWN: a violation that exists today, with the plan lane (~/rs-wt/agent-layers/PLAN.md) that
# removes it. "crate: ban: lane". A row whose violation is gone fails, so the list only shrinks.
# (none today)
KNOWN=()

# BANS: "name|tiers|extended regex" over the non-test source of a crate of those tiers. A line
# that starts with `//` is skipped, and so is everything from the first `#[cfg(test)]` on.
BANS=(
  'fs|P|std::(fs|\{[^}]*\bfs\b)'
  'env|P|std::(env|\{[^}]*\benv\b)'
  'process|P|std::(process|\{[^}]*\bprocess\b)'
  'net|P|std::(net|os::unix::net|\{[^}]*\bnet\b)'
  'clock|P|SystemTime::now|Instant::now'
  'sleep|P|thread::sleep'
  'tokio|P|\btokio\b'
  'hyper|P|\bhyper\b'
  'reqwest|P|\breqwest\b'
  'zbus|P|\bzbus\b'
  'rustix|P|\brustix\b'
  'ui-words|PC|\b(Help|Sheet|SettingRow)\b|ds[-_]settings'
  # Outside main.rs and the src/bin programs; the environment is read once, at the entry point.
  'env-var|A|\benv::(var|var_os|vars|vars_os)\b'
)

# RULES: what a crate reaches through ANY path (transitive, default features). The portable
# crates never reach a bus, a runtime, an HTTP client, a keyring, an ONNX runtime or an
# embedding library. rusqlite is allowed to eventlog, recall and everything above them (it is
# their storage), but never to almanac-core, almanac-seal, memfiles or memfiles-format; notify only to
# almanac-watch (the Space roots) and memoryd (the directory watch on the person's settings file). almanac-seal reaches oo7 only through its `oo7` feature, almanac-client reaches
# zbus only through its `dbus` feature, and links no rusqlite or openssl-sys (SQLCipher, vendored
# OpenSSL) even with its `in_process` feature (the service is generic over the store seams), and almanac-dbus and almanac-dbus-serve reach tokio only through zbus's
# `tokio` feature. almanac-store (the seam traits and types the service stands on) and
# almanac-service reach no storage at all: no rusqlite, no SQLCipher (libsqlite3-sys), no OpenSSL,
# no runtime, no bus. memoryd is the place that reaches the daemon's runtime, so it has no rule.
EFFECTS="zbus zvariant tokio reqwest hyper oo7 ort fastembed"
# almanac-core is also light enough for cua-bus to depend on (interface ask 26: `HandedBack`
# carries `almanac_core::EventRef`; cua's own EFFECTS list adds these to ours), so it reaches none
# of them either.
CUA_EFFECTS="hyper-util rustls pipewire wayland-client wayland-backend wayland-server reis atspi rmcp cedar-policy"
# What a storage-free crate must never link: the SQL engine, SQLCipher's host crate and OpenSSL.
# atomic-write-file is the file-replacing writer memoryd's FileLedger uses; the service keeps the
# removal notes behind a seam instead (RemovalLedger), so it never reaches a file writer.
STORAGE="rusqlite libsqlite3-sys openssl-sys openssl atomic-write-file"
RULES=(
  "almanac-core: $EFFECTS rusqlite notify toml $CUA_EFFECTS"
  "almanac-seal: $EFFECTS rusqlite notify"
  "almanac-keyfile: $EFFECTS rusqlite notify"
  "almanac-store: $EFFECTS $STORAGE notify"
  "eventlog: $EFFECTS notify"
  "memfiles: $EFFECTS rusqlite notify"
  "memfiles-format: $EFFECTS $STORAGE notify"
  "recall: $EFFECTS notify"
  "almanac-settings: $EFFECTS $STORAGE notify"
  "almanac-tidy: $EFFECTS $STORAGE notify"
  "almanac-search: $EFFECTS $STORAGE notify"
  "almanac-service: $EFFECTS $STORAGE notify"
  "almanac-host: $EFFECTS $STORAGE notify"
  "almanac-watch: $EFFECTS rusqlite"
  # The inferd adapters reach inferd through porter-client's Transport only: no bus, no HTTP, no
  # keyring, no embedding runtime and no storage, so an app other than memoryd can use them.
  "almanac-infer: zbus zvariant reqwest hyper oo7 ort fastembed $STORAGE notify"
  "almanac-dbus: reqwest hyper oo7 ort fastembed rusqlite notify"
  "almanac-dbus-serve: reqwest hyper oo7 ort fastembed rusqlite notify"
  "almanac-client: $EFFECTS rusqlite openssl-sys notify"
  "almanac-fake: $EFFECTS notify"
  "almanac-local: $EFFECTS notify"
  # The daemon reaches the bus, a runtime, the keyring and the file watch. It never reaches HTTP,
  # an embedding runtime, the MCP SDK or the policy engine (inferd is the model, over the bus).
  "memoryd: reqwest hyper ort fastembed rmcp cedar-policy"
)

tier_of() {
  local row
  for row in "${LAYERS[@]}"; do
    [ "${row%%:*}" = "$1" ] && { echo "${row#*: }"; return; }
  done
}
rank() { case "$1" in P) echo 0 ;; C) echo 1 ;; A) echo 2 ;; *) echo 9 ;; esac; }

# Prints "file:line: text" for each non-test source line of crate $2 under root $1 that matches
# regex $3. A file whose path matches the regex $4 is skipped.
scan_source() {
  local root="$1" crate="$2" regex="$3" skip="${4:-^$}" file
  [ -d "$root/crates/$crate/src" ] || return 0
  while IFS= read -r file; do
    [[ "$file" =~ $skip ]] && continue
    sed '/^[[:space:]]*#\[cfg(test)\]/q' "$file" | grep -nE "$regex" \
      | grep -vE '^[0-9]+:[[:space:]]*//' | sed "s#^#$file:#"
  done < <(find "$root/crates/$crate/src" -name '*.rs' ! -name tests.rs ! -path '*/tests/*' | sort)
}

# Runs every source ban on crate $2 under root $1; prints "ban: hit" lines.
source_violations() {
  local root="$1" crate="$2" tier spec name tiers regex skip
  tier=$(tier_of "$crate")
  for spec in "${BANS[@]}"; do
    name="${spec%%|*}"; spec="${spec#*|}"
    tiers="${spec%%|*}"; regex="${spec#*|}"
    case "$tiers" in *"$tier"*) ;; *) continue ;; esac
    skip='^$'
    [ "$name" = env-var ] && skip='/(main\.rs|bin/[^/]*)$'
    scan_source "$root" "$crate" "$regex" "$skip" | sed "s#^#$name: #"
  done
}

known_lane() { # crate ban -> lane, empty when not listed
  local row
  for row in "${KNOWN[@]}"; do
    case "$row" in "$1: $2: "*) echo "${row##*: }"; return ;; esac
  done
}

if [ "${1:-}" = "--self-test" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  LAYERS=("p-crate: P" "c-crate: C" "a-crate: A")
  for c in p-crate c-crate a-crate; do mkdir -p "$tmp/crates/$c/src"; done
  printf 'pub fn f() { let _ = std::fs::read("x"); }\n' >"$tmp/crates/p-crate/src/lib.rs"
  printf 'pub fn f() { let _ = std::fs::read("x"); }\npub struct SettingRow;\n' >"$tmp/crates/c-crate/src/lib.rs"
  printf 'pub fn f() { let _ = std::env::var("X"); }\n' >"$tmp/crates/a-crate/src/lib.rs"
  printf 'fn main() { let _ = std::env::var("X"); }\n' >"$tmp/crates/a-crate/src/main.rs"
  p_hits=$(source_violations "$tmp" p-crate)
  c_hits=$(source_violations "$tmp" c-crate)
  a_hits=$(source_violations "$tmp" a-crate)
  grep -q '^fs: ' <<<"$p_hits" || { echo "SELF-TEST FAILED: a std::fs call in a P crate was not caught"; exit 1; }
  grep -q '^fs: ' <<<"$c_hits" && { echo "SELF-TEST FAILED: a std::fs call in a C crate was reported"; exit 1; }
  grep -q '^ui-words: ' <<<"$c_hits" || { echo "SELF-TEST FAILED: a UI word in a C crate was not caught"; exit 1; }
  grep -q 'lib.rs:.*env-var\|^env-var: .*lib.rs' <<<"$a_hits" || { echo "SELF-TEST FAILED: an env read in an A library was not caught"; exit 1; }
  grep -q 'main.rs' <<<"$a_hits" && { echo "SELF-TEST FAILED: an env read in main.rs was reported"; exit 1; }
  echo "self-test ok: P bans fire, C allows IO but not UI words, A libraries read no env outside main.rs"
  exit 0
fi

fail=0
long_files=0
[ "${1:-}" = "--long-files" ] && long_files=1

members=$(sed -n '/^members = \[/,/^\]/s#^  "crates/\(.*\)",$#\1#p' Cargo.toml)

# Every workspace member has a row in LAYERS and in RULES, and every row names a crate, so a new
# crate cannot slip in unchecked.
layer_rows=$(printf '%s\n' "${LAYERS[@]}" | grep -v '^#')
for crate in $members; do
  grep -q "^$crate: [PCA]\$" <<<"$layer_rows" || { echo "ERROR: $crate has no row in LAYERS"; fail=1; }
  printf '%s\n' "${RULES[@]}" | grep -q "^$crate:" || { echo "ERROR: $crate has no row in RULES"; fail=1; }
done
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  [ -f "crates/$crate/Cargo.toml" ] || { echo "ERROR: LAYERS names $crate, which has no crates/$crate/Cargo.toml"; fail=1; }
done

# Dependencies point down only: a crate's DIRECT normal and build path dependencies (all
# features, so almanac-client's `in_process` and `dbus` both count) are of its own tier or a
# lower one. Dev dependencies are outside it. A dependency from another repo (porter, prov) has
# no row here.
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  tier=$(tier_of "$crate")
  deps=$(cargo tree -p "$crate" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
    | grep -E '\((/|https://github.com/PoHsuanLai/)' | awk '{print $1}' | grep -vx "$crate" | sort -u)
  bad=0
  for dep in $deps; do
    dep_tier=$(tier_of "$dep")
    [ -z "$dep_tier" ] && continue
    if [ "$(rank "$dep_tier")" -gt "$(rank "$tier")" ]; then
      echo "LAYER: $crate ($tier) depends on $dep ($dep_tier), which is above it"; bad=1; fail=1
    fi
  done
  [ "$bad" -eq 0 ] && echo "layers hold: $crate ($tier) depends only on its own tier or below"
done

# Source bans, with the KNOWN list.
for row in "${LAYERS[@]}"; do
  crate="${row%%:*}"
  hits=$(source_violations . "$crate")
  seen=""
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    ban="${hit%%: *}"
    lane=$(known_lane "$crate" "$ban")
    if [ -n "$lane" ]; then
      echo "KNOWN: $crate ($ban) is removed by lane $lane: ${hit#*: }"
      seen="$seen $ban"
    else
      echo "BAN: $crate ($(tier_of "$crate")): $hit"
      fail=1
    fi
  done <<<"$hits"
  for k in "${KNOWN[@]}"; do
    [ "${k%%:*}" = "$crate" ] || continue
    k_ban=$(cut -d: -f2 <<<"$k" | tr -d ' ')
    grep -qw -- "$k_ban" <<<"$seen" || { echo "STALE: KNOWN names $k_ban in $crate, which no longer breaks it; delete the row"; fail=1; }
  done
  [ -z "$hits" ] && echo "bans hold: $crate ($(tier_of "$crate")) source is clean"
done

# Source files over 500 lines: reported on request, not a failure yet.
if [ "$long_files" -eq 1 ]; then
  find crates -name '*.rs' -path '*/src/*' -not -path '*/target/*' -print0 \
    | xargs -0 wc -l | awk '$1 > 500 && $2 != "total" {print "LONG: " $2 " has " $1 " lines"}'
fi

for rule in "${RULES[@]}"; do
  crate="${rule%%:*}"
  read -r -a forbidden <<<"${rule#*:}"
  # A crate that cargo cannot find would make every check below pass vacuously.
  if ! cargo tree -p "$crate" --depth 0 >/dev/null 2>&1; then
    echo "ERROR: cargo tree cannot resolve $crate; the boundary was not checked"
    fail=1
    continue
  fi
  # almanac-client's default feature (`quire-desktop`, the app-level desktop switch) turns `dbus` on;
  # the rule is about what an app without it links, hosting the service in process, so that crate is
  # checked without defaults and with `in_process`.
  defaults=()
  [ "$crate" = almanac-client ] && defaults=(--no-default-features --features in_process)
  leaked=0
  for dep in "${forbidden[@]}"; do
    if cargo tree -p "$crate" "${defaults[@]}" -i "$dep" -e normal,build 2>/dev/null | grep -q .; then
      echo "LEAK: $crate depends on $dep"
      cargo tree -p "$crate" "${defaults[@]}" -i "$dep" -e normal,build 2>/dev/null | head -20
      leaked=1
      fail=1
    fi
  done
  if [ "$leaked" -eq 0 ]; then
    echo "boundary holds: $crate reaches none of ${forbidden[*]}"
  fi
done

# Nothing of almanac's reaches another area's repo.
for banned in docket-core docket-router intentd cua-run cua-bus cuad companion-wire; do
  if cargo tree --workspace -i "$banned" -e normal,build,dev 2>/dev/null | grep -q .; then
    echo "LEAK: the workspace depends on $banned (almanac stores other areas' payloads opaquely)"
    fail=1
  fi
done
# model-provider comes in under porter-infer (a reply's `charge` is its `Money`; owner 2026-10-28),
# so only a direct edge from one of almanac's own crates is a leak: almanac never names a model payload.
if cargo tree --workspace -i model-provider -e normal,build,dev --depth 1 2>/dev/null | grep -q "($PWD/"; then
  echo "LEAK: an almanac crate depends on model-provider directly (almanac stores model payloads opaquely)"
  fail=1
fi

# The portable core (ARCHITECTURE.md section 1a) is checked by its own script: no-default-features
# builds, no platform dependency in the tree, cross-target checks. It needs no network.
./scripts/check-portable.sh || fail=1

# The test-only features are never on in a default build (a dist build uses default features):
# memoryd's `test-keys` and `test-proc-root`; `test-keys` is what brings almanac-keyfile in.
enabled=$(cargo tree -p memoryd -e normal,build -f '{p} [{f}]' --prefix none 2>/dev/null \
  | grep -E '^memoryd ' | grep -oE '\[[^]]*\]' | tr ',[]' '\n\n\n')
if printf '%s\n' "$enabled" | grep -qE '^test-'; then
  echo "TEST FEATURE: a default build of memoryd enables: $(printf '%s\n' "$enabled" | grep -E '^test-' | tr '\n' ' ')"
  fail=1
else
  echo "test features (test-keys, test-proc-root) are off in a default build of memoryd"
fi

if cargo tree -p memoryd -e normal,build --prefix none 2>/dev/null | grep -q '^almanac-keyfile '; then
  echo "TEST FEATURE: a default build of memoryd links almanac-keyfile (FileKeys, TEST ONLY)"
  fail=1
fi

exit "$fail"
