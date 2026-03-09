# HTTP APIs

## `GET /api/github-packages/webhook/deliveries/events`

- Auth: Forward Auth / same user requirement as other internal SSE routes
- Query:
  - `afterId?: number`
- Headers:
  - `Last-Event-ID?: number`
- Response: `200 text/event-stream`
- Default behavior:
  - When neither `afterId` nor `Last-Event-ID` is provided, the stream starts in tail-follow mode from the current latest delivery event id.
  - The effective cursor is `max(afterId, Last-Event-ID)`.
- SSE events:
  - `github_packages_delivery_event`
  - `github_packages_delivery_events_error`
