# Update Rollback Evidence HTTP API Contract

## `GET /api/jobs/{job_id}`

- Scope: external
- Change: Modify
- Authorization: existing `require_user`

The existing `job.summary` may contain `rollbackEvidence` metadata when evidence handling ran. It is metadata only and never embeds archive bytes or raw logs.

```json
{
  "rollbackEvidence": {
    "status": "available",
    "archiveFormat": "tar",
    "compression": "zstd",
    "failedCandidates": 2,
    "archiveSizeBytes": 4096,
    "services": [
      { "serviceId": "service-a", "logsTruncated": false },
      { "serviceId": "service-b", "logsTruncated": true }
    ]
  }
}
```

- `status` is `available`, `incomplete`, or `absent`.
- `absent` omits the archive metadata from jobs that produced no failed candidate evidence.
- An archive can be `available` even when an individual service capture is incomplete; that service's metadata explains which collection step failed.
- Jobs without evidence omit `rollbackEvidence`.

## `GET /api/jobs/{job_id}/rollback-evidence`

- Scope: external
- Change: New
- Authorization: existing `require_user`

Returns the original BLOB without decompression or JSON embedding.

| Response | Meaning |
| --- | --- |
| `200 OK` | `Content-Type: application/zstd`; attachment filename ends in `.tar.zst`; body is the job archive. |
| `401` or `403` | Existing authorization behavior for a request rejected by `require_user`. |
| `404` | The job does not exist or has no completed archive. |
| `500` | Stored archive cannot be read; the response body contains no archive bytes. |

- The endpoint is not included in jobs list, job events, or live terminal APIs.
- Job Detail presents the download entry only when metadata reports `status=available`.
