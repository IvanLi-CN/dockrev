# Notification Push Payload Contract

## Real notification payload

Existing fields remain unchanged:

```json
{
  "schema": "dockrev.notification.job_finished.v2",
  "kind": "job_finished",
  "title": "服务更新完成",
  "body": "示例服务已完成更新",
  "url": "/queue/job-id",
  "notificationId": "opaque-notification-id",
  "unreadCount": 3
}
```

Rules:

- `notificationId` identifies the persisted item and is required for real notification payloads.
- `unreadCount` is an absolute non-negative integer read after item persistence.
- The Service Worker may display the notification and set the Badge from `unreadCount`, but it does not call an authenticated read API.
- On notification click, the Service Worker transfers `notificationId` and the target URL to the app; the app performs the authenticated acknowledgement before completing navigation.

## Test and legacy payloads

- Notification test payloads omit `notificationId` and `unreadCount`.
- A legacy real payload that lacks `unreadCount` may still display its notification, but MUST NOT apply a local `+1` or `-1`; the next foreground synchronization repairs the Badge.
- A malformed or negative `unreadCount` is ignored for Badge mutation and does not alter the server item.
