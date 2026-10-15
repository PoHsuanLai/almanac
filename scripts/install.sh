#!/usr/bin/env bash
# install.sh: install almanac's memory daemon (memoryd), for the whole machine or for one user.
#
#   scripts/install.sh                   build (release, --locked) and install under /usr/local;
#                                        the steps that need root use $SUDO, else sudo, else doas,
#                                        else pkexec, else they are printed for you to run
#   scripts/install.sh --user            install for you alone under ~/.local, no root
#   --prefix DIR                         install under DIR (default /usr/local; with --user ~/.local)
#   --destdir DIR                        put everything under DIR, for a package; nothing asks for root
#   --no-build                           install what is already in target/release
#   --bin-dir DIR                        install programs already built in DIR instead of building
#   --dry-run                            print every action; build and write nothing
#   --sandbox auto|on|off                install the unit's private-view protections (the sandbox.conf
#                                        drop-in). auto (default) tests whether this system allows them
#                                        and leaves them out when it does not
#   --distro debian|fedora|arch          the family of Linux, for package names (default: read from
#                                        /etc/os-release; ubuntu counts as debian)
#
# What goes where (system install; --user puts the same under ~/.local and ~/.config):
#   PREFIX/libexec/quire/memoryd         the daemon
#   PREFIX/lib/systemd/user/             memoryd.service, ExecStart pointed at the prefix, and
#                                        memoryd.service.d/sandbox.conf, its protections
#   PREFIX/share/dbus-1/services/        the activation file; interfaces/ has the interface XML
#   PREFIX/share/quire/settings/         the Settings schema
#   /etc/quire/memory-callers.toml       which units may call memoryd (needs root; memoryd reads it at
#                                        start, as MEMORYD_SYSTEM_CALLERS in a test build)
set -euo pipefail

REPO_NAME="almanac"
SANDBOX_NOTE="memoryd still limits itself to its own folders with the kernel's own file restrictions (Landlock), unit or not."
# name|cargo package|libexec or bin
BINARIES=("memoryd|memoryd|libexec")

extra_arg() { return 1; }

# SQLCipher is built from source, with its own OpenSSL: that needs perl and make.
extra_build_tools() {
  need perl perl-core perl perl
  need make make build-essential base-devel
}

install_rest() {
  put_unit "$ROOT/dbus/memoryd.service"
  put 644 "$ROOT/dbus/org.quire.Memory1.service" "$SHARE/dbus-1/services/org.quire.Memory1.service" unit
  put 644 "$ROOT/dbus/org.quire.Memory1.xml" "$SHARE/dbus-1/interfaces/org.quire.Memory1.xml" copy
  put_tree 644 "$ROOT/dist/settings" "$SHARE/quire/settings"
  install_callers "$ROOT/dbus/memory-callers.toml" memory-callers.toml
}

# memoryd keeps its keys in the person's password store; without one a Space cannot open.
runtime_checks() {
  have busctl || return 0
  busctl --user list --activatable 2>/dev/null | grep 'org.freedesktop.secrets' >/dev/null && return 0
  busctl --user list 2>/dev/null | grep 'org.freedesktop.secrets' >/dev/null && return 0
  systemctl --user show-environment >/dev/null 2>&1 || return 0
  say ""
  say "No password store (GNOME Keyring or KWallet) was found on your session. memoryd keeps its keys there."
  case "$FAMILY" in
    fedora|debian|arch) say "Install one:  $(install_cmd) gnome-keyring" ;;
  esac
}

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

PREFIX=""
DESTDIR=""
MODE=system
BUILD=1
DRY=0
SANDBOX=auto
FAMILY=""
BIN_FROM=""

