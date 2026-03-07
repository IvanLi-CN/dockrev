# Event contracts

## `dockrev.notification.new_version_discovered.v2`

### Stable schema

- Payload schema and channel-specific renderers remain unchanged.

### Trigger change

- Previous behavior: only scheduled checks emitted this notification.
- New behavior: checks with `reason=schedule` or `reason=webhook` emit this notification when `newVersions.count > 0`.
- Checks with `reason=ui` must remain silent for this notification type.

### Observability

- Job logs for webhook-triggered checks must include normal `notify:` success/failure entries so operators can verify delivery from Queue detail pages.
