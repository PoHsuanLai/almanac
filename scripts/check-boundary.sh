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
# almanac-watch. almanac-seal reaches oo7 only through its `oo7` feature, almanac-client reaches
# zbus only through its `dbus` feature and rusqlite and openssl-sys (SQLCipher, vendored OpenSSL)
# only through its `in_process` feature, and almanac-dbus reaches tokio only through zbus's
# `tokio` feature. recall-fastembed and memoryd are the places that reach fastembed and the
# daemon's runtime, so they have no rule.
EFFECTS="zbus zvariant tokio reqwest hyper oo7 ort fastembed"
# almanac-core is also light enough for cua-bus to depend on (interface ask 26: `HandedBack`
# carries `almanac_core::EventRef`; cua's own EFFECTS list adds these to ours), so it reaches none
# of them either.
CUA_EFFECTS="hyper-util rustls pipewire wayland-client wayland-backend wayland-server reis atspi rmcp cedar-policy"
RULES=(
  "almanac-core: $EFFECTS rusqlite notify toml $CUA_EFFECTS"
  "almanac-seal: $EFFECTS rusqlite notify"
  "eventlog: $EFFECTS notify"
  "memfiles: $EFFECTS rusqlite notify"
  "recall: $EFFECTS notify"
  "almanac-service: $EFFECTS notify"
  "almanac-watch: $EFFECTS rusqlite"
  "almanac-dbus: reqwest hyper oo7 ort fastembed rusqlite notify"
  "almanac-client: $EFFECTS rusqlite openssl-sys notify"
  "almanac-fake: $EFFECTS notify"
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
  leaked=0
  for dep in "${forbidden[@]}"; do
    if cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | grep -q .; then
      echo "LEAK: $crate depends on $dep"
      cargo tree -p "$crate" -i "$dep" -e normal,build 2>/dev/null | head -20
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
# almanac depends on porter (porter-core, prov, and memoryd's porter-infer and porter-client),
# never on stoker, docket or cua: other areas' payloads are opaque `EventBody::Area`.
EDGES=(
  "almanac-core: porter-core prov"
  "almanac-seal: almanac-core"
  "eventlog: almanac-core almanac-seal"
  "memfiles: almanac-core almanac-seal"
  "recall:"
  "recall-fastembed: recall"
  "almanac-service: almanac-core almanac-seal eventlog memfiles recall"
  "almanac-watch: almanac-core"
  "almanac-dbus: almanac-core"
  "almanac-client: almanac-core almanac-dbus almanac-service"
  "almanac-fake: almanac-core almanac-seal eventlog memfiles recall almanac-service"
  "memoryd: almanac-core almanac-seal eventlog memfiles recall almanac-service almanac-watch almanac-dbus porter-core porter-infer porter-client"
)
for edge in "${EDGES[@]}"; do
  crate="${edge%%:*}"
  read -r -a allowed <<<"${edge#*:}"
  found=$(cargo tree -p "$crate" --depth 1 -e normal,build --prefix none --all-features 2>/dev/null \
    | grep '(/' | awk '{print $1}' | grep -vx "$crate" | sort -u | tr '\n' ' ')
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

# Every workspace member has a row above, so a new crate cannot slip in unchecked.
for member in $(sed -n 's#^  "crates/\(.*\)",$#\1#p' Cargo.toml); do
  printf '%s\n' "${EDGES[@]}" | grep -q "^$member:" || { echo "ERROR: $member has no row in EDGES"; fail=1; }
done

exit "$fail"
