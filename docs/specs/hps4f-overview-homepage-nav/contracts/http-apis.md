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

## `GET /api/services/resource-usage/overview?window=15m|1h|6h`

Returns the latest resource summary for active services in one aggregate response.

```json
{
  "enabled": true,
  "window": "1h",
  "generatedAt": "2026-04-28T14:20:00.000Z",
  "staleAfterSeconds": 60,
  "services": [
    {
      "serviceId": "svc_123",
      "sampledAt": "2026-04-27T14:19:30.000Z",
      "cpuPercent": 2.4,
      "memUsedBytes": 235929600,
      "memLimitBytes": 1073741824,
      "netRxRateBps": 18432.2,
      "netTxRateBps": 9216.1,
      "stale": false,
      "sampleCount": 121
    }
  ]
}
```

Rules:

- Requires the same app authorization as other service APIs.
- `window` accepts `15m`, `1h`, or `6h`; unsupported values return an invalid-argument response.
- `services` contains active, non-archived services. Services without samples remain present with nullable metric fields and `sampleCount=0`.
- Network RX/TX rates are derived from the latest two monotonic byte counters in the requested window; missing or reset counters return `null`.
- `stale` is true when the latest sample is older than `max(sample_interval_seconds * 2, 60)`.
- When resource monitoring is disabled, the endpoint returns `200` with `enabled=false` and an empty `services` array so Overview can degrade without blocking navigation.

## `GET /api/homepage-icons/{provider}/{path}`

Proxies the built-in Homepage icon sources through the Dockrev origin so the navigation page is not dependent on cross-origin image policy for known providers.

Allowed forms:

- `/api/homepage-icons/iconify/mdi/{name}.svg?color=%23dbeafe`
- `/api/homepage-icons/iconify/simple-icons/{name}.svg?color=%23dbeafe`
- `/api/homepage-icons/selfhst/{svg|png|webp}/{name}.{ext}`
- `/api/homepage-icons/dashboard/{svg|png|webp}/{name}.{ext}`

Rules:

- Requires the same app authorization boundary as other app routes.
- `provider` is limited to `iconify`, `selfhst`, and `dashboard`.
- Iconify collections are limited to `mdi` and `simple-icons`.
- Static icon extensions are limited to `svg`, `png`, and `webp`.
- Filenames may only contain ASCII letters, digits, `.`, `_`, and `-`, and must not contain traversal segments.
- `color` is accepted only for Iconify and must be a hex color.
- Successful responses set an image content type and public cache headers.
- SVG responses also set a restrictive `Content-Security-Policy` that sandboxes the document and disables script execution.
- Upstream errors, unsupported providers, unsafe paths, oversized responses, or unknown content families must fail closed so the web client can render the default fallback icon.
- Absolute `homepage.icon` URLs supplied by compose are not proxied by this endpoint and remain direct browser image loads.