usage() { sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'; }
say() { printf '%s\n' "$*"; }
act() { say "  $*"; }
die() { printf '%s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
want_arg() { [ "$2" -ge 2 ] || die "$1 needs a value"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) want_arg "$1" $#; PREFIX="$2"; shift ;;
    --prefix=*) PREFIX="${1#--prefix=}" ;;
    --destdir) want_arg "$1" $#; DESTDIR="$2"; shift ;;
    --destdir=*) DESTDIR="${1#--destdir=}" ;;
    --user) MODE=user ;;
    --no-build) BUILD=0 ;;
    --dry-run) DRY=1 ;;
    --bin-dir) want_arg "$1" $#; BIN_FROM="$2"; shift ;;
    --bin-dir=*) BIN_FROM="${1#--bin-dir=}" ;;
    --sandbox) want_arg "$1" $#; SANDBOX="$2"; shift ;;
    --sandbox=*) SANDBOX="${1#--sandbox=}" ;;
    --distro) want_arg "$1" $#; FAMILY="$2"; shift ;;
    --distro=*) FAMILY="${1#--distro=}" ;;
    -h|--help) usage; exit 0 ;;
    *) if ! extra_arg "$1"; then say "unknown argument: $1" >&2; usage >&2; exit 2; fi ;;
  esac
  shift
done

case "$SANDBOX" in auto|on|off) ;; *) die "--sandbox is auto, on or off" ;; esac

# ---- which family of Linux this is: debian (Ubuntu too), fedora or arch ----
detect_family() {
  local id like word
  id="$( . /etc/os-release 2>/dev/null; printf '%s' "${ID:-}" )"
  like="$( . /etc/os-release 2>/dev/null; printf '%s' "${ID_LIKE:-}" )"
  for word in $id $like; do
    case "$word" in
      ubuntu|debian) echo debian; return ;;
      fedora|rhel|centos) echo fedora; return ;;
      arch) echo arch; return ;;
    esac
  done
  echo unknown
}
case "${FAMILY:-}" in
  "") FAMILY="$(detect_family)" ;;
  ubuntu|debian) FAMILY=debian ;;
  fedora|rhel) FAMILY=fedora ;;
  arch) FAMILY=arch ;;
  *) die "--distro is one of: debian (or ubuntu), fedora (or rhel), arch" ;;
esac

install_cmd() {
  case "$FAMILY" in
    fedora) echo "sudo dnf install" ;;
    debian) echo "sudo apt install" ;;
    arch) echo "sudo pacman -S" ;;
  esac
}

# ---- where things go ----
if [ "$MODE" = user ]; then
  [ "$(id -u)" -ne 0 ] || die "--user is for yourself; run it as your own user, not root"
  [ -n "$PREFIX" ] || PREFIX="$HOME/.local"
else
  [ -n "$PREFIX" ] || PREFIX=/usr/local
