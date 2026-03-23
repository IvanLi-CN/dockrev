# HTTP APIs

## Modified

### `GET /api/stacks/{id}`

- `stack.services[].settings.repoUrl?: string | null`

### `GET /api/services/{service_id}/settings`

```json
{
  "autoRollback": true,
  "backupTargets": {
    "bindPaths": {},
    "volumeNames": {}
  },
  "repoUrl": "https://github.com/owner/repo"
}
```

### `PUT /api/services/{service_id}/settings`

- Accepts the same `repoUrl?: string | null` field.
- If `repoUrl` is omitted, the existing stored value is preserved for backward compatibility with older clients.
- Empty string is normalized to `null`.
- Only absolute `http` or `https` URLs are accepted when non-null.

## New

### `POST /api/services/{service_id}/repo-link/infer`

Response:

```json
{
  "repoUrl": "https://github.com/owner/repo",
  "strategy": "oci_source",
  "reason": null
}
```

Rules:

- `404` only when the service does not exist.
- `200` + `{"repoUrl": null, "strategy": "none", "reason": "..."}` when no repo can be inferred.
- `strategy` enum: `oci_source | ghcr_exact | none`
