# Implementation

- SQLite migration `0013_job_history_retention` rebuilds `backups` with nullable `job_id ... ON DELETE SET NULL`, creates `job_service_targets`, backfills direct and summary target links, and creates pagination/retention indexes.
- Resource and terminal job GC run once after startup and then hourly, deleting bounded batches only. SQLite page reclamation remains a separate operator maintenance action.
- `/api/jobs` uses a base64url JSON cursor and fetches one extra row to derive `nextCursor`.
