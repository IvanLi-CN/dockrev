# HTTP API 契约

## Existing APIs

### `PUT /api/services/{service_id}/settings`

- Request shape remains unchanged.
- `repoUrl` omitted: preserve stored `repo_url` and `repo_url_auto_disabled`.
- `repoUrl: null` or blank string: persist `repo_url = NULL` and `repo_url_auto_disabled = true`.
- `repoUrl: <absolute http/https url>`: persist the URL and `repo_url_auto_disabled = false`.

### `GET /api/jobs` / `GET /api/jobs/{job_id}`

- May now return `type = "repo_link_backfill"`.
- No new endpoint is added.
