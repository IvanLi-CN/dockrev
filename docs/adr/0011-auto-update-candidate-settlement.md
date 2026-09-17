# Automatic Update Candidate Settlement

## Status

Accepted for implementation.

## Context

Dockrev discovers a new image digest before an asynchronous version-inference
worker can resolve a semantic version for floating tags such as
<code>latest</code>. The automatic policy matcher currently evaluates the
check summary, sees a non-semver raw tag, and treats the candidate as not
matching a SemVer rule. The later inference completion updates snapshot or
display data, but does not reliably re-enter policy evaluation.

This makes three distinct facts look like one incomplete operation:

- the candidate digest has been discovered;
- the candidate's version evidence is still pending or unresolved;
- an automatic update job has not yet been queued or completed.

It also leaves recovery dependent on in-memory events and gives
<code>auto_update_pending</code> responsibilities that belong to a durable
candidate record. Finally, “publish” is ambiguous: Dockrev can deploy a
service to a candidate digest, but it does not publish an image or GitHub
Release as part of automatic update.

## Decision

### 1. Persist a canonical candidate lifecycle

Create an <code>auto_update_candidates</code> lifecycle record keyed by
<code>serviceId + candidateDigest</code>. It owns raw tag, digest-bound
evidence, resolved version, discovery provenance, discovery time, retry state,
settlement status and supersession.

The candidate settlement states are:

- <code>awaiting_inference</code>;
- <code>ready</code>;
- <code>unresolved</code>;
- <code>superseded</code>.

Candidate state and policy action state remain separate. The existing
<code>auto_update_pending</code> record continues to represent only a matched
delayed or enqueue action and references the candidate identity.

### 2. Make SemVer waiting explicit and fail closed

Strict-semver raw tags are immediately usable. Floating tags wait for
resolved-version evidence. SemVer rules never fall back to raw
<code>latest</code>, an unbound historical tag, or an unrelated repository tag.

Evidence is selected in this order:

1. strict-semver raw tag;
2. exact-digest registry tag snapshot;
3. exact-digest OCI
   <code>org.opencontainers.image.version</code> with the shared normalization
   rules;
4. explicit <code>unresolved</code> terminal settlement.

Regex and Glob rules may evaluate the raw tag while inference is pending or
unresolved. This preserves their tag-oriented behavior without weakening
SemVer safety.

### 3. Re-evaluate policy after settlement

The same evaluator runs after candidate discovery, inference settlement,
qualified schedule/GHCR webhook observations, policy changes, service recovery,
startup recovery and periodic reconciliation.

Automatic deployment remains source-gated: only schedule checks and GHCR
webhook service checks may create automatic update jobs. Manual checks,
previews and dry-runs only update discovery facts.

### 4. Use durable facts and compensating reconciliation

The system commits snapshot and candidate settlement before publishing a
settlement event. The event is an immediate wake-up signal, not the source of
truth. Startup and periodic reconciliation reload non-terminal candidates and
active actions so event loss and process restart do not lose deployment
eligibility.

The current scope remains single-instance. Database conditional transitions
and existing operation ownership protect claim idempotency; no distributed
lock is introduced.

### 5. Supersede old digests and anchor delay to discovery

When a newer digest is accepted for a service, the old candidate becomes
<code>superseded</code> and any active delayed action becomes
<code>skipped</code> with an explicit reason. Historical candidates remain
available for version-lag accounting, but only the latest valid candidate may
claim an update.

Delayed policy age starts at candidate discovery time. Inference latency,
check completion time and process restart do not reset the age.

### 6. Reuse the normal update path and keep publication separate

After a successful policy claim, Dockrev creates the existing explicit-target
update job and lets the normal updater, backup policy, digest lock,
cross-tag guard and operation settlement decide the service state. Inference
completion is never reported as deployment completion.

Image push, registry publication and GitHub Release publication are outside
this contract and must not be introduced as a side effect of candidate
settlement.

## Consequences

- A floating-tag candidate is visible and recoverable even while its version
  remains pending.
- A SemVer policy can continue automatically after inference completes without
  requiring another manual check.
- An inference terminal failure is diagnosable and safe: SemVer does not
  deploy an unknown version, while raw-tag policies remain usable.
- Duplicate events, restarts and new digests no longer depend on a single
  in-memory continuation.
- Existing pending rows need a migration/link to candidate identity, and the
  API/UI must expose two independent state dimensions.
- Automatic deployment remains a service update operation; it does not become
  an image or release publication pipeline.

## Rejected alternatives

- Treating <code>latest</code> as a SemVer match: unsafe and semantically
  incorrect.
- Waiting synchronously inside the check job: increases registry latency and
  violates the existing inference decoupling contract.
- Having the inference worker enqueue update jobs directly: bypasses current
  policy, source, claim and updater safety boundaries.
- Relying only on <code>task_finished</code>: loses state on restart or event
  delivery failure.
- Replaying every historical unresolved notification during rollout: can
  trigger deployments without a proven source or discovery time.

## Related Specs

- [Auto-update candidate settlement](../specs/auto-update-candidate-settlement/SPEC.md)
- [Version inference decoupling](../specs/kdapc-version-inference-decouple/SPEC.md)
- [Version inference convergence](../specs/wczjc-version-inference-convergence/SPEC.md)
- [New-version notification settlement](../specs/s4fqf-new-version-notify-event-settle-explicit-version/SPEC.md)
- [Auto-deploy policy configurator](../specs/xyy72-auto-deploy-policy-configurator/SPEC.md)
