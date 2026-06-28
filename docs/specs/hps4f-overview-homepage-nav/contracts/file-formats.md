## Compose `services.<name>.labels`

The compose parser must continue to support both YAML forms:

### list form

```yaml
services:
  gitea:
    labels:
      - homepage.group=Developer
      - homepage.name=Gitea
      - homepage.icon=si-gitea
      - homepage.href=https://git.example.com
      - homepage.description=Git forge
```

### map form

```yaml
services:
  gitea:
    labels:
      homepage.group: Developer
      homepage.name: Gitea
      homepage.icon: si-gitea
      homepage.href: https://git.example.com
      homepage.description: Git forge
```

Extraction rules:

- Only `homepage.group`, `homepage.name`, `homepage.icon`, `homepage.href`, and `homepage.description` are extracted.
- `homepage.widget.*` and unrelated labels are ignored.
- Missing fields stay `null`; the parser must not invent defaults.

## `HomepageSnapshotV2`

Homepage local cache is now a single snapshot document.

```json
{
  "version": 2,
  "generatedAt": "2026-06-27T12:00:00Z",
  "lastCheckAt": "2026-06-27T11:58:10Z",
  "resourceSummary": {
    "enabled": true,
    "window": "1h",
    "generatedAt": "2026-06-27T12:00:00Z",
    "staleAfterSeconds": 60,
    "services": []
  },
  "cards": [
    {
      "id": "svc_api",
      "stackId": "stack_prod",
      "stackName": "prod",
      "serviceId": "svc_api",
      "serviceName": "api",
      "imageRef": "ghcr.io/acme/api:5.2.1",
      "groupName": "Brain",
      "title": "Acme API",
      "description": "API gateway",
      "href": "https://api.example.com",
      "icon": "si-github",
      "status": "updatable",
      "isDockrev": false,
      "service": {
        "id": "svc_api"
      }
    }
  ]
}
```

Rules:

- The resource summary and card list share one `generatedAt` and one persisted blob.
- Frontend must support best-effort migration from the legacy split cache:
  - `dockrev.homepage.nav.snapshot.v1`
  - `dockrev.homepage.resource.summary.v1`
- Legacy `status="updatable"` cards must not be migrated into actionable cached update candidates; the migrated card may stay visible, but its cached status must be non-actionable until live data arrives.
- Cache reads may mark `resourceSummary.services[*].stale=true` when the snapshot is older than `staleAfterSeconds`, but must preserve the metric values for fast first paint.
