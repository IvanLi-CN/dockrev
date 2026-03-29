# DB 契约

## `services`

- Add column: `repo_url_auto_disabled INTEGER NOT NULL DEFAULT 0`
- Semantics:
  - `0`: service is eligible for automatic repo URL backfill when `repo_url IS NULL`
  - `1`: service was explicitly cleared by the user and must be skipped by automatic backfill jobs

## Sync rules

- New services default to `repo_url = NULL` and `repo_url_auto_disabled = 0`.
- When only the image tag or digest changes but the image repository stays the same, preserve both `repo_url` and `repo_url_auto_disabled`.
- When the image repository changes, clear `repo_url` but preserve `repo_url_auto_disabled`.
- Automatic backfill may only write rows where `repo_url IS NULL AND repo_url_auto_disabled = 0`.
