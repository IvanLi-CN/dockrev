import type { ServiceRollbackTargetResponse } from '../../../api'
import type { DockrevApiScenario } from './shared'
import { nowIso } from './shared'

export type RollbackTargetRaceState = {
  staleResponse: ServiceRollbackTargetResponse
  staleServed: boolean
}

type ApplyRollbackTargetRaceAfterUpdateArgs = {
  rollbackTargets: Record<string, ServiceRollbackTargetResponse>
  raceByServiceId: Map<string, RollbackTargetRaceState>
  scenario: DockrevApiScenario
  serviceId: string
  nextTag: string
  nextDigest: string
  nextResolvedTag: string
  previousDigest: string
  previousDisplayTag: string | null
}

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(() => resolve(), ms)
  })
}

export function applyRollbackTargetRaceAfterUpdate(args: ApplyRollbackTargetRaceAfterUpdateArgs) {
  const {
    rollbackTargets,
    raceByServiceId,
    scenario,
    serviceId,
    nextTag,
    nextDigest,
    nextResolvedTag,
    previousDigest,
    previousDisplayTag,
  } = args
  if (scenario !== 'service-detail-rollback-stale-after-update') return

  rollbackTargets[serviceId] = {
    available: true,
    currentDigest: nextDigest,
    currentDisplayTag: nextResolvedTag || nextTag || null,
    targetDigest: previousDigest || null,
    targetDisplayTag: previousDisplayTag,
    sourceUpdateJobId: 'job-update-rollback-race',
    sourceFinishedAt: nowIso(),
    unavailableReason: null,
    activeJobId: null,
    activeJobStatus: null,
  }
  raceByServiceId.set(serviceId, {
    staleResponse: {
      available: false,
      currentDigest: previousDigest,
      currentDisplayTag: previousDisplayTag,
      targetDigest: null,
      targetDisplayTag: null,
      sourceUpdateJobId: null,
      sourceFinishedAt: null,
      unavailableReason: 'no_matching_update_history',
      activeJobId: null,
      activeJobStatus: null,
    },
    staleServed: false,
  })
}

export async function maybeServeRollbackTargetRaceResponse(
  scenario: DockrevApiScenario,
  serviceId: string,
  raceByServiceId: Map<string, RollbackTargetRaceState>,
): Promise<ServiceRollbackTargetResponse | null> {
  const rollbackRace = raceByServiceId.get(serviceId)
  if (scenario === 'service-detail-rollback-stale-after-update' && rollbackRace && !rollbackRace.staleServed) {
    rollbackRace.staleServed = true
    await delay(700)
    return rollbackRace.staleResponse
  }
  if (scenario === 'service-detail-rollback-stale-after-update' && rollbackRace?.staleServed) {
    await delay(40)
  }
  return null
}
