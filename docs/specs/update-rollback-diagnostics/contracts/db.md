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
- Summary metadata is bounded for both normal finalization and recovery: at most 32 service records, 4 capture errors per service, 256 characters per metadata text field, 16 top-level errors, and 512 characters per top-level error. Any omitted or shortened metadata is identified by a bounded note, while `failedCandidates` remains the total count. These limits do not alter the archive manifest or raw log bytes.
- No additional index is required because the blob is read only by job ID and must not participate in jobs list queries.

### Write and Recovery Contract

- Candidate evidence is written to the private job spool before its rollback.
- At job finalization, the archive BLOB and terminal `rollbackEvidence` metadata are updated in one database transaction.
- If archive persistence fails, finalization retries without a new BLOB, records `rollbackEvidence.status=incomplete` with a bounded error, and keeps the spool for later recovery. The spool is deleted only after the archive transaction commits.
- Recovery attaches an archive only with a transaction-local conditional write when `rollback_evidence_tar_zstd IS NULL`. If another finalizer has already committed the BLOB, recovery leaves both that BLOB and its summary unchanged.
- If the finish call returns an error, finalization checks the persisted terminal status, finish timestamp, `available` summary, and matching archive BLOB size before classifying it as a pre-commit archive failure. When those values prove the archive transaction committed, it cleans the spool and propagates the post-commit error without rewriting evidence metadata; if the check itself fails, it retains the spool.
- If spool initialization fails and candidate health failure later triggers rollback, terminal job summary records `rollbackEvidence.status=incomplete` with a bounded setup error; no archive BLOB is written.
- A checkpoint manifest is written before starting log capture. If capture is interrupted, recovery promotes the partial log file to `container.log`, keeps `logsTruncated=true`, and archives it with the same job. Startup runs generic incomplete-job recovery before evidence recovery. Evidence recovery itself does not change job status; for jobs handled by the existing deferred interrupted-update-backup recovery, evidence may be attached before that deferred recovery completes, and the existing recovery path retains its status semantics. A spool that remains after an interrupted archive operation is also a recovery input. If manifest parsing, partial-log recovery, archive rebuilding, archive-size lookup, or archive attachment fails, recovery records bounded `rollbackEvidence.status=incomplete` metadata when the job row is available and no archive BLOB exists, then keeps the spool for retry. It never overwrites summary metadata for an already attached archive.
- For a terminal job with an attached archive BLOB, startup removes leftover spool, archive, and partial-archive files before reading its manifest. A missing or corrupt manifest cannot keep a second raw-log copy on disk.
- Startup also scans for archive-only and part-only files whose spool directory is absent. Failed cleanup emits a warning and preserves files that could not be removed; an existing archive BLOB's summary is not overwritten.
- After the finalization transaction commits, a failed archive-size lookup preserves the local spool and archive but does not turn the completed job into an error; cleanup proceeds only after a positive BLOB-size lookup. Deletion failures are reported as warnings, and cleanup still attempts each remaining local copy without changing committed metadata.
- Concurrent recovery scans are serialized so only one scan can rebuild or attach a job archive at a time.

### Migration and Compatibility

- The migration is additive. Existing rows retain `NULL`; no backfill is attempted because the historical candidate data no longer exists.
- A rollback to an application version that does not read the column leaves the stored archive intact.
- Existing terminal-job retention remains the evidence retention policy once a job is terminal. Its GC path removes the blob with its job row and removes any matching private spool directory, including an archive-failed spool.
