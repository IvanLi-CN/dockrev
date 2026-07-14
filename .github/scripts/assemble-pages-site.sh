#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: assemble-pages-site.sh <docs_dir> <storybook_dir> <demo_dir> <output_dir>

Copies the built docs site into the output root and nests Storybook under output_dir/storybook
and the public demo under output_dir/demo.
EOF
}

if [[ "$#" -ne 4 ]]; then
  usage >&2
  exit 1
fi

docs_dir="$1"
storybook_dir="$2"
demo_dir="$3"
output_dir="$4"

if [[ ! -d "$docs_dir" ]]; then
  echo "docs_dir does not exist: $docs_dir" >&2
  exit 1
fi

if [[ ! -d "$storybook_dir" ]]; then
  echo "storybook_dir does not exist: $storybook_dir" >&2
  exit 1
fi

if [[ ! -d "$demo_dir" ]]; then
  echo "demo_dir does not exist: $demo_dir" >&2
  exit 1
fi

rm -rf "$output_dir"
mkdir -p "$output_dir/storybook" "$output_dir/demo"

cp -R "$docs_dir"/. "$output_dir"/
cp -R "$storybook_dir"/. "$output_dir/storybook"/
cp -R "$demo_dir"/. "$output_dir/demo"/

python3 - "$output_dir/404.html" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
html = path.read_text()
marker = "</head>"
if marker not in html:
    raise SystemExit("assembled site 404.html is missing </head>")

inline_script = """<script>
(function () {
  var restoreStorageKey = 'dockrev:pages-demo:restore';
  var currentPath = window.location.pathname;
  var match = currentPath.match(/^(.*?\\/demo)(\\/.*)$/);
  if (!match) return;

  var demoBasePath = match[1] + '/';
  if (currentPath === demoBasePath || currentPath === match[1] + '/index.html') return;

  var pendingPath = window.location.pathname + window.location.search + window.location.hash;
  try {
    window.sessionStorage.setItem(
      restoreStorageKey,
      JSON.stringify({ path: pendingPath, savedAt: Date.now() })
    );
  } catch (_) {
    return;
  }
  window.location.replace(demoBasePath);
})();
</script>"""

if restoreStorageKey := "dockrev:pages-demo:restore":
    if restoreStorageKey in html:
        sys.exit(0)

path.write_text(html.replace(marker, inline_script + "\n" + marker, 1))
PY

if [[ ! -f "$output_dir/index.html" ]]; then
  echo "assembled site is missing root index.html" >&2
  exit 1
fi

if [[ ! -f "$output_dir/storybook/index.html" ]]; then
  echo "assembled site is missing storybook/index.html" >&2
  exit 1
fi

if [[ ! -f "$output_dir/demo/index.html" ]]; then
  echo "assembled site is missing demo/index.html" >&2
  exit 1
fi
