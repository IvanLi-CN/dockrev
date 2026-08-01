export const SERVICE_TREE_REFRESH_EVENT = 'dockrev:service-tree-refresh'

export type ServiceTreeRefreshDetail = {
  stackId: string
  serviceId?: string | null
  reason?: string
}

export function publishServiceTreeRefresh(detail: ServiceTreeRefreshDetail): void {
  if (typeof window === 'undefined') return
  const stackId = detail.stackId.trim()
  if (!stackId) return
  window.dispatchEvent(
    new CustomEvent<ServiceTreeRefreshDetail>(SERVICE_TREE_REFRESH_EVENT, {
      detail: { ...detail, stackId },
    }),
  )
}
