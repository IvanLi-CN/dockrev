#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/dockrev-worktree-bootstrap.XXXXXX")"
remote_repo="$tmp_root/remote.git"
seed_repo="$tmp_root/seed"
test_worktree="$tmp_root/worktree"
missing_tool_err="$tmp_root/missing-tool.err"
missing_tool_out="$tmp_root/missing-tool.out"
branch_name="test/bootstrap"
bin_dir="$tmp_root/bin"
calls_log="$tmp_root/calls.log"

cleanup() {
  if git -C "$seed_repo" worktree list --porcelain >/dev/null 2>&1; then
    git -C "$seed_repo" worktree remove "$test_worktree" --force >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_root"
}
trap cleanup EXIT

mkdir -p "$bin_dir"

cat > "$bin_dir/bun" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'bun %s\n' "$*" >> "${DOCKREV_BOOTSTRAP_TEST_CALLS:?}"
if [[ "${1:-}" == "run" && "${2:-}" == "bootstrap:worktree" ]]; then
  exec bash scripts/bootstrap-worktree.sh
fi
if [[ "${1:-}" == "run" && "${2:-}" == "hooks:install" ]]; then
  exec bash scripts/install-hooks.sh
fi
exit 0
STUB

cat > "$bin_dir/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo %s\n' "$*" >> "${DOCKREV_BOOTSTRAP_TEST_CALLS:?}"
exit 0
STUB

chmod +x "$bin_dir/bun" "$bin_dir/cargo"

git init --bare "$remote_repo" >/dev/null
git clone "$remote_repo" "$seed_repo" >/dev/null
(
  cd "$seed_repo"
  git config user.email "dockrev-bootstrap-test@example.com"
  git config user.name "Dockrev Bootstrap Test"
)

for path in \
  package.json bun.lock Cargo.toml Cargo.lock \
  web/package.json web/bun.lock \
  docs-site/package.json docs-site/bun.lock \
  scripts/bootstrap-worktree.sh scripts/install-hooks.sh
do
  mkdir -p "$seed_repo/$(dirname "$path")"
  cp "$repo_root/$path" "$seed_repo/$path"
done

(
  cd "$seed_repo"
  git add .
  git commit -m "test fixture" >/dev/null
  git push origin HEAD:main >/dev/null
  git branch -M main
)

PATH="$bin_dir:$PATH" DOCKREV_BOOTSTRAP_TEST_CALLS="$calls_log" \
  git -C "$seed_repo" worktree add -b "$branch_name" "$test_worktree" main >/dev/null

(
  cd "$test_worktree"
  PATH="$bin_dir:$PATH" DOCKREV_BOOTSTRAP_TEST_CALLS="$calls_log" bun run hooks:install
  PATH="$bin_dir:$PATH" DOCKREV_BOOTSTRAP_TEST_CALLS="$calls_log" git checkout -b second-checkout >/dev/null
)

grep -q 'bun install --cwd . --frozen-lockfile' "$calls_log"
grep -q 'bun install --cwd web --frozen-lockfile' "$calls_log"
grep -q 'bun install --cwd docs-site --frozen-lockfile' "$calls_log"
grep -q 'cargo fetch --locked' "$calls_log"

initial_calls="$(wc -l < "$calls_log" | tr -d ' ')"
(
  cd "$test_worktree"
  PATH="$bin_dir:$PATH" DOCKREV_BOOTSTRAP_TEST_CALLS="$calls_log" bash scripts/bootstrap-worktree.sh
)
after_cached_calls="$(wc -l < "$calls_log" | tr -d ' ')"
if [[ "$after_cached_calls" != "$initial_calls" ]]; then
  echo "Expected cached bootstrap to skip dependency installs" >&2
  exit 1
fi

(
  cd "$test_worktree"
  PATH="$bin_dir:$PATH" DOCKREV_BOOTSTRAP_TEST_CALLS="$calls_log" DOCKREV_BOOTSTRAP_SKIP=1 git checkout "$branch_name" >/dev/null
)
after_skip_calls="$(wc -l < "$calls_log" | tr -d ' ')"
if [[ "$after_skip_calls" != "$initial_calls" ]]; then
  echo "Expected DOCKREV_BOOTSTRAP_SKIP=1 to skip hook bootstrap" >&2
  exit 1
fi

(
  cd "$test_worktree"
  if PATH="/usr/bin:/bin" bash scripts/bootstrap-worktree.sh >"$missing_tool_out" 2>"$missing_tool_err"; then
    echo "Expected bootstrap to fail without Bun/Cargo" >&2
    exit 1
  fi
  grep -Eq 'Bun is required|Cargo is required' "$missing_tool_err"
)

echo "OK: worktree bootstrap smoke passed"
