# Dockrev Context

## Service Digest and Rollback Target

- `service digest` is the digest currently reported by the stack detail snapshot.
- `rollback target` is the single backend-selected version that can restore the service to the previous successful update state.
- A rollback target is valid only when its `currentDigest` matches the service digest in the same refresh generation.

## Refresh Generation

- `request generation` is the monotonically increasing stack refresh request id.
- A response from an older generation must not overwrite the service, rollback target, refreshing flag, or error state of a newer generation.
- A digest mismatch is a transient ordering condition between the service snapshot and rollback-target response. The frontend retries only within the current generation, at most five times with a 250ms delay.

## Neutral Refresh and Settlement

- `neutral refresh state` is the visible `回滚信息刷新中…` state shown while the service digest and rollback target are being reconciled. It must not expose an older unavailable reason.
- `update settled event` is the management SSE `jobs` event with `summary.terminal=true`. It triggers the current service detail refresh after the update job leaves `queued` or `running`.
- A successful target response exits neutral refresh and clears the transient refresh error. Retry exhaustion or a failed target request clears stale target and active rollback snapshots and leaves a retryable error.
