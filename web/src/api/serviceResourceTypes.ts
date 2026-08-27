export type ServiceResourceUsageWindow = '3m' | '1h' | '24h' | '7d' | '30d'

export type ServiceResourceSample = {
  sampledAt: string
  cpuPercent: number
  memUsedBytes?: number
  memLimitBytes?: number
  netRxBytes?: number
  netTxBytes?: number
  netRxRateBps?: number
  netTxRateBps?: number
  blockReadBytes?: number
  blockWriteBytes?: number
  blockReadRateBps?: number
  blockWriteRateBps?: number
  pids?: number
  containerCount: number
}

export type ServiceResourceSnapshot = {
  fetchedAt?: string | null
  windowKey: ServiceResourceUsageWindow
  samples: ServiceResourceSample[]
  monitorDisabled?: boolean
}

export type ServiceResourceHistoryResponse = {
  serviceId: string
  window: ServiceResourceUsageWindow | string
  samples: ServiceResourceSample[]
  resolutionSeconds?: number
  peaks?: ServiceResourcePeak[]
  lifecycle?: ServiceLifecycleProjection
}

export type ServiceLifecycleEvent = {
  id: number
  serviceId: string
  stackId?: string | null
  operationGroupId: string
  jobId?: string | null
  origin: string
  transition: string
  observedAt: string
  boundaryPrecision: string
  evidence: Record<string, unknown>
  details: Record<string, unknown>
  createdAt: string
}

export type LifecycleAvailabilityInterval = {
  operationGroupId: string
  startedAt: string
  stoppedAt: string
  startEventId: number
  stopEventId: number
  complete: boolean
}

export type ServiceLifecycleProjection = {
  events: ServiceLifecycleEvent[]
  availabilityIntervals: LifecycleAvailabilityInterval[]
  nextCursor?: number | null
  lastEventId?: number | null
  retentionSince: string
}

export type ServiceLifecycleSnapshotResponse = ServiceLifecycleProjection & {
  serviceId: string
  since: string
  until: string
}

export type ServiceResourcePeak = {
  sampledAt: string
  cpuPercent: number
  memUsedBytes?: number
  memLimitBytes?: number
  pids?: number
  containerCount: number
  netRxRateBps?: number
  netTxRateBps?: number
  blockReadRateBps?: number
  blockWriteRateBps?: number
}

export type ServiceResourceOverviewItem = {
  serviceId: string
  sampledAt?: string | null
  cpuPercent?: number | null
  memUsedBytes?: number | null
  memLimitBytes?: number | null
  netRxRateBps?: number | null
  netTxRateBps?: number | null
  stale: boolean
  sampleCount: number
}

export type ServiceResourceOverviewResponse = {
  enabled: boolean
  window: ServiceResourceUsageWindow | string
  generatedAt: string
  staleAfterSeconds: number
  services: ServiceResourceOverviewItem[]
}
