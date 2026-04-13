## `GET /api/stacks` / `GET /api/stacks/{id}` / related `Service` payloads

`Service` objects gain an optional `homepage` object:

```json
{
  "id": "svc_123",
  "name": "gitea",
  "image": {
    "raw": "docker.gitea.com/gitea:1.23"
  },
  "homepage": {
    "group": "Developer",
    "name": "Gitea",
    "icon": "si-gitea",
    "href": "https://git.example.com",
    "description": "Git forge"
  }
}
```

Rules:

- `homepage` is nullable / omittable when the service has no Homepage metadata.
- The object only contains the five basic Homepage fields: `group`, `name`, `icon`, `href`, `description`.
- This round must ignore any `homepage.widget.*` compose labels; they must not appear in API payloads.
- Existing consumers that ignore unknown fields remain compatible.
