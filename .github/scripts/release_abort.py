#!/usr/bin/env python3
"""Abandon an unmerged release identity and release its version reservation."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

import release_policy


class AbortError(RuntimeError):
    pass


def api_request(
    api_root: str, token: str, method: str, path: str,
    payload: dict[str, Any] | None = None,
) -> Any:
    url = f"{api_root.rstrip('/')}{path}"
    data = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(
        url,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "dockrev-release-abort",
            **({"Content-Type": "application/json"} if data else {}),
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = response.read().decode(response.headers.get_content_charset() or "utf-8")
            return json.loads(body) if body else {}
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        if error.code == 404:
            return None
        raise AbortError(f"GitHub API {method} {path} failed: {error.code}: {detail[:400]}") from error
    except urllib.error.URLError as error:
        raise AbortError(f"GitHub API request failed: {error.reason}") from error


def repository_parts(repository: str) -> tuple[str, str]:
    owner, separator, name = repository.partition("/")
    if not separator or not owner or not name:
        raise AbortError("repository must be owner/name")
    return owner, name


def ref_sha(api_root: str, token: str, repository: str, ref: str) -> str | None:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(ref, safe="/")
    response = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/git/ref/{quoted}")
    if response is None:
        return None
    sha = str((response.get("object") or {}).get("sha", ""))
    release_policy.validate_sha(sha, f"{ref} SHA")
    return sha


def open_identity_pull_request(
    api_root: str, token: str, repository: str, branch: str,
) -> dict[str, Any] | None:
    owner, name = repository_parts(repository)
    query = urllib.parse.urlencode({"state": "open", "head": f"{owner}:{branch}", "base": "main", "per_page": 100})
    pulls = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/pulls?{query}")
    if not isinstance(pulls, list):
        raise AbortError("GitHub did not return the release identity pull request list")
    if len(pulls) > 1:
        raise AbortError("multiple open pull requests exist for one release identity branch")
    return pulls[0] if pulls else None


def identity_merged(api_root: str, token: str, repository: str, identity_sha: str) -> bool:
    owner, name = repository_parts(repository)
    quoted = urllib.parse.quote(identity_sha, safe="")
    compare = api_request(api_root, token, "GET", f"/repos/{owner}/{name}/compare/{quoted}...main")
    if not isinstance(compare, dict):
        raise AbortError("release identity history could not be compared with main")
    status = compare.get("status")
    if status == "behind":
        return False
    if status in {"ahead", "identical"}:
        return True
    raise AbortError(f"release identity history comparison is inconclusive: {status!r}")


def abort(args: argparse.Namespace) -> dict[str, Any]:
    release_policy.load_policy()
    release_policy.parse_version(args.version)
    reason = args.reason.strip()
    if not reason:
        raise AbortError("abort reason must be a non-empty string")

    owner, name = repository_parts(args.repository)
    reservation_ref = f"heads/release-reservation/v{args.version}"
    preparation_ref = f"heads/release-preparation/v{args.version}"
    publication_lock_ref = f"heads/release-publication-lock/v{args.version}"

    identity_sha = ref_sha(args.api_root, args.token, args.repository, reservation_ref)
    if identity_sha is None:
        raise AbortError(f"no version reservation exists for {args.version}")

    for tag in (f"v{args.version}", args.version):
        quoted = urllib.parse.quote(tag, safe="")
        if api_request(args.api_root, args.token, "GET", f"/repos/{owner}/{name}/git/ref/tags/{quoted}") is not None:
            raise AbortError(f"release tag {tag} already exists; published identities cannot be aborted")

    released = api_request(
        args.api_root, args.token, "GET",
        f"/repos/{owner}/{name}/releases/tags/{urllib.parse.quote(f'v{args.version}', safe='')}",
    )
    if released is not None:
        raise AbortError("a GitHub Release already exists; published identities cannot be aborted")

    if ref_sha(args.api_root, args.token, args.repository, publication_lock_ref) is not None:
        raise AbortError("a publication lock exists; publication already started")

    if identity_merged(args.api_root, args.token, args.repository, identity_sha):
        raise AbortError("release identity is merged into main; use the Release recovery path instead")

    preparation_sha = ref_sha(args.api_root, args.token, args.repository, preparation_ref)
    if preparation_sha is not None and preparation_sha != identity_sha:
        raise AbortError("release preparation branch does not point at the reserved identity")

    closed_pull_request = None
    pull = open_identity_pull_request(args.api_root, args.token, args.repository, f"release-preparation/v{args.version}")
    if pull is not None:
        head_sha = str((pull.get("head") or {}).get("sha", ""))
        if head_sha != identity_sha:
            raise AbortError("release identity pull request head does not match the reserved identity")
        number = pull.get("number")
        if isinstance(number, bool) or not isinstance(number, int) or number < 1:
            raise AbortError("release identity pull request has no valid number")
        closed = api_request(
            args.api_root, args.token, "PATCH", f"/repos/{owner}/{name}/pulls/{number}", {"state": "closed"}
        )
        if not isinstance(closed, dict) or closed.get("state") != "closed":
            raise AbortError(f"pull request #{number} could not be closed")
        closed_pull_request = number

    deleted_refs: list[str] = []
    for ref_name in (preparation_ref, reservation_ref):
        if ref_sha(args.api_root, args.token, args.repository, ref_name) is None:
            continue
        quoted = urllib.parse.quote(ref_name, safe="/")
        api_request(args.api_root, args.token, "DELETE", f"/repos/{owner}/{name}/git/refs/{quoted}")
        deleted_refs.append(ref_name)

    return {
        "schema_version": 1,
        "repository": args.repository,
        "version": args.version,
        "identity_sha": identity_sha,
        "reason": reason,
        "closed_pull_request": closed_pull_request,
        "deleted_refs": deleted_refs,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--reason", required=True)
    parser.add_argument("--token", default="")
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--output", type=argparse.FileType("w", encoding="utf-8"), default=sys.stdout)
    args = parser.parse_args()
    if not args.token:
        parser.error("--token is required")
    try:
        result = abort(args)
    except (AbortError, release_policy.PolicyError) as error:
        print(str(error), file=sys.stderr)
        return 2
    json.dump(result, args.output, indent=2, sort_keys=True)
    args.output.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
