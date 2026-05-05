import type { NewVersionDiscoveryTimelineResponse, StackDetail } from '../../../api'
import { hashString, nowIso, offsetMockVersion, type DockrevMockApiOptions } from './shared'

export type MockDiscoveryTimelineServiceMatch = { svc: StackDetail['services'][number] } | null

export function buildMockDiscoveryTimeline(
  serviceId: string,
  options: DockrevMockApiOptions,
  findService: (serviceId: string) => MockDiscoveryTimelineServiceMatch,
): NewVersionDiscoveryTimelineResponse {
  if (options.discoveryTimelineErrorServiceIds?.includes(serviceId)) {
    throw new Error('mock discovery timeline failed')
  }

  const override = options.discoveryTimelineByServiceId?.[serviceId]
  if (override) {
    return {
      items: override.items.map((item) => ({ ...item })),
    }
  }

  const found = findService(serviceId)
  const count = Math.max(1, found?.svc.newVersionDiscoveryCount ?? ((hashString(serviceId) % 3) + 2))
  const runningVersion = found?.svc.image.resolvedTag?.trim() || found?.svc.image.tag?.trim() || '1.0.0'
  const candidateVersion =
    found?.svc.candidate?.resolvedTag?.trim() ||
    found?.svc.candidate?.tag?.trim() ||
    offsetMockVersion(runningVersion, 2, '1.0.2')

  const items: NewVersionDiscoveryTimelineResponse['items'] = [
    {
      kind: 'currentCandidate',
      version: candidateVersion,
      occurredAt: nowIso(-15 * 60 * 1000),
    },
  ]

  for (let index = 1; index < count; index += 1) {
    items.push({
      kind: 'historicalCandidate',
      version: offsetMockVersion(candidateVersion, -index, `1.0.${Math.max(0, count - index)}`),
      occurredAt: nowIso(-(15 + index * 37) * 60 * 1000),
    })
  }

  items.push({
    kind: 'currentRunning',
    version: runningVersion,
    occurredAt: found ? nowIso(-4 * 60 * 60 * 1000) : null,
  })

  return { items }
}
