#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: assemble-pages-site.sh <docs_dir> <storybook_dir> <output_dir>

Copies the built docs site into the output root and nests Storybook under output_dir/storybook.
EOF
}

if [[ "$#" -ne 3 ]]; then
  usage >&2
  exit 1
fi

docs_dir="$1"
storybook_dir="$2"
output_dir="$3"

if [[ ! -d "$docs_dir" ]]; then
  echo "docs_dir does not exist: $docs_dir" >&2
  exit 1
fi

if [[ ! -d "$storybook_dir" ]]; then
  echo "storybook_dir does not exist: $storybook_dir" >&2
  exit 1
fi

rm -rf "$output_dir"
mkdir -p "$output_dir/storybook"

cp -R "$docs_dir"/. "$output_dir"/
cp -R "$storybook_dir"/. "$output_dir/storybook"/

if [[ ! -f "$output_dir/index.html" ]]; then
  echo "assembled site is missing root index.html" >&2
  exit 1
fi

if [[ ! -f "$output_dir/storybook/index.html" ]]; then
  echo "assembled site is missing storybook/index.html" >&2
  exit 1
fi