fi
case "$PREFIX" in /*) ;; *) die "the prefix must be an absolute path: $PREFIX" ;; esac
case "$PREFIX" in *[\|\&\\\ ]*) die "the prefix cannot contain spaces, |, & or \\: $PREFIX" ;; esac
case "$DESTDIR" in ""|/*) ;; *) die "--destdir must be an absolute path: $DESTDIR" ;; esac

LIBEXEC="$PREFIX/libexec/quire"
BINDIR="$PREFIX/bin"
if [ "$MODE" = user ]; then
  UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
  CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/quire"
else
  UNIT_DIR="$PREFIX/lib/systemd/user"
  SHARE="$PREFIX/share"
  CONF_DIR=/etc/xdg/quire
fi
SYS_QUIRE=/etc/quire

under_home() { case "$PREFIX" in "$HOME"|"$HOME"/*) return 0 ;; *) return 1 ;; esac; }

# ---- the program that gets administrator rights: $SUDO, sudo, doas, pkexec ----
ESC=()
find_escalator() {
  [ "$MODE" = system ] || return 0
  if [ -n "${SUDO:-}" ]; then read -r -a ESC <<<"$SUDO"
  elif have sudo; then ESC=(sudo)
  elif have doas; then ESC=(doas)
  elif have pkexec; then ESC=(pkexec)
  fi
}

STAGE=""
MANUAL=()
cleanup() { if [ -n "$STAGE" ] && [ "${#MANUAL[@]}" -eq 0 ]; then rm -rf "$STAGE"; fi; }
trap cleanup EXIT

can_write() { # the nearest directory that exists must be ours to write
  local d="$1"
  while [ ! -e "$d" ] && [ "$d" != "/" ]; do d="$(dirname "$d")"; done
  [ -w "$d" ]
}

LAST=ran
# priv TARGET COMMAND...: run COMMAND (which writes TARGET) as ourselves, as root through the
# helper, or queue it for the person to run.
priv() {
  local target="$1"; shift
  LAST=ran
  if can_write "$target"; then "$@"
  elif [ "${#ESC[@]}" -gt 0 ]; then "${ESC[@]}" "$@"
  else MANUAL+=("$(printf '%q ' "$@")"); LAST=queued
  fi
}

# ---- text rewrites ----
rewrite_exec() { # the shipped /usr/libexec/quire is now under the prefix
  sed -E -e "s|^(ExecStart=)/usr/libexec/quire/|\1$LIBEXEC/|" -e "s|^(Exec=)/usr/libexec/quire/|\1$LIBEXEC/|"
}
render() { # kind source -> text on stdout
  case "$1" in
    unit) rewrite_exec <"$2" ;;
    # A program under the home folder could not start if its unit hid the home folder.
    sandbox) if under_home; then sed -E 's|^ProtectHome=yes$|ProtectHome=read-only|' "$2"; else cat "$2"; fi ;;
    *) cat "$2" ;;
  esac
}

# put MODE SOURCE DEST KIND [keep]: install one file; with keep, never replace an existing one.
put() {
  local mode="$1" src="$2" dest="$DESTDIR$3" kind="$4" keep="${5:-replace}" tmp
  if [ "$keep" = keep ] && [ -e "$dest" ]; then act "kept (yours): $dest"; return 0; fi
  if [ "$DRY" = 1 ]; then act "install -m $mode $src -> $dest"; return 0; fi
  tmp="$(mktemp "$STAGE/file.XXXXXX")"
  render "$kind" "$src" >"$tmp"
  priv "$dest" install -D -m "$mode" "$tmp" "$dest"
  if [ "$LAST" = ran ]; then act "installed: $dest"; else act "needs you to run it as root: $dest"; fi
}
put_tree() { # MODE SOURCE_DIR DEST_DIR: every file under a directory
  local mode="$1" src="$2" dest="$3" file
  [ -d "$src" ] || return 0
  while IFS= read -r file; do put "$mode" "$src/$file" "$dest/$file" copy; done \
    < <(cd "$src" && find . -type f | sed 's|^\./||' | sort)
}

# ---- build tools, and the names of their packages on each family ----
MISSING_CMDS=()
MISSING_PKGS=()
# need COMMAND FEDORA-PACKAGE DEBIAN-PACKAGE ARCH-PACKAGE
need() {
  have "$1" && return 0
  local pkg=""
  case "$FAMILY" in fedora) pkg="$2" ;; debian) pkg="$3" ;; arch) pkg="$4" ;; esac
  MISSING_CMDS+=("$1")
  if [ -n "$pkg" ]; then MISSING_PKGS+=("$pkg"); fi
}
report_missing() {
  [ "${#MISSING_CMDS[@]}" -gt 0 ] || return 0
  say "Building needs programs this computer does not have yet: ${MISSING_CMDS[*]}"
  if [ "$FAMILY" = unknown ]; then
    say "Install them with your system's package manager, then run this again."
  elif [ "${#MISSING_PKGS[@]}" -gt 0 ]; then
    say "On this system:  $(install_cmd) ${MISSING_PKGS[*]}"
  fi
  case " ${MISSING_CMDS[*]} " in
    *" cargo "*)
      if [ "$FAMILY" = debian ]; then
        say "Rust: install rustup from https://rustup.rs. The Rust that Ubuntu packages is too old for this project."
      else
        say "Rust: install rustup (from https://rustup.rs if your system does not package it). It fetches the exact Rust version this project asks for, so it needs the network."
      fi ;;
  esac
  say "Or install without building: put programs you built elsewhere in a folder and pass --bin-dir FOLDER."
  exit 1
}
check_build_tools() {
  need cargo rustup "" rustup
  need cc gcc build-essential base-devel
  need pkg-config pkgconf-pkg-config pkg-config pkgconf
  extra_build_tools
  report_missing
}

# ---- the linker the repo's .cargo/config.toml asks for (clang with mold) ----
# A clean machine has neither. Without both, link with the system's cc and keep the rest of the
# repo's rustflags. The file is read, never edited.
relax_linker() {
  have clang && have mold && return 0
  local cfg="$ROOT/.cargo/config.toml" line raw=() words=() kept=() w next i=0
  [ -f "$cfg" ] || return 0
  grep -qE 'mold|^linker *= *"clang"' "$cfg" || return 0
  line="$(sed -n '/^\[target\.x86_64-unknown-linux-gnu\]/,/^\[/{s/^rustflags *= *\[\(.*\)\].*/\1/p}' "$cfg")"
  IFS=, read -r -a raw <<<"$line"
  for w in "${raw[@]}"; do w="${w//\"/}"; w="${w// /}"; [ -z "$w" ] || words+=("$w"); done
  while [ "$i" -lt "${#words[@]}" ]; do
    w="${words[$i]}"
    case "$w" in
      -C)
        next="${words[$((i + 1))]:-}"
        case "$next" in link-arg=-fuse-ld=*|linker=*) i=$((i + 2)); continue ;; esac ;;
      -Clink-arg=-fuse-ld=*|-Clinker=*) i=$((i + 1)); continue ;;
    esac
    kept+=("$w"); i=$((i + 1))
  done
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc
  export RUSTFLAGS="${RUSTFLAGS-${kept[*]}}"
  say "clang and mold are not both installed, so this build uses the system linker (a little slower to link)."
}

