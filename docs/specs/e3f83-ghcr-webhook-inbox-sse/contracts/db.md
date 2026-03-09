# DB

## `github_packages_delivery_events`

Suggested columns:

- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `delivery_id TEXT NOT NULL`
- `received_at TEXT NOT NULL`
- `payload_json TEXT NOT NULL`

Indexes:

- primary key on `id`
- optional lookup index on `delivery_id` if later needed

Notes:

- `id` is the SSE cursor source.
- `payload_json` stores the already-materialized user-visible delivery snapshot used by the inbox SSE consumer.
- Events are appended only after the delivery state is user-visible; placeholder insertions must not emit rows.
