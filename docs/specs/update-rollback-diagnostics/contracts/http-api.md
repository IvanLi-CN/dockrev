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
- `failedCandidates` counts candidate records captured into evidence. It is zero when spool setup fails before any candidate record can be written, even though the update may still roll back.
- Summary metadata is bounded: at most 32 service records, 4 capture errors per service, 256 characters per metadata text field, 16 top-level errors, and 512 characters per top-level error. If any item is omitted or shortened, `errors` includes a bounded note. `failedCandidates` still reports the full total; the limits do not apply to the downloadable archive manifest or raw log bytes.
- `absent` omits the archive metadata from jobs that produced no failed candidate evidence.
- A successfully completed update with no rollback does not create rollback evidence metadata or an archive download attachment.
- An archive can be `available` even when an individual service capture is incomplete; that service's metadata explains which collection step failed. `logsTruncated=false` means the command exited successfully, Dockrev reached EOF on the merged stdout/stderr output, and all received bytes were written; `true` means capture did not meet all three conditions or was recovered from an interrupted capture. It does not claim that Docker retained logs already removed by its own rotation.
- If storing an archive fails, job finalization records `status=incomplete` and a bounded persistence error; unless an archive is already attached, the existing download endpoint returns `404` and the private spool remains available for recovery.
- Jobs without evidence omit `rollbackEvidence`.

## `GET /api/jobs/{job_id}/rollback-evidence`

- Scope: external
- Change: Modify
- Authorization: existing `require_user`

Streams the original BLOB in bounded chunks without decompression or JSON embedding; the response bytes remain identical to the stored archive.

| Response | Meaning |
| --- | --- |
| `200 OK` | `Content-Type: application/zstd`; `Content-Length` equals the stored archive size; `Cache-Control: private, no-store`; attachment filename ends in `.tar.zst`; body is the original job archive. |
| `401` or `403` | Existing authorization behavior for a request rejected by `require_user`. |
| `404` | The job does not exist or has no attached archive. |
| `500` | The stored archive size cannot be read before the response starts. If a later chunk read fails after streaming begins, the response terminates early; `Content-Length` identifies the expected complete size. |

- The endpoint is not included in jobs list, job events, or live terminal APIs.
- Job Detail presents the download entry only when metadata reports `status=available`.
