export type NotificationSyncRequest = (
  withItems: boolean,
  shouldBroadcast: boolean,
) => Promise<boolean>

export type NotificationSyncRetryWait = () => Promise<void>

export async function refreshNotificationAfterServerChange(
  sync: NotificationSyncRequest,
  invalidate: () => void,
  withItems: boolean,
  shouldBroadcast: boolean,
  waitForRetry: NotificationSyncRetryWait = () =>
    new Promise<void>((resolve) => window.setTimeout(resolve, 0)),
): Promise<boolean> {
  invalidate()
  for (let attempt = 0; attempt < 2; attempt += 1) {
    if (await sync(withItems, shouldBroadcast)) return true
    await waitForRetry()
  }
  return false
}
