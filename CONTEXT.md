# Dockrev Context

## CI Release Assurance

**fast main gate**:
The `CI (main)` result that establishes prompt feedback for a main commit. It is not by itself permission to publish a release.
_Avoid_: complete release gate, source-build smoke

**source-build release gate**:
The release-blocking verification of a target SHA's Dockerfile source build and Compose deployment topology. It is distinct from the fast main gate and remains required before publication.
_Avoid_: optional Docker smoke, post-release validation

**release preparation artifact**:
An unpublished, exact-SHA Web and binary deliverable prepared without package, tag, GitHub Release, or image publication authority. Release may consume it only after validating its provenance and the independent source-build release gate; it is not source-build proof.
_Avoid_: release proof, source-build artifact

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

## Version Discovery and Auto Deployment

**floating tag**:
A tag such as `latest`, `stable`, or `main` whose spelling does not identify one semantic release version. A floating tag can point to a different image digest over time.
_Avoid_: mutable digest, resolved version

**resolved version**:
The semantic release version associated with one image digest after examining the registry's version evidence. It supplements the raw image tag and does not replace the tag's identity.
_Avoid_: rewritten tag, guessed current version

**version inference**:
The process of finding a resolved version for a digest when the raw tag is not itself a semantic version. It is supplementary discovery information; its temporary absence does not invalidate the digest candidate.
_Avoid_: update readiness, deployment result

**version evidence**:
A digest-bound fact that can support a resolved version, ordered by the candidate settlement contract. Registry tag snapshots and a valid OCI image version label are evidence for the exact digest, not proof that a tag points to that digest in the future.
_Avoid_: image release metadata, mutable tag state

**candidate readiness**:
The semantic state of a discovered image candidate: `ready` when the information required by its matcher is available, `awaiting_inference` when a floating tag still needs resolution, and `unresolved` when resolution reached a terminal failure without a usable version.
_Avoid_: check job status, update job status

**candidate settlement**:
The authoritative resolution of a discovered candidate's raw tag, digest evidence, resolved version, readiness, and failure reason. Consumers must use the same settlement rather than independently interpreting partial evidence.
_Avoid_: notification formatting, policy match result

**inference retry budget**:
The bounded number and schedule of additional attempts to resolve a candidate after a transient inference failure. Exhausting the budget produces an explicit unresolved outcome; it does not authorize an unsafe SemVer fallback.
_Avoid_: infinite polling, update retry

**superseded candidate**:
A previously discovered candidate whose digest is no longer the service's newest eligible candidate. It remains historical evidence but cannot start or replace an automatic deployment.
_Avoid_: rejected candidate, failed update

**policy re-evaluation**:
A new application of the current effective auto-update policy to a candidate after its readiness or relevant service state changes. It may produce a rule match, a delay decision, a skip, or an automatic deployment request.
_Avoid_: repeating the original check, retrying Compose

**candidate discovery time**:
The time at which Dockrev first accepts a candidate digest as a new observation for a service. Delay policies measure candidate age from this point, even when supporting version evidence becomes available later.
_Avoid_: inference completion time, deployment start time

**candidate reconciliation**:
A compensating evaluation that compares active candidate state with current registry evidence and policy state after a restart, missed event, or delayed worker result. It may advance, supersede, or close a candidate, but it does not replay obsolete history.
_Avoid_: full historical replay, repeated check

**automatic deployment claim**:
The exclusive acceptance of one eligible candidate for creation of an automatic update operation. A claim must be revalidated against the service's current candidate and effective policy before side effects begin.
_Avoid_: candidate discovery, Compose lock

**automatic action pending**:
The state of a ready candidate that has matched an effective policy but is waiting for a configured time or version-lag gate before deployment. It is distinct from a candidate awaiting version inference.
_Avoid_: unresolved candidate, queued update job

