#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any
from urllib import error, parse, request

COMMENT_MARKER = "<!-- codex-release-version-comment -->"
BOT_LOGIN = "github-actions[bot]"


class CommentError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create or update the PR release-version issue comment.")
    parser.add_argument("--github-repository", required=True)
    parser.add_argument("--github-token", required=True)
    parser.add_argument("--pr-number", required=True, type=int)
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--release-channel", required=True, choices=("stable", "rc"))
    parser.add_argument("--release-url", required=True)
    parser.add_argument("--workflow-run-url", required=True)
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))
    return parser.parse_args()


def github_request_json(
    api_root: str,
    token: str,
    method: str,
    path: str,
    *,
    body: dict[str, Any] | None = None,
    query: dict[str, Any] | None = None,
) -> Any:
    url = f"{api_root.rstrip('/')}{path}"
    if query:
        url += "?" + parse.urlencode(query)
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "dockrev-release-pr-comment",
    }
    data = None
    if body is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(body).encode("utf-8")
    req = request.Request(url, headers=headers, data=data, method=method)
    try:
        with request.urlopen(req) as resp:
            payload = resp.read()
            if not payload:
                return None
            return json.loads(payload.decode("utf-8"))
    except error.HTTPError as exc:
        body_text = exc.read().decode("utf-8", errors="replace")
        raise CommentError(f"GitHub API error on {path}: {exc.code} {body_text}") from exc
    except error.URLError as exc:
        raise CommentError(f"GitHub API request failed on {path}: {exc}") from exc


def export_key_values(values: dict[str, Any], github_output: str) -> None:
    lines = []
    for key, value in values.items():
        rendered = "" if value is None else str(value)
        if "\n" in rendered:
            lines.append(f"{key}<<__CODEX__")
            lines.append(rendered)
            lines.append("__CODEX__")
        else:
            lines.append(f"{key}={rendered}")
    payload = "\n".join(lines) + "\n"
    if github_output:
        with Path(github_output).open("a", encoding="utf-8") as handle:
            handle.write(payload)
    else:
        sys.stdout.write(payload)


def render_comment_body(*, release_tag: str, release_channel: str, release_url: str, workflow_run_url: str) -> str:
    return "\n".join(
        [
            COMMENT_MARKER,
            "Release published for this PR.",
            "",
            f"- Channel: `{release_channel}`",
            f"- Version: `{release_tag}`",
            f"- Release: {release_url}",
            f"- Workflow run: {workflow_run_url}",
        ]
    )


def issue_comments(api_root: str, repository: str, token: str, pr_number: int) -> list[dict[str, Any]]:
    owner, repo = repository.split("/", 1)
    comments: list[dict[str, Any]] = []
    page = 1
    while True:
        payload = github_request_json(
            api_root,
            token,
            "GET",
            f"/repos/{owner}/{repo}/issues/{pr_number}/comments",
            query={"per_page": 100, "page": page},
        )
        if not isinstance(payload, list):
            raise CommentError("GitHub API returned a malformed issue comments payload")
        comments.extend(comment for comment in payload if isinstance(comment, dict))
        if len(payload) < 100:
            return comments
        page += 1


def comment_body(comment: dict[str, Any]) -> str:
    body = comment.get("body")
    return body if isinstance(body, str) else ""


def comment_id(comment: dict[str, Any]) -> int:
    value = comment.get("id")
    if not isinstance(value, int):
        raise CommentError("Issue comment payload is missing a numeric id")
    return value


def comment_login(comment: dict[str, Any]) -> str:
    user = comment.get("user")
    if not isinstance(user, dict):
        return ""
    login = user.get("login")
    return login if isinstance(login, str) else ""


