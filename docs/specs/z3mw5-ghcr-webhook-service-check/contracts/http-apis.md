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
- Delivery outcome payload/logging must record:
  - `repo`
  - `deliveryId`
  - `matchedServiceIds`
  - `checkJobIds`
  - `reusedJobIds`
  - `fallbackUsed`
  - `fallbackJobId`

### Matching contract

- Normalize webhook repo to lowercase `ghcr.io/<owner>/<repo>`.
- Normalize each service image ref to its repository component and compare case-insensitively.
- Ignore archived stacks and archived services.
