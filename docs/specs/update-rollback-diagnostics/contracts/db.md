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

- `NULL` means the job has no completed rollback evidence archive.
- The column stores one complete `tar.zst` archive for the whole update job. The archive contains one directory per failed service.
- `summary_json.rollbackEvidence` stores only availability and diagnostic metadata. It does not duplicate archive content.
- No additional index is required because the blob is read only by job ID and must not participate in jobs list queries.

### Write and Recovery Contract

- Candidate evidence is written to the private job spool before its rollback.
- At job finalization, the archive BLOB and terminal `rollbackEvidence` metadata are updated in one database transaction.
- The spool is deleted only after that transaction commits.
- A spool that remains after an interrupted archive operation is a recovery input. Recovery can attach its archive to the same job or record an explicit archive failure; it must not silently delete the spool.

### Migration and Compatibility

- The migration is additive. Existing rows retain `NULL`; no backfill is attempted because the historical candidate data no longer exists.
- A rollback to an application version that does not read the column leaves the stored archive intact.
- Existing terminal-job retention remains the evidence retention policy. Its GC path removes the blob with its job row and removes any matching private spool directory, including an archive-failed spool.
