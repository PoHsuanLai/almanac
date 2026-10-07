#!/usr/bin/env bash
# The portable core (design/36-PORTABLE-CORE.md): these crates build and keep a clean dependency
# tree with `--no-default-features`, so an app on macOS, Windows or another desktop can host
# almanac in process. Everything else (almanac-dbus, memoryd, almanac-fake, recall-fastembed) is a
# desktop extra or a test helper and is not checked here.
#
# No network: every cargo call is --offline against the vendored/cached registry.
set -uo pipefail
cd "$(dirname "$0")/.."

# crate[:features]. almanac-client's in-process transport is its portable path, so it is checked
# with `in_process` (its default-off `dbus` feature is the desktop's). almanac-watch keeps the
# seam and the pure `join`; its `linux` feature (InotifyWatch) is off here.
CORE=(
  "almanac-core"
  "almanac-seal"
  "eventlog"
  "memfiles"
  "recall"
  "almanac-service"
  "almanac-watch"
  "almanac-client:in_process"
)
# What the core must never reach, by crate name (platform services and the desktop's bus).
FORBIDDEN='zbus|zvariant|inotify|notify|landlock|oo7|secret-service'
# Cross targets: checked when rustup has them installed (this script never installs one).
TARGETS=(x86_64-apple-darwin x86_64-pc-windows-gnu x86_64-pc-windows-msvc)
# Crates without C dependencies check on every target; the SQLCipher ones need that target's C
# toolchain (and OpenSSL's cross build), which a Linux box lacks, so they are only reported.
PURE_RUST=(almanac-core almanac-seal memfiles almanac-watch)
# blake3 builds C and assembly SIMD for x86_64 targets, which needs the target's C compiler too;
# its `pure` feature (through each crate's `pure-hash`) is a plain-Rust build for the check.
declare -A PURE_HASH=(
  [almanac-seal]="pure-hash"
  [memfiles]="almanac-seal/pure-hash"
  [almanac-watch]="pure-hash"
)

fail=0
flags() { # "crate[:features]" -> cargo flags
  local crate="${1%%:*}" features=""
  [[ "$1" == *:* ]] && features="${1#*:}"
  printf -- '-p %s --no-default-features' "$crate"
  [ -n "$features" ] && printf -- ' --features %s' "$features"
}

for entry in "${CORE[@]}"; do
  read -r -a args <<<"$(flags "$entry")"
  if ! cargo check --offline -q "${args[@]}" >/dev/null 2>&1; then
    echo "FAIL: cargo check ${args[*]}"
    fail=1
    continue
  fi
  leaked=$(cargo tree --offline "${args[@]}" -e normal,build --prefix none 2>/dev/null \
    | awk '{print $1}' | grep -xE "$FORBIDDEN" | sort -u | tr '\n' ' ')
  if [ -n "$leaked" ]; then
    echo "LEAK: ${args[*]} reaches $leaked"
    fail=1
  else
    echo "portable: ${args[*]} checks and reaches none of ${FORBIDDEN//|/ }"
  fi
done

installed=$(rustup target list --installed 2>/dev/null)
for target in "${TARGETS[@]}"; do
  if ! grep -qx "$target" <<<"$installed"; then
    echo "SKIP: rustup target $target is not installed (not installing it)"
    continue
  fi
  for entry in "${CORE[@]}"; do
    crate="${entry%%:*}"
    read -r -a args <<<"$(flags "$entry")"
    [ -n "${PURE_HASH[$crate]:-}" ] && args+=(--features "${PURE_HASH[$crate]}")
    if cargo check --offline -q --target "$target" "${args[@]}" >/dev/null 2>&1; then
      echo "target $target: $crate checks"
    elif printf '%s\n' "${PURE_RUST[@]}" | grep -qx "$crate"; then
      echo "FAIL: $crate does not check for $target"
      fail=1
    else
      echo "NOTE: $crate not checked for $target (needs the target's C toolchain for SQLCipher and blake3)"
    fi
  done
done
exit "$fail"
