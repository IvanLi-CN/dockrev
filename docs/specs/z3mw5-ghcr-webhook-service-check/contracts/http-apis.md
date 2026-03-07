# HTTP API contracts

## `POST /api/webhooks/github-packages`

### Stable surface

- Path, signature validation, selected-repo filtering, and delivery de-dup semantics stay unchanged.
- Response remains synchronous acknowledgment of whether Dockrev accepted or ignored the delivery.

### Behavior changes

- When payload resolves to `owner/repo` and that repo matches one or more non-archived managed services:
  - enqueue or reuse `check.service` jobs for matched services;
  - do not enqueue `discovery.all` by default.
- When payload cannot resolve to repo, or repo resolves but matches zero managed services:
  - enqueue or reuse one `discovery.all` fallback job.
- Delivery acknowledgement payload/logging must record:
  - `repo`
  - `deliveryId`
  - `matchedServiceIds`
  - `jobIds`
  - `reusedJobIds`
  - `fallbackUsed`
- Response keeps `jobId` as the primary alias for backward-compatible single-job navigation.
- Delivery history APIs persist the full `jobIds` list so multi-service matches remain traceable after the synchronous webhook response is gone.

### Matching contract

- Normalize webhook repo to lowercase `ghcr.io/<owner>/<repo>`.
- Normalize each service image ref to its repository component and compare case-insensitively.
- Ignore archived stacks and archived services.
