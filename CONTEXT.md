# Dockrev Context

## CI Release Assurance

**fast main gate**:
The `CI (main)` result that establishes prompt feedback for a main commit. It is not by itself permission to publish a release.
_Avoid_: complete release gate, source-build smoke

**source-build release gate**:
The release-blocking verification of a target SHA's Dockerfile source build and Compose deployment topology. It is distinct from the fast main gate and remains required before publication.
_Avoid_: optional Docker smoke, post-release validation

## Backup Retention

**backup cleanup eligibility**:
A successful backup artifact is eligible for automatic deletion once it is outside the retained set and its configured retention delay has elapsed. Eligibility is independent of the runtime state of unrelated services in the Stack.
_Avoid_: stack health gate, all-services-running requirement

**retained backup set**:
The newest successful, undeleted backup artifacts of a Stack, limited by its `keepLast` policy. Membership is the retention rule that protects an otherwise due artifact from automatic deletion.
_Avoid_: all successful backups, healthy Stack backups

**cleanup delayed**:
A backup artifact that is eligible for deletion and past its planned deletion time but has not reached a terminal cleanup outcome. It is a retryable cleanup state, not a retention state.
_Avoid_: retained backup, successful cleanup

**cleanup attempt**:
One execution that reconciles an eligible backup artifact with its storage path. It records when the attempt occurred and, when incomplete, the reason that the artifact remains present or cannot be verified.
_Avoid_: backup run, retention check

**verified missing backup**:
A terminal cleanup outcome in which an eligible artifact is absent from its managed storage path when Dockrev checks it. It is distinct from a backup deleted by Dockrev.
_Avoid_: deleted backup, cleanup failure

## Service Digest and Rollback Target

- `service digest` is the digest currently reported by the stack detail snapshot.
- `accepted deployment state` is the latest service deployment state accepted outside an in-progress mutating operation or established by that operation's terminal settlement. A candidate container is not part of the accepted deployment state.
- `transient operation observation` is a runtime or configuration observation made while a mutating operation overlaps the service. It may be used for operation progress and diagnosis, but it is not authoritative service state.
- `service mutation ownership` is the durable, exclusive right of one operation to replace a service's accepted deployment state. It begins before runtime side effects and ends only through service state settlement.
- `accepted-state generation` is the monotonic revision of a service's accepted deployment state. An observation can publish only when the generation it read is still current and no service mutation ownership is open.
- `service state settlement` is the terminal reconciliation that aligns the service snapshot with the final runtime state after a mutating operation.
- `rollback target` is the single backend-selected version that can restore the service to the previous successful update state.
- A rollback target is valid only when its `currentDigest` matches the service digest in the same refresh generation.

## Update Rollback Diagnostics

**candidate container**:
The post-apply container that runs an update candidate before Dockrev accepts it or begins automatic rollback. It is distinct from the rollback container that restores the prior image.
_Avoid_: new container, updated container

**rollback evidence**:
A bounded diagnosis artifact captured from a candidate container before automatic rollback begins. It preserves captured output verbatim, belongs to the update record, and is distinct from normal service logs.
_Avoid_: rollback logs, service logs

**health status**:
Docker's candidate-specific health evaluation: `starting`, `healthy`, or `unhealthy`. It does not by itself describe the container process state, restart count, exit error, or health-check output.
_Avoid_: container status, readiness result

**health-policy deadline**:
The time boundary after which a continuously `starting` candidate is treated as a health failure. It is derived solely from the candidate's effective health policy.
_Avoid_: fixed health timeout, Docker unhealthy time

## Compose Configuration

**source Compose configuration**:
The authored Compose files and explicit env-file inputs recorded for a Stack. It remains the auditable input to controlled Compose mutations.
_Avoid_: effective Compose configuration, rendered Compose configuration

**effective Compose configuration**:
The fully merged and interpolated service configuration emitted by the Stack's configured Compose CLI. It is the authoritative declaration for observing a service's image reference and tag.
_Avoid_: raw Compose YAML, source Compose configuration

## Refresh Generation

- `request generation` is the monotonically increasing stack refresh request id.
- A response from an older generation must not overwrite the service, rollback target, refreshing flag, or error state of a newer generation.
- A digest mismatch is a transient ordering condition between the service snapshot and rollback-target response. The frontend retries only within the current generation, at most five times with a 250ms delay.

## Neutral Refresh and Settlement

- `neutral refresh state` is the visible `回滚信息刷新中…` state shown while the service digest and rollback target are being reconciled. It must not expose an older unavailable reason.
- `update settled event` is the management SSE `jobs` event with `summary.terminal=true`. It triggers the current service detail refresh after the update job leaves `queued` or `running`.
- A successful target response exits neutral refresh and clears the transient refresh error. Retry exhaustion or a failed target request clears stale target and active rollback snapshots and leaves a retryable error.

## Management Event Transport

- `management event transport` is the one per-tab stream that carries management invalidations. Its health describes only that stream, never the health or freshness of independent service-log, resource-monitoring, or job-log streams.
- `transport connected` means the management event transport has an established stream. `transport reconnecting` means that stream is being replaced after a transport failure. These states do not claim that every page snapshot has finished refreshing.
- `page synchronization` is a page-owned REST refresh caused by a management invalidation or a transport reconnection. A page can be synchronizing while the management event transport is connected.
- `protocol-invalid management event` is an event whose payload cannot satisfy the management-event contract. It is a data-correctness condition, not evidence that the transport itself is disconnected.
- `observable management heartbeat` is a management event that proves a browser received the management event transport. It is distinct from a proxy keepalive comment, which can keep an HTTP connection alive without proving client delivery.
- `management transport session` is one owned, replaceable per-tab management stream. A foreground resume or recovery starts a fresh session before page synchronization resumes.
- `recovery synchronization` is the one-time page synchronization triggered after a transport session is connected, resumed, or found to have received a protocol-invalid event.

## Service Lifecycle Observability

- `service lifecycle event` is a durable record that a Dockrev-managed operation caused a service to stop, start, or restart. It identifies the affected service, origin, outcome, and relevant times.
- `operation interval` is the lifespan of a Dockrev-managed lifecycle action from acceptance through its terminal result. It is not necessarily the time during which the service was unavailable.
- `availability interval` is the time from a confirmed non-running state until a confirmed running state. It can remain open when a stopped service has not yet started again.
- `runtime lifecycle confirmation` is proof that every expected replica is running or that none is running. A partial or unknown replica state does not confirm a lifecycle transition.
- `operation-scoped lifecycle observer` is the Docker Engine event observer active for one Dockrev-managed operation. It combines observed Engine events with final container inspection to establish lifecycle boundaries without treating unrelated external activity as part of the operation.
- `lifecycle operation group` is the shared identity that relates service lifecycle events produced by one stack-level operation.
- `failed lifecycle attempt` is an unsuccessful lifecycle operation that is recorded for diagnosis but does not invent a service-state transition or close an availability interval.
- `incomplete lifecycle observation` is a lifecycle operation whose observation source did not establish every interval boundary. It preserves confirmed transitions but never fabricates the missing boundary.
- `system lifecycle log row` is a service-log entry derived from a service lifecycle event rather than emitted by the container. It remains distinct from container output.
- `lifecycle observability window` is the 30-day period in which lifecycle events remain available to match the longest resource-monitoring view.