def marker_comments(comments: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [comment for comment in comments if COMMENT_MARKER in comment_body(comment)]


def owned_marker_comments(comments: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [comment for comment in marker_comments(comments) if comment_login(comment) == BOT_LOGIN]


def delete_comment(api_root: str, repository: str, token: str, comment_id_value: int) -> None:
    owner, repo = repository.split("/", 1)
    github_request_json(
        api_root,
        token,
        "DELETE",
        f"/repos/{owner}/{repo}/issues/comments/{comment_id_value}",
    )


def prune_extra_owned_marker_comments(
    api_root: str,
    repository: str,
    token: str,
    comments: list[dict[str, Any]],
    *,
    keep_comment_id: int,
) -> None:
    for comment in owned_marker_comments(comments):
        current_id = comment_id(comment)
        if current_id == keep_comment_id:
            continue
        delete_comment(api_root, repository, token, current_id)


def select_comment_target(comments: list[dict[str, Any]]) -> tuple[str, int | None, str]:
    marked = marker_comments(comments)
    foreign = [comment for comment in marked if comment_login(comment) != BOT_LOGIN]
    if foreign:
        comment = max(foreign, key=comment_id)
        return (
            "skip_foreign_marker",
            None,
            f"Marker comment already owned by {comment_login(comment) or 'unknown user'} (comment_id={comment_id(comment)}); cannot satisfy release comment contract.",
        )

    owned = [comment for comment in marked if comment_login(comment) == BOT_LOGIN]
    if not owned:
        return ("create", None, "")
    comment = max(owned, key=comment_id)
    return ("update", comment_id(comment), "")


def verify_release_comment_contract(comments: list[dict[str, Any]], *, expected_body: str) -> int:
    foreign = [comment for comment in marker_comments(comments) if comment_login(comment) != BOT_LOGIN]
    if foreign:
        comment = max(foreign, key=comment_id)
        raise CommentError(
            f"Marker comment already owned by {comment_login(comment) or 'unknown user'} (comment_id={comment_id(comment)}); cannot satisfy release comment contract."
        )

    owned = owned_marker_comments(comments)
    if len(owned) != 1:
        raise CommentError(f"Expected exactly 1 bot-owned marker comment after publish, found {len(owned)}")

    verified = owned[0]
    if comment_body(verified) != expected_body:
        raise CommentError("Bot-owned marker comment body does not match the published release payload")
    return comment_id(verified)


def upsert_release_comment(
    *,
    api_root: str,
    repository: str,
    token: str,
    pr_number: int,
    release_tag: str,
    release_channel: str,
    release_url: str,
    workflow_run_url: str,
) -> dict[str, Any]:
    if pr_number <= 0:
        raise CommentError("pr_number must be a positive integer")

    owner, repo = repository.split("/", 1)
    comments = issue_comments(api_root, repository, token, pr_number)
    action, target_comment_id, warning = select_comment_target(comments)
    body = render_comment_body(
        release_tag=release_tag,
        release_channel=release_channel,
        release_url=release_url,
        workflow_run_url=workflow_run_url,
    )

    if action == "skip_foreign_marker":
        raise CommentError(warning)

    if action == "update":
        payload = github_request_json(
            api_root,
            token,
            "PATCH",
            f"/repos/{owner}/{repo}/issues/comments/{target_comment_id}",
            body={"body": body},
        )
    else:
        payload = github_request_json(
            api_root,
            token,
            "POST",
            f"/repos/{owner}/{repo}/issues/{pr_number}/comments",
            body={"body": body},
        )

    if not isinstance(payload, dict):
        raise CommentError("GitHub API returned a malformed issue comment response")
    keep_comment_id = comment_id(payload)
    prune_extra_owned_marker_comments(
        api_root,
        repository,
        token,
        issue_comments(api_root, repository, token, pr_number),
        keep_comment_id=keep_comment_id,
    )
    verified_comment_id = verify_release_comment_contract(
        issue_comments(api_root, repository, token, pr_number),
        expected_body=body,
    )
    return {
        "comment_status": action,
        "comment_id": verified_comment_id,
        "comment_body": body,
    }


def main() -> int:
    args = parse_args()
    try:
        result = upsert_release_comment(
            api_root=args.api_root,
            repository=args.github_repository,
            token=args.github_token,
            pr_number=args.pr_number,
            release_tag=args.release_tag,
            release_channel=args.release_channel,
            release_url=args.release_url,
            workflow_run_url=args.workflow_run_url,
        )
        export_key_values(result, args.github_output)
        return 0
    except CommentError as exc:
        print(f"release_pr_comment.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
