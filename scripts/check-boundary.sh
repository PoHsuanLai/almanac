#!/usr/bin/env bash
# Crate boundaries, mechanically enforced (ARCHITECTURE.md section 1 is the table; this is its
# mechanical form).
#
# `cargo tree -i <dep>` exits 101 when the dependency is absent, which is precisely the state
# we want. Checking the exit status would therefore fail whenever the boundary holds, so we
# check for OUTPUT instead: any line naming the dependency is a leak.
set -uo pipefail
cd "$(dirname "$0")/.."

# RULES: what a crate reaches through ANY path (transitive, default features). The portable
# crates never reach a bus, a runtime, an HTTP client, a keyring, an ONNX runtime or an
# embedding library. rusqlite is allowed to eventlog, recall and everything above them (it is
# their storage), but never to almanac-core, almanac-seal or memfiles; notify only to
# almanac-watch (the Space roots) and memoryd (the directory watch on the person's settings file). almanac-seal reaches oo7 only through its `oo7` feature, almanac-client reaches
# zbus only through its `dbus` feature, and links no rusqlite or openssl-sys (SQLCipher, vendored
# OpenSSL) even with its `in_process` feature (the service is generic over the store seams), and almanac-dbus reaches tokio only through zbus's
# `tokio` feature. almanac-store (the seam traits and types the service stands on) and
# almanac-service reach no storage at all: no rusqlite, no SQLCipher (libsqlite3-sys), no OpenSSL,
# no runtime, no bus. recall-fastembed and memoryd are the places that reach fastembed and the
# daemon's runtime, so they have no rule.
EFFECTS="zbus zvariant tokio reqwest hyper oo7 ort fastembed"
# almanac-core is also light enough for cua-bus to depend on (interface ask 26: `HandedBack`
# carries `almanac_core::EventRef`; cua's own EFFECTS list adds these to ours), so it reaches none
# of them either.
CUA_EFFECTS="hyper-util rustls pipewire wayland-client wayland-backend wayland-server reis atspi rmcp cedar-policy"
# What a storage-free crate must never link: the SQL engine, SQLCipher's host crate and OpenSSL.
STORAGE="rusqlite libsqlite3-sys openssl-sys openssl"
RULES=(
  "almanac-core: $EFFECTS rusqlite notify toml $CUA_EFFECTS"
  "almanac-seal: $EFFECTS rusqlite notify"
  "almanac-store: $EFFECTS $STORAGE notify"
  "eventlog: $EFFECTS notify"
  "memfiles: $EFFECTS rusqlite notify"
  "recall: $EFFECTS notify"
  "almanac-service: $EFFECTS $STORAGE notify"
  "almanac-watch: $EFFECTS rusqlite"
  "almanac-dbus: reqwest hyper oo7 ort fastembed rusqlite notify"
  "almanac-client: $EFFECTS rusqlite openssl-sys notify"
  "almanac-fake: $EFFECTS notify"
  "almanac-local: $EFFECTS notify"
)
fail=0

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

# The allowed edges between our own crates (and porter's): each crate's DIRECT normal and build
# path dependencies (all features), and nothing else. A dependency not listed is a leak; so is
# one the crate no longer has, so the table stays exact. Dev dependencies are outside it. The
# check passes --all-features, so `almanac-client`'s optional `almanac-service` (feature
# `in_process`) and `almanac-dbus` (feature `dbus`) both belong to its row.
# almanac depends on porter (porter-core, prov, and memoryd's porter-infer, porter-client and porter-dbus),
# never on stoker, docket or cua: other areas' payloads are opaque `EventBody::Area`.
EDGES=(
  "almanac-core: porter-core prov"
  "almanac-seal: almanac-core"
  "almanac-store: almanac-core almanac-seal"
  "eventlog: almanac-core almanac-seal almanac-store"
  "memfiles: almanac-core almanac-seal almanac-store"
  "recall: almanac-store"
  "recall-fastembed: recall"
  "almanac-service: almanac-core almanac-seal almanac-store memfiles"
  "almanac-watch: almanac-core"
  "almanac-dbus: almanac-core"
  "almanac-client: almanac-core almanac-dbus almanac-service"
  "almanac-fake: almanac-core almanac-seal eventlog memfiles recall almanac-service"
  "almanac-local: almanac-core almanac-seal eventlog memfiles recall almanac-service"
  "memoryd: almanac-core almanac-seal eventlog memfiles recall almanac-service almanac-watch almanac-dbus porter-core porter-daemon porter-dbus porter-infer porter-client"
)
for edge in "${EDGES[@]}"; do
  crate="${edge%%:*}"
  read -r -a allowed <<<"${edge#*:}"
  found=$(cargo tree -p "$crate" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
    | grep -E '\((/|https://github.com/PoHsuanLai/)' | awk '{print $1}' | grep -vx "$crate" | sort -u | tr '\n' ' ')
  want=$(printf '%s\n' "${allowed[@]}" | grep . | sort -u | tr '\n' ' ')
  if [ "$found" != "$want" ]; then
    echo "EDGE: $crate depends on [${found% }], the table allows [${want% }]"
    fail=1
  else
    echo "edges hold: $crate depends on [${found% }]"
  fi
done

# Nothing of almanac's reaches another area's repo.
for banned in docket-core docket-router intentd cua-run cua-bus cuad companion-wire model-provider; do
  if cargo tree --workspace -i "$banned" -e normal,build,dev 2>/dev/null | grep -q .; then
    echo "LEAK: the workspace depends on $banned (almanac stores other areas' payloads opaquely)"
    fail=1
  fi
done

# The portable core (ARCHITECTURE.md section 1a) is checked by its own script: no-default-features
# builds, no platform dependency in the tree, cross-target checks. It needs no network.
./scripts/check-portable.sh || fail=1

# The test-only features are never on in a default build (a dist build uses default features):
# memoryd's `test-keys` and `test-proc-root`, and almanac-seal's `test-keys` that one turns on.
enabled=$(cargo tree -p memoryd -e normal,build -f '{p} [{f}]' --prefix none 2>/dev/null \
  | grep -E '^(memoryd|almanac-seal) ' | grep -oE '\[[^]]*\]' | tr ',[]' '\n\n\n')
if printf '%s\n' "$enabled" | grep -qE '^test-'; then
  echo "TEST FEATURE: a default build of memoryd enables: $(printf '%s\n' "$enabled" | grep -E '^test-' | tr '\n' ' ')"
  fail=1
else
  echo "test features (test-keys, test-proc-root) are off in a default build of memoryd"
fi

# Every workspace member has a row above, so a new crate cannot slip in unchecked.
for member in $(sed -n 's#^  "crates/\(.*\)",$#\1#p' Cargo.toml); do
  printf '%s\n' "${EDGES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EDGES"; fail=1; }
done

exit "$fail"
