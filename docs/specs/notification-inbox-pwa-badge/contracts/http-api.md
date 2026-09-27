# Notification Inbox HTTP Contract

## Authentication and ownership

- All inbox endpoints use the existing Dockrev authentication boundary.
- The current deployment model has one shared operator console. Read state is stored by the deployment and is shared by all authenticated tabs and installed clients.
- The existing `GET /api/notifications`, `PUT /api/notifications` and `POST /api/notifications/test` settings endpoints remain separate from this contract.

## `GET /api/notifications/inbox`

Query parameters:

- `limit`: optional integer, default `50`, maximum `50`.
- `cursor`: optional opaque cursor returned by the previous response.

Response shape:

```json
{
  "items": [
    {
      "id": "opaque-notification-id",
      "kind": "job_finished",
      "title": "服务更新完成",
      "body": "示例服务已完成更新",
      "url": "/queue/job-id",
      "sourceJobId": "job-id",
      "createdAt": "2026-09-26T08:00:00Z",
      "readAt": null
    }
  ],
  "nextCursor": "opaque-cursor-or-null",
  "unreadCount": 1
}
```

Rules:

- Items are ordered newest first by creation time and stable item ID.
- `kind` is one of `job_finished`, `new_version_discovered` or `ghcr_webhook_anomaly`.
- `sourceJobId` is omitted or null when the event has no single source job.
- `readAt` is null for unread items and an RFC 3339 timestamp for read items.
- The cursor is opaque and must not be interpreted by the frontend.

## `GET /api/notifications/unread-count`

Response:

```json
{
  "unreadCount": 1
}
```

The value is the authoritative count at the time of the response.

## `POST /api/notifications/{notificationId}/read`

- Request body: empty JSON object or no body.
- The operation is idempotent.
- Reading an unknown or already-retained read item does not create another item.

Response:

```json
{
  "notificationId": "opaque-notification-id",
  "readAt": "2026-09-26T08:02:00Z",
  "unreadCount": 0
}
```

## `POST /api/notifications/read-all`

- Request body: empty JSON object or no body.
- The operation marks all current unread items as read in one transaction.

Response:

```json
{
  "readAt": "2026-09-26T08:03:00Z",
  "unreadCount": 0
}
```

## Frontend synchronization channel

- The frontend uses `BroadcastChannel("dockrev:notifications")` when available.
- Broadcast messages are invalidation hints and may include `unreadCount`, `notificationId` and an operation such as `created`, `read` or `read-all`.
- A receiving tab MUST refresh from REST when it needs item content or when the broadcast count is missing. It MUST NOT treat a broadcast delta as the authoritative count.
