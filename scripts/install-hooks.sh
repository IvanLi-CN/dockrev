#!/usr/bin/env bash
set -euo pipefail

die() {
  printf '[dockrev hooks] error: %s\n' "$*" >&2
  exit 1
}

if ! repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  die "not inside a Git worktree"
fi

cd "$repo_root"

git_common_dir="$(git rev-parse --git-common-dir)"
case "$git_common_dir" in
  /*) ;;
  *) git_common_dir="$repo_root/$git_common_dir" ;;
esac
hooks_path="${DOCKREV_HOOKS_PATH:-$git_common_dir/hooks}"

mkdir -p "$hooks_path"
hook_file="$hooks_path/post-checkout"
previous_hook="$hooks_path/post-checkout.dockrev-previous"

if [[ -f "$hook_file" ]] && ! grep -q 'DOCKREV MANAGED POST-CHECKOUT HOOK' "$hook_file"; then
  cp "$hook_file" "$previous_hook"
  chmod +x "$previous_hook"
  printf '[dockrev hooks] preserved existing post-checkout hook at %s\n' "$previous_hook"
fi

cat > "$hook_file" <<'HOOK'
#!/usr/bin/env bash
set -euo pipefail
# DOCKREV MANAGED POST-CHECKOUT HOOK

hook_dir="$(cd "$(dirname "$0")" && pwd)"
previous_hook="$hook_dir/post-checkout.dockrev-previous"
if [[ -x "$previous_hook" ]]; then
  "$previous_hook" "$@"
fi

if [[ "${DOCKREV_BOOTSTRAP_SKIP:-}" == "1" ]]; then
  exit 0
fi

if ! repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  exit 0
fi

bootstrap="$repo_root/scripts/bootstrap-worktree.sh"
if [[ ! -x "$bootstrap" ]]; then
  if [[ -f "$bootstrap" ]]; then
    chmod +x "$bootstrap" 2>/dev/null || true
  fi
fi

if [[ -x "$bootstrap" ]]; then
  "$bootstrap"
else
  printf '[dockrev hooks] bootstrap script not found in this checkout; skipping\n' >&2
fi
HOOK

chmod +x "$hook_file"
git config core.hooksPath "$hooks_path"

printf '[dockrev hooks] installed shared hooks at %s\n' "$hooks_path"
