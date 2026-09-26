# Notification Inbox Database Contract

## `notification_items`

The table is a durable application inbox, not a record of channel delivery attempts.

Logical fields:

- `id`: stable opaque primary key.
- `kind`: `job_finished`, `new_version_discovered` or `ghcr_webhook_anomaly`.
- `identity_key`: event-specific idempotency identity.
- `title`, `body`, `target_url`: rendered user-facing content and navigation target.
- `source_job_id`: nullable source job identity.
- `created_at`: item creation time.
- `read_at`: nullable acknowledgement time.

Identity rules:

- Job completion uses the terminal job identity.
- Candidate discovery uses the existing service-and-candidate-digest identity.
- GHCR anomaly items use owner, repository, anomaly state and an occurrence identity that changes only after recovery and recurrence.
- The persistence operation MUST be safe under concurrent retries. A duplicate observation returns the existing item or a no-op instead of adding a second unread item.

Indexes and cleanup:

- The implementation MUST support newest-first inbox reads and efficient unread counting.
- Single-item read and mark-all-read MUST update `read_at` atomically with their returned count.
- A retention operation MAY delete read items older than 90 days. It MUST NOT delete unread items because of age, source resolution or delivery failure.
- Deleting retained read history MUST NOT change the unread count or Badge.

## Transaction boundary

- A qualified event MUST persist its notification item before external channel delivery is reported as successful.
- External channel retries MUST reference the item identity and MUST NOT recreate the item.
- Push subscription removal or delivery failure MUST leave the item and its read state unchanged.