**settlement event**:
A notification that a candidate's authoritative settlement has been committed and is available for consumers. Consumers may react immediately, but the event is not the source of truth.
_Avoid_: check completion, notification delivery

**candidate notification identity**:
The stable service-and-candidate-digest identity used to deduplicate notifications for one discovered image candidate, regardless of whether its display version changes from raw tag to resolved version.
_Avoid_: notification message text, semantic version

**auto-update candidate**:
A service's newly observed candidate digest together with its raw tag, resolved version when available, current deployment baseline, and discovery provenance. It remains a candidate until an effective policy accepts or rejects it.
_Avoid_: update job, candidate container

**auto-update policy evaluation**:
The decision that applies the effective Stack or Service policy to an auto-update candidate. `semver` rules match resolved semantic versions; `regex` and `glob` rules may match the raw tag when no resolved version exists.
_Avoid_: version inference, update execution

**auto-update deployment**:
An accepted automatic update operation that changes the service to the candidate digest through the normal update path. It is a deployment action, not publication of an image to a registry.
_Avoid_: image publication, version discovery

## Notifications and PWA Badge

**notification event**:
A user-relevant operational occurrence that Dockrev exposes for operator attention, such as a completed update or rollback, an aggregated new-version discovery, or an aggregated GHCR webhook anomaly. It is distinct from delivery through any particular notification channel.
_Avoid_: channel delivery, push receipt, message count

**notification item**:
One user-facing aggregate representation of a notification event and the unit counted by the PWA badge. Multiple services or repositories included in the same event remain one notification item.
_Avoid_: service notification, repository notification, delivery record

**unread notification**:
A notification item that the operator has not explicitly acknowledged. Opening Dockrev, displaying a notification, or reading the same event in an external channel does not acknowledge it.
_Avoid_: pending event, undelivered notification

**notification acknowledgement**:
The explicit operator action that changes a notification item from unread to read, either by opening that item or by marking it read. Acknowledgement is idempotent and is shared across the operator's installed Dockrev clients.
_Avoid_: notification delivery, notification dismissal

**badge unread count**:
The number of unread notification items represented on an installed Dockrev PWA icon. It counts notification items rather than push attempts, delivery channels, services, or repositories.
_Avoid_: push count, message count, notification channel count

**notification item identity**:
The stable event-specific key used to ensure that retries and repeated observations of the same event do not create another notification item. Job completion uses the job identity; candidate discovery continues to use the candidate notification identity.
_Avoid_: delivery attempt, notification timestamp

**anomaly identity**:
The stable GHCR webhook anomaly represented by an owner, repository, and anomaly state. A changed error message alone does not create a new anomaly identity.
_Avoid_: audit run, error text

**notification history retention**:
The period for keeping acknowledged notification items available as history. Acknowledged items are retained for 90 days, while unread items remain until the operator acknowledges them.
_Avoid_: badge expiration, delivery timeout

**foreground notification synchronization**:
The authenticated page-owned read of the server notification count and inbox triggered at bootstrap, foreground resume, focus, online recovery, and every 60 seconds while visible. It repairs local Badge state; it is not a background wake mechanism.
_Avoid_: Service Worker timer, Push delivery, local delta

**background badge update**:
An update to the installed PWA icon while the page is closed. Dockrev can provide this through Web Push when subscribed, but cannot promise it when Push is disabled; Periodic Background Sync is only a future best-effort enhancement.
_Avoid_: foreground synchronization, server event creation, notification acknowledgement

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

## Application Shell Updates

**PWA shell update**:
A replacement of the browser application's Service Worker and its app-shell cache. It is ready only after the candidate Worker has populated its entire precache and reached `waiting`.
_Avoid_: service update, container update

**precache-eligible asset**:
A current app-shell build artifact whose public URL returns a successful HTTP response and can therefore be committed to the candidate Worker's precache.
_Avoid_: every generated file, not-found document

