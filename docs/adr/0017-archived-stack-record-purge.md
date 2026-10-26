# Missing Stack records and independent archival

Status: accepted

## Context

Dockrev currently uses the archived flag for two different situations:

- A user hides a Stack because it is no longer part of their active view.
- Discovery can no longer find the Stack's saved Compose files.

Those situations have different meanings and different recovery paths. A user archive is reversible and must not imply that Docker resources are stopped or absent. A missing Compose source is a discovery fact and may require permanent removal of stale Dockrev records. Neither state is a safe reason to delete Docker resources implicitly.

## Decision

### Independent Stack states

Dockrev will model user archival and discovery state independently.

- `user_archived` is a reversible visibility preference. Discovery never changes it.
- `missing` is persisted when all saved Compose paths are confirmed missing with `ENOENT`. `stopped` and `invalid` are not eligible for record deletion.
- A Stack may be both missing and user-archived. It belongs in the missing-record view, with its user-archived status shown, and must not be duplicated in the normal archived view.
- When the Compose files reappear before deletion, the next scan clears `missing` and preserves the original Stack ID and user-archive preference. After permanent deletion, a later rediscovery creates a new Stack ID.

The existing `auto_archive_compose_files_missing` meaning will be migrated to `missing`. User-archived records remain user-archived; if their files are also confirmed missing, they are additionally eligible for the missing-record view.

### Permanent deletion of a missing record

The UI will provide an explicit per-Stack action named “permanently delete missing record”. There is no bulk-delete action.

The server will revalidate that the Stack is still `missing` and is not owned by discovery, update, restore, or another conflicting operation. Each Stack is committed independently. A conflict, unsafe backup path, or other failure leaves that Stack's Dockrev data unchanged and can be retried later.

Successful deletion removes the Stack's Dockrev metadata and dependent service, history, task, backup-record, and backup-file data. A referenced backup file that is already absent counts as cleared and is recorded in the audit result. Backup paths must be validated before deletion and must remain within the managed Dockrev backup area.

The operation never deletes host Compose files and never stops or removes Docker containers, images, volumes, networks, or builder data. If Docker resources still exist, the confirmation explains that they will remain and become unmanaged by Dockrev after the record is deleted.

The operation is idempotent by Stack ID. A repeated request after successful deletion reports that the record no longer exists and does not repeat unrelated work or create a duplicate success event. Each deletion attempt that reaches the operation handler leaves a minimal audit event containing the target, actor, time, and result, without retaining the deleted Stack's business data or backup contents.

### Docker resource reclamation

Docker resource reclamation remains a separate, auditable operation governed by Docker's unused-resource rules. Its ownership inventory includes every retained Stack, regardless of `user_archived` or `missing`, so visibility state cannot change resource ownership or reclamation eligibility. Docker runtime checks remain authoritative for in-use protection.

After a Stack record is permanently deleted, any remaining Docker resources are no longer associated with that Stack in Dockrev and are handled as unowned resources by the existing reclamation rules. The default reclamation scope remains stopped, provably Stack-owned containers and private networks; volumes, images, bind mounts, and external or shared networks require separate explicit confirmation.

## Consequences

- The UI no longer implies that “archived” means “Docker resources exist but are stopped”.
- Missing records have a clear, non-batch cleanup path without coupling it to Docker resource reclamation.
- The current Docker cleanup safety checks remain applicable, but its managed ownership context must include retained archived and missing Stacks.
- Existing auto-archived cards require a data migration and no longer appear as ordinary archived Stacks.
- A permanent record deletion is intentionally irreversible from Dockrev's perspective; rediscovery after deletion creates a new identity.