# ---- build ----
bin_source() { echo "${BIN_FROM:-${CARGO_TARGET_DIR:-$ROOT/target}/release}/$1"; }
cargo_packages() { # -p NAME for each package of BINARIES, once
  local entry rest pkg seen=" "
  for entry in "${BINARIES[@]}"; do
    rest="${entry#*|}"; pkg="${rest%%|*}"
    case "$seen" in *" $pkg "*) continue ;; esac
    seen="$seen$pkg "
    printf '%s\n' "-p" "$pkg"
  done
}
build_all() {
  local args=()
  if [ -n "$BIN_FROM" ]; then say "Using the programs already built in $BIN_FROM."; return 0; fi
  [ "$BUILD" = 1 ] || return 0
  mapfile -t args < <(cargo_packages)
  [ "${#args[@]}" -gt 0 ] || return 0
  if [ "$DRY" = 1 ]; then act "cargo build --release --locked ${args[*]} (in $ROOT)"; return 0; fi
  check_build_tools
  relax_linker
  say "Building (this takes a while the first time)..."
  (cd "$ROOT" && cargo build --release --locked "${args[@]}")
}
preflight() {
  local entry name missing=0
  [ "$DRY" = 0 ] || return 0
  for entry in "${BINARIES[@]}"; do
    name="${entry%%|*}"
    if [ ! -f "$(bin_source "$name")" ]; then
      echo "not built: $(bin_source "$name")" >&2; missing=1
    fi
  done
  [ "$missing" = 0 ] || die "Nothing was installed. Build first (leave out --no-build), or point --bin-dir at the built programs."
}

