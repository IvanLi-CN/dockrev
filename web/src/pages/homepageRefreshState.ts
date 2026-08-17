export function isCurrentHomepageRefresh(
  requestId: number,
  latestRequestId: number,
): boolean {
  return requestId === latestRequestId;
}
