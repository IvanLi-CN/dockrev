# Store Rollback Evidence with Its Update Job

When an automatic rollback replaces a candidate container, its logs and runtime state are otherwise lost. Store one `tar.zst` archive in the update job row, with a small summary describing its availability, so evidence follows the job's existing authorization and retention lifecycle. The archive is assembled from a private per-job spool after all candidate handling completes, but each candidate's files are atomically spooled before that candidate is rolled back.

## Considered Options

- Append raw output to `job_logs`: rejected because normal command logging and live terminal delivery must not carry complete candidate output, and the table is not a single bounded binary job artifact.
- Store an unframed compressed text stream: rejected because a batch update can produce evidence for several services and operators need a reliable per-service boundary.
- Store a separate evidence table or external file as the durable artifact: rejected because the confirmed contract requires one binary field on the update record and evidence must be purged with the job.

## Consequences

- The evidence archive contains original captured log output without redaction and is available only through the existing authorized job access boundary.
- A failed spool, archive, or database write does not cancel an automatic rollback; its diagnostic state remains explicit in the update summary.
- Existing terminal-job retention deletes the archive together with its job row.
