# Fence Accepted Service State With Generations

Dockrev protects each Service's accepted deployment state with a durable generation compare-and-swap protocol. A stable Service has an even generation. A mutating operation atomically acquires ownership by moving the generation to the next odd value and recording the opened generation plus a complete baseline snapshot in `job_service_targets`. Terminal settlement writes the accepted result, advances the generation to the next even value, and finishes the owning Job in one immediate transaction.

Read-only observers carry the generation they read through Docker, registry, and Compose I/O. Their persistence transaction succeeds only when that generation is still current and even, and a successful observation advances it by two. Discovery applies the same rule to the complete existing Service set of a Stack and either commits the whole Stack snapshot or defers it.

## Considered Options

- Extend the process-global managed-override lock: rejected because unrelated Stacks would block one another, observation I/O would lengthen the critical section, and the lock cannot survive process restart.
- Reject writes only while a matching Job is active: rejected because an observation started before the mutation can finish after the Job becomes terminal and overwrite the settled state.
- Add terminal settlement without fencing observers: rejected because a later stale observer can still overwrite that settlement.
- Use generation compare-and-swap with durable Job ownership: selected because it rejects observations from before, during, and after a mutation without serializing unrelated Services.

## Consequences

- `services` stores an `accepted_state_generation`; `job_service_targets` stores the owning generation and a versioned baseline accepted-state snapshot.
- Update, rollback, lifecycle, managed-override reconcile, and backup paths that can stop, restart, or replace a Service must use the same ownership and settlement interface. Dry runs never acquire ownership.
- Check and runtime-scan writes return an explicit applied or stale/deferred result. A successful accepted observation increments the generation by two so another result from the same read generation cannot overwrite it.
- Discovery must validate Stack membership and every existing Service generation in the same transaction as Compose and Service synchronization. It cannot partially synchronize a Stack.
- Registry failure does not erase a previously accepted candidate. Settlement preserves durable baseline knowledge and marks candidate refresh as deferred.
- Startup recovery restores operation side effects and settles odd generations before publishing terminal Job state. An unresolved odd generation remains fenced rather than accepting unverified observations.
- The existing managed-override lock remains responsible only for Compose and override filesystem side effects; it is not the Service snapshot consistency boundary.
