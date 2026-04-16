## `services`

Add a nullable JSON text column:

- `homepage_json TEXT NULL`

Stored shape:

```json
{
  "group": "Developer",
  "name": "Gitea",
  "icon": "si-gitea",
  "href": "https://git.example.com",
  "description": "Git forge"
}
```

Rules:

- Only the five basic Homepage fields are stored.
- `NULL` means the service currently has no Homepage metadata.
- Discovery sync must upsert, update, or clear `homepage_json` so persisted data matches the latest compose truth.