**not-found document**:
The generated `404.html` body rendered with HTTP `404` for an unknown Dockrev document URL. It is not part of the app shell and is not precache-eligible.
_Avoid_: static 404 asset, offline fallback

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

## Overview Refresh Semantics

- `initial snapshot load` is the first page-owned read before an overview has usable data. It may use an immediate skeleton because there is no prior snapshot to preserve.
- `manual refresh` is a user-requested authoritative overview read. It preserves the prior snapshot, shows local feedback after the 200ms user-action threshold, and may cover the requested region without disabling the existing list.
- `event-driven refresh` is a page-owned REST read caused by a management invalidation event. It updates the snapshot silently and never starts a loading mask; the management event is not itself a complete data push.
- `recovery synchronization` is the one-time page synchronization after a management transport session is connected or resumed. It is silent and targeted when the cursor replay is intact, and becomes a full reconciliation only after a replay gap, `resync_required`, or a protocol-invalid event.

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

## Mobile Detail Headers

**service detail mobile header**:
The single-row mobile header for one service. It presents the navigation trigger, the Dockrev mark, the current service name, and the service action menu; its constrained identity treatment is not a rule for Stack pages.
_Avoid_: Stack detail header, generic detail header

**Stack detail mobile header**:
The mobile header for one Stack. It presents the navigation trigger, the full Dockrev wordmark, and the Stack action menu. It has no service-name or monitoring-summary region to compete for horizontal space.
_Avoid_: service detail header, icon-only Stack header

The Stack detail mobile-header contract applies from 320px through 960px wide: it remains one row, keeps the full wordmark, and does not introduce horizontal overflow.

## Service Resource Monitoring

- `historical sampling gap` is an interval between consecutive durable REST history samples that exceeds the cadence inferred from the current history window. It describes persisted sampling continuity, not the absence of a live browser observation.
- `live observation` is a foreground 1-second resource SSE sample used to keep the chart and summary current. It does not establish that the corresponding historical sample was persisted or change the historical gap cadence.

## Navigation Model

- `page navigation` is the fixed, icon-led set of Dockrev's primary pages. It changes the operator's work area and is independent of the selected Stack, service, task, or settings section.
- `context navigation` is the page-owned directory beneath page navigation. It exposes only the groups, work queues, service hierarchy, cleanup scopes, or settings sections relevant to the current primary page.
- `navigation group` is a named collection of service entries on Navigation Overview. Selecting it locates the matching group within that page; it does not change the active primary page.
- `Overview header tools` are the desktop resource summary, browser-local current time, and service search in that order. When the usable header width is constrained, the resource summary remains while current time is absent and search is opened from its icon trigger.
- `browser-local current time` is a browser-clock display that updates every second and includes its GMT offset. It is independent from the server-provided recent scan time and resource sample times, and does not claim their freshness.
- `Overview service search` has one mounted input in each actual layout state: the wide header input, the constrained-header popover input, or the narrow-screen context-drawer input. It is not duplicated with CSS-hidden controls.
- `active task` is a queued or running task requiring current operational awareness. `recent completed task` is one of the newest terminal tasks retained for immediate review. Neither term implies that a task succeeded.
- `settings directory` is the context navigation for System Settings. On desktop it locates a section in the settings page; on mobile it opens the matching settings subpage. Both destinations represent the same settings section.
- `cleanup context` is the pair of independent page-owned filters that select a cleanup scope and one or more resource kinds. Scope communicates the operation's ownership and blast radius; resource kind communicates the candidate class.
- `mobile context drawer` is the temporary, page-owned presentation of context navigation on narrow screens. It complements the persistent primary bottom navigation and does not combine with it into a single undifferentiated menu.
- `service directory` is the searchable, multi-expand Stack and service tree for Operations Dashboard. It keeps its expanded branches and selected entry while the operator remains in that work area.
- `recent completed task` retains exactly five newest terminal tasks in context navigation and links to each task's detailed record. The complete task history remains in the primary content area.
