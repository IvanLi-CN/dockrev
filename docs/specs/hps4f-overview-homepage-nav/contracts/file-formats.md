## Compose `services.<name>.labels`

The compose parser must support both YAML forms:

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
- `homepage.widget.*` and any unrelated labels are ignored.
- Missing fields stay `null`; the parser must not invent defaults.
