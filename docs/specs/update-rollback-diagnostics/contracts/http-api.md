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
- `failedCandidates`, when present, counts candidate records captured into evidence. It is zero when spool setup fails before any candidate record can be written, even though the update may still roll back. If manifest recovery fails before any count is known and no earlier count was recorded, the field is omitted rather than reporting a false zero; a previously known count, including zero, is preserved.
- Summary metadata is bounded: at most 32 service records, 4 capture errors per service, 256 characters per metadata text field, 16 top-level errors, and 512 characters per top-level error. If any item is omitted or shortened, `errors` includes a bounded note. A known `failedCandidates` value still reports the full total; the limits do not apply to the downloadable archive manifest or raw log bytes.
- Docker `State.Error` is omitted from this ordinary job summary because runtime details may be sensitive. Its original value remains only in the private evidence archive manifest returned through the authorized download endpoint.
- Jobs list and detail responses also sanitize summaries persisted by older versions: legacy `stateError` values are omitted and the current service/error count and text limits are applied without changing the private archive.
- `absent` omits the archive metadata from jobs that produced no failed candidate evidence.
- A successfully completed update with no rollback does not create rollback evidence metadata or an archive download attachment.
- An archive can be `available` even when an individual service capture is incomplete; that service's metadata explains which collection step failed. `logsTruncated=false` means the command exited successfully, Dockrev reached EOF on the merged stdout/stderr output, and all received bytes were written; `true` means capture did not meet all three conditions or was recovered from an interrupted capture. It does not claim that Docker retained logs already removed by its own rotation.
- If storing an archive fails, job finalization records `status=incomplete` and a bounded persistence error; unless an archive is already attached, the existing download endpoint returns `404` and the private spool remains available for recovery.
- Startup binds the API listener before reconstructing legacy evidence archives in the background. An existing job can therefore be readable with `status=incomplete` while recovery is still running; the download endpoint returns `404` until the archive BLOB commits, after which the summary reports `available` and the same authorized endpoint serves it.
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
