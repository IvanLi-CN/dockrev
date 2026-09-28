# Update Rollback Evidence Database Contract

## Rollback Evidence Archive

- Scope: internal
- Change: Modify
- Affected table: `jobs`

### Schema Delta

```sql
ALTER TABLE jobs
  ADD COLUMN rollback_evidence_tar_zstd BLOB NULL;
```

- `NULL` means the job has no attached rollback evidence archive.
- A successfully completed update with no candidate rollback leaves this column `NULL` and creates no evidence spool or archive file.
- The column stores one complete `tar.zst` archive for the whole update job. The archive contains one directory per failed service.
- The Docker CLI stdout and stderr bytes for `docker logs --timestamps` are merged at the process boundary and streamed to one private per-service temporary file, then atomically renamed to `container.log`; no application-side parsing, redaction, re-encoding, or size cap is applied. A 300-second watchdog or write failure preserves partial bytes and sets `logsTruncated=true` in metadata.
- The archive file is streamed into the existing BLOB using incremental SQLite I/O inside the same transaction that stores `rollbackEvidence` metadata; recovery uses the same bounded-memory write path. The authorized download reads fixed-size chunks from that BLOB. These paths do not materialize the complete archive in application memory or retain another durable archive copy.
- `summary_json.rollbackEvidence` stores only availability and diagnostic metadata. It does not duplicate archive content.
- No additional index is required because the blob is read only by job ID and must not participate in jobs list queries.

### Write and Recovery Contract

- Candidate evidence is written to the private job spool before its rollback.
- At job finalization, the archive BLOB and terminal `rollbackEvidence` metadata are updated in one database transaction.
- If archive persistence fails, finalization retries without a new BLOB, records `rollbackEvidence.status=incomplete` with a bounded error, and keeps the spool for later recovery. The spool is deleted only after the archive transaction commits.
- A checkpoint manifest is written before starting log capture. If capture is interrupted, recovery promotes the partial log file to `container.log`, keeps `logsTruncated=true`, and archives it with the same job. Startup applies the existing job-recovery flow before evidence recovery; attaching evidence does not change the status already determined by that flow. A spool that remains after an interrupted archive operation is also a recovery input. Recovery may attach its archive to the same job; if it cannot, it keeps the spool for a later retry instead of silently deleting it.
- Concurrent recovery scans are serialized so only one scan can rebuild or attach a job archive at a time.

### Migration and Compatibility

- The migration is additive. Existing rows retain `NULL`; no backfill is attempted because the historical candidate data no longer exists.
- A rollback to an application version that does not read the column leaves the stored archive intact.
- Existing terminal-job retention remains the evidence retention policy once a job is terminal. Its GC path removes the blob with its job row and removes any matching private spool directory, including an archive-failed spool.