# ---- do the user units' private-view protections work here? ----
USE_SANDBOX=on
SANDBOX_WHY=""
probe_sandbox() {
  have systemd-run || return 0                                   # no systemd to ask: assume yes
  systemctl --user show-environment >/dev/null 2>&1 || return 0   # no user manager to ask: assume yes
  if have timeout; then
    timeout 30 systemd-run --user --quiet --collect -p PrivateTmp=yes -p ProtectSystem=strict --wait true >/dev/null 2>&1
  else
    systemd-run --user --quiet --collect -p PrivateTmp=yes -p ProtectSystem=strict --wait true >/dev/null 2>&1
  fi
}
decide_sandbox() {
  if [ "${NO_UNITS:-0}" = 1 ]; then USE_SANDBOX=off; return 0; fi
  case "$SANDBOX" in
    on|off) USE_SANDBOX="$SANDBOX"; return 0 ;;
  esac
  if [ -n "$DESTDIR" ]; then USE_SANDBOX=on   # building a package: it cannot be tested here
  elif probe_sandbox; then USE_SANDBOX=on
  else USE_SANDBOX=off; SANDBOX_WHY=probe
  fi
}
sandbox_remedy() {
  cat <<EOT

The protections for the background programs were left out.
This computer does not let your background programs run in a private view of the system (their
own temporary files, a read-only system, no sight of your home folder), so units with those
settings would not start. The programs install and run without that extra layer.
$SANDBOX_NOTE
Ubuntu 24.04 and later block it for programs that have no AppArmor profile. To turn the
protections on, either allow it for the whole machine:
  sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
(to keep that after a restart, write "kernel.apparmor_restrict_unprivileged_userns = 0" to a
file in /etc/sysctl.d), or add an AppArmor profile that lets /usr/lib/systemd/systemd create
user namespaces. Then run this installer again. Leave it as it is if you prefer.
EOT
}

# ---- one unit: the unit, and its protections as a drop-in ----
put_unit() { # UNIT-FILE (a path in the repo)
  local unit="$1" name
  name="$(basename "$unit")"
  put 644 "$unit" "$UNIT_DIR/$name" unit
  if [ "$USE_SANDBOX" = on ] && [ -f "$unit.d/sandbox.conf" ]; then
    put 644 "$unit.d/sandbox.conf" "$UNIT_DIR/$name.d/sandbox.conf" sandbox
  fi
}

install_bins() {
  local entry name rest where
  for entry in "${BINARIES[@]}"; do
    name="${entry%%|*}"; rest="${entry#*|}"; where="${rest#*|}"
    case "$where" in
      libexec) put 755 "$(bin_source "$name")" "$LIBEXEC/$name" copy ;;
      bin) put 755 "$(bin_source "$name")" "$BINDIR/$name" copy ;;
    esac
  done
}
install_callers() { # SOURCE NAME: the system's table, read by the daemon at start
  put 644 "$1" "$SYS_QUIRE/$2" copy
}

finish() {
  if [ "${#MANUAL[@]}" -gt 0 ]; then
    say ""
    if [ "$MODE" = user ]; then say "These steps change the whole machine, so --user leaves them to you:"
    else say "No sudo, doas or pkexec was found. Run these steps as an administrator:"; fi
    printf '  %s\n' "${MANUAL[@]}"
    say "(the files they copy are kept in $STAGE)"
  fi
  if [ "$DRY" = 1 ]; then say ""; say "This was a dry run: nothing was built or written."; return 0; fi
  if [ "$USE_SANDBOX" = off ] && [ "$SANDBOX_WHY" = probe ]; then sandbox_remedy; fi
  if [ -z "$DESTDIR" ]; then
    say ""
    say "Done. To start the new units:  systemctl --user daemon-reload"
  fi
}

# A Debian or Ubuntu system without this package has user units and a session bus that do not see each other.
common_checks() {
  [ "${NO_UNITS:-0}" = 0 ] || return 0
  [ "$FAMILY" = debian ] && have dpkg || return 0
  dpkg -s dbus-user-session >/dev/null 2>&1 && return 0
  say ""
  say "This system lacks the package dbus-user-session, which background services need:  sudo apt install dbus-user-session"
}

main() {
  find_escalator
  say "Installing $REPO_NAME: $MODE install under $PREFIX, units in $UNIT_DIR"
  build_all
  preflight
  decide_sandbox
  if [ "$DRY" = 0 ]; then STAGE="$(mktemp -d)"; fi
  install_bins
  install_rest
  runtime_checks
  common_checks
  finish
}

main
