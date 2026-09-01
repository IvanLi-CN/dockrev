# l2nm4 Release failure Telegram alerts implementation

## Coverage

- `.github/workflows/notify-release-failure.yml` keeps the `Release` failure
  filter, target SHA resolver, and manual smoke trigger.
- The failure and smoke jobs call Oidrune `notify.yml` at the pinned release
  commit `e48822f99c6402a753ed86557ea029754cbab20b`.
- The caller builds the complete notification summary, including repository,
  status, resolved target SHA, workflow run URL, and the existing context fields.
- Both Oidrune calls receive `id-token: write`; the old Shoutrrr secret and
  gateway override inputs are absent.

## Verification

- `.github/scripts/release-channel-contract-check.sh` parses the notification
  workflow and asserts trigger, fixed-reference, permission, summary, and
  secret/override invariants.
- Manual smoke verification is limited to static and contract checks. A real
  Telegram notification is outside this implementation.
