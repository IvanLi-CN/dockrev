import type { CleanupScanRequest, CleanupScanResponse } from '../../../api'
import type { DockrevApiScenario, Fixture } from './shared'
import { parseJsonBody } from './shared'

export function cloneFixture(fixture: Fixture): Fixture {
  if (typeof structuredClone === 'function') {
    return structuredClone(fixture)
  }
  return JSON.parse(JSON.stringify(fixture)) as Fixture
}

function nextNumericSuffix(value: string): number {
  const match = value.match(/(\d+)(?!.*\d)/)
  if (!match) return 0
  const parsed = Number.parseInt(match[1] ?? '0', 10)
  return Number.isFinite(parsed) ? parsed : 0
}

export function seedIgnoreSequence(fixture: Fixture | null): number {
  if (!fixture) return 0
  return fixture.ignores.reduce((max, rule) => Math.max(max, nextNumericSuffix(rule.id)), 0)
}

export function seedJobSequence(fixture: Fixture | null): number {
  if (!fixture) return 0
  return fixture.jobs.reduce((max, job) => Math.max(max, nextNumericSuffix(job.id)), 0)
}

export function parseCleanupScanRequest(body: unknown): CleanupScanRequest {
  const parsed = parseJsonBody(body) as CleanupScanRequest | null
  return {
    reason: parsed?.reason === 'confirm' ? 'confirm' : 'page',
    refresh: parsed?.refresh !== false,
    preset:
      parsed?.preset === 'conservative' ||
      parsed?.preset === 'balanced' ||
      parsed?.preset === 'project_deep_clean' ||
      parsed?.preset === 'aggressive'
        ? parsed.preset
        : 'balanced',
    scope: parsed?.scope === 'stack' || parsed?.scope === 'service' ? parsed.scope : 'all',
    stackId: typeof parsed?.stackId === 'string' ? parsed.stackId : undefined,
    serviceId: typeof parsed?.serviceId === 'string' ? parsed.serviceId : undefined,
  }
}

export function partialCleanupResponse(response: CleanupScanResponse): CleanupScanResponse {
  const firstStack = response.stackGroups[0]
  const partialStacks = firstStack
    ? [
        {
          ...firstStack,
          services: firstStack.services.slice(0, 1),
          stackOrphans: firstStack.stackOrphans.slice(0, 1),
        },
      ]
    : []
  return {
    ...response,
    status: 'pending',
    refreshing: true,
    retryAfterMs: 450,
    serverDiskUsage: null,
    stackGroups: partialStacks,
    unownedGroup: null,
    confirmationFingerprint: null,
  }
}

export function resolveMockEventSourcePollInterval(scenario: DockrevApiScenario): number {
  return scenario === 'queue-progress-smoothing' || scenario === 'queue-long-logs' || scenario === 'service-detail-rollback-stale-after-update' ? 700 : 4_000
}
