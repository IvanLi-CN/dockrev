# Events

## `github_packages_delivery_event`

```json
{
  "type": "github_packages_delivery_event",
  "deliveryId": "uuid-or-github-delivery-id",
  "receivedAt": "2026-03-09T12:34:56Z",
  "firstReceivedAt": "2026-03-09T12:34:56Z",
  "owner": "acme",
  "repo": "widgets",
  "event": "package",
  "action": "published",
  "decision": "processed",
  "reason": null,
  "responseStatus": 200,
  "jobId": "job_xxx",
  "jobIds": ["job_xxx"],
  "attemptCount": 1
}
```

## `github_packages_delivery_events_error`

```json
{
  "type": "github_packages_delivery_events_error",
  "error": "human-readable error"
}
```
