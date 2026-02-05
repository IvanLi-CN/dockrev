#!/usr/bin/env bash
set -euo pipefail

api_root="${GITHUB_API_URL:-https://api.github.com}"
repo="${GITHUB_REPOSITORY:-}"
token="${GITHUB_TOKEN:-}"
pr_number="${PR_NUMBER:-}"

if [[ -z "${repo}" ]]; then
  echo "label-gate: missing GITHUB_REPOSITORY" >&2
  exit 2
fi

if [[ -z "${token}" ]]; then
  echo "label-gate: missing GITHUB_TOKEN" >&2
  exit 2
fi

if [[ -z "${pr_number}" ]]; then
  echo "label-gate: missing PR_NUMBER" >&2
  exit 2
fi

labels_json="$(
  curl -fsSL \
    --max-time 15 \
    -H "Accept: application/vnd.github+json" \
    -H "Authorization: Bearer ${token}" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${api_root}/repos/${repo}/issues/${pr_number}/labels?per_page=100"
)"

export labels_json
python - <<'PY'
from __future__ import annotations

import json
import os
import sys

allowed = {
    "type:docs",
    "type:skip",
    "type:patch",
    "type:minor",
    "type:major",
}

allowed_channels = {
    "channel:prerelease",
}

labels = json.loads(os.environ["labels_json"])
names = [l.get("name", "") for l in labels if isinstance(l, dict)]

type_like = sorted({n for n in names if n.startswith("type:")})
unknown_type = sorted({n for n in type_like if n not in allowed})
allowed_present = sorted({n for n in names if n in allowed})

channel_like = sorted({n for n in names if n.startswith("channel:")})
unknown_channel = sorted({n for n in channel_like if n not in allowed_channels})
allowed_channels_present = sorted({n for n in names if n in allowed_channels})

def fail(msg: str) -> None:
    print(f"::error::{msg}")
    print(f"Allowed intent labels: {', '.join(sorted(allowed))}")
    print(f"Allowed channel labels: {', '.join(sorted(allowed_channels))}")
    print(f"Found labels: {', '.join(sorted(set(names)))}")
    sys.exit(1)

if unknown_type:
    fail(f"Unknown intent label(s): {', '.join(unknown_type)}")

if unknown_channel:
    fail(f"Unknown channel label(s): {', '.join(unknown_channel)}")

if len(allowed_channels_present) > 1:
    fail(f"Conflicting channel labels: {', '.join(allowed_channels_present)} (must be at most one)")

if len(allowed_present) == 0:
    fail("Missing intent label: PR must have exactly one intent label")

if len(allowed_present) > 1:
    fail(f"Conflicting intent labels: {', '.join(allowed_present)} (must be exactly one)")

intent = allowed_present[0]
release_channel = "prerelease" if "channel:prerelease" in allowed_channels_present else "stable"

if intent in ("type:docs", "type:skip"):
    bump_level = ""
else:
    bump_level = intent.removeprefix("type:")

out_path = os.environ.get("GITHUB_OUTPUT")
if out_path:
    with open(out_path, "a", encoding="utf-8") as f:
        f.write(f"release_intent_label={intent}\n")
        f.write(f"bump_level={bump_level}\n")
        f.write(f"release_channel={release_channel}\n")

print(f"Intent label OK: {intent}")
print(f"release_channel={release_channel}")
if bump_level:
    print(f"bump_level={bump_level}")
else:
    print("bump_level=<none> (release will be skipped)")
PY
