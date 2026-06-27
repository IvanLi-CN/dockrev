#!/usr/bin/env bash
set -euo pipefail

log() {
  printf '[dockrev bootstrap] %s\n' "$*"
}

die() {
  printf '[dockrev bootstrap] error: %s\n' "$*" >&2
  exit 1
}

if [[ "${DOCKREV_BOOTSTRAP_SKIP:-}" == "1" ]]; then
  log "skipped because DOCKREV_BOOTSTRAP_SKIP=1"
  exit 0
fi

if ! repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  die "not inside a Git worktree"
fi

cd "$repo_root"

if ! command -v bun >/dev/null 2>&1; then
  die "Bun is required. Install Bun, then rerun: bun run bootstrap:worktree"
fi

if ! command -v cargo >/dev/null 2>&1; then
  die "Cargo is required. Install Rust/Cargo, then rerun: bun run bootstrap:worktree"
fi

stamp_dir="${DOCKREV_BOOTSTRAP_STAMP_DIR:-}"
if [[ -z "$stamp_dir" ]]; then
  git_dir="$(git rev-parse --git-dir)"
  stamp_dir="$git_dir/dockrev-bootstrap"
fi
mkdir -p "$stamp_dir"

hash_input() {
  local path
  for path in \
    package.json bun.lock \
    web/package.json web/bun.lock \
    docs-site/package.json docs-site/bun.lock \
    Cargo.toml Cargo.lock
  do
    if [[ -f "$path" ]]; then
      printf '%s\n' "--- $path ---"
      cat "$path"
      printf '\n'
    fi
  done
}

if command -v shasum >/dev/null 2>&1; then
  current_hash="$(hash_input | shasum -a 256 | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  current_hash="$(hash_input | sha256sum | awk '{print $1}')"
else
  die "shasum or sha256sum is required to compute the bootstrap stamp"
fi

stamp_file="$stamp_dir/dependencies.sha256"
if [[ -f "$stamp_file" ]] && [[ "$(cat "$stamp_file")" == "$current_hash" ]]; then
  log "dependencies already match lockfiles; nothing to do"
  exit 0
fi

run_bun_install() {
  local dir="$1"
  if [[ -f "$dir/package.json" && -f "$dir/bun.lock" ]]; then
    log "installing Bun dependencies in $dir"
    bun install --cwd "$dir" --frozen-lockfile
  elif [[ -f "$dir/package.json" ]]; then
    die "$dir/package.json exists but $dir/bun.lock is missing"
  fi
}

run_bun_install "."
run_bun_install "web"
run_bun_install "docs-site"

if [[ -f Cargo.toml && -f Cargo.lock ]]; then
  log "fetching Rust dependencies"
  cargo fetch --locked
elif [[ -f Cargo.toml ]]; then
  die "Cargo.toml exists but Cargo.lock is missing"
fi

tmp_stamp="$stamp_file.tmp.$$"
printf '%s\n' "$current_hash" > "$tmp_stamp"
mv "$tmp_stamp" "$stamp_file"
log "bootstrap complete"
