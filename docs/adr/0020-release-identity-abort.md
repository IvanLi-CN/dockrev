# Unmerged release identity reservations can be explicitly aborted

## Status

Superseded by ADR 0022; version reservations no longer exist.

## Context

A version reservation is durable by design: repeating a preparation reuses the
same identity, and a foreign identity on the same version fails closed. Nothing
removed a reservation whose identity never reached `main`. If such a version was
later published by another identity, `Release` would refuse to publish because
the reservation still pointed at the abandoned commit, and only a manual ref
deletion could unblock it.

## Consequences

- `Release abort` takes a `version` and a `reason` and runs only from `main`.
- It refuses when the identity is merged, when either tag form exists, when a
  GitHub Release exists, when a publication lock exists, or when the reservation
  points at another identity. Refusals report the reason and change nothing.
- Otherwise it closes the identity PR and deletes the preparation branch and the
  version reservation, freeing the version for a new decision.
- Aborting never deletes a tag, GitHub Release, or image. Published identities
  are out of scope by construction, so the operation cannot be used as a
  rollback.
- There is no time-based garbage collection: an abandoned identity stays
  reserved until an owner aborts it.
