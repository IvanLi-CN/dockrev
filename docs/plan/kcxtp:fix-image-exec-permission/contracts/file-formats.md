# Contracts: File formats（#kcxtp）

## Dockrev GHCR image：运行契约

### Name

- Image: `ghcr.io/ivanli-cn/dockrev:<tag>`

### Filesystem layout (required)

- `/usr/local/bin/dockrev`
  - Required: yes
  - Mode: executable (recommended `0755`)
- `/usr/local/bin/dockrev-supervisor`
  - Required: yes
  - Mode: executable (recommended `0755`)

### Default command (runtime)

- Default `CMD`: `["/usr/local/bin/dockrev"]`

### Validation (blocking)

- Image filesystem:
  - `docker run --rm --entrypoint /bin/sh ghcr.io/ivanli-cn/dockrev:<tag> -c 'ls -l /usr/local/bin/dockrev /usr/local/bin/dockrev-supervisor && test -x /usr/local/bin/dockrev && test -x /usr/local/bin/dockrev-supervisor'`
- Runtime smoke (recommended):
  - Build/run the image and validate `GET /api/health == ok` and `GET /api/version` matches the released version (reuse `./.github/scripts/smoke-test.sh` logic).

### Compatibility rules

- Keep binary paths stable (`/usr/local/bin/dockrev*`).
- Do not ship images that require external chmod/workarounds to start.
