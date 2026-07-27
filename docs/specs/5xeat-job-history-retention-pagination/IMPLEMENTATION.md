# Implementation

- SQLite migration `0013_job_history_retention` rebuilds `backups` with nullable `job_id ... ON DELETE SET NULL`, creates `job_service_targets`, backfills direct and summary target links, and creates pagination/retention indexes.
- Resource GC runs after startup and then once per minute, deleting at most ten 10,000-row batches and yielding between batches; it is independent from sampler in-flight invalidation and never repopulates sampling caches. Terminal job GC remains bounded. SQLite page reclamation remains a separate operator maintenance action.
- `/api/jobs` uses a base64url JSON cursor and fetches one extra row to derive `nextCursor`.
