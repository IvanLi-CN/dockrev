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
  return scenario.startsWith('cleanup-console')
    ? 250
    : scenario === 'queue-progress-smoothing' || scenario === 'queue-long-logs' || scenario === 'service-detail-rollback-stale-after-update'
      ? 700
      : 4_000
}

export function normalizeDigestValue(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return ''
  return trimmed.includes(':') ? trimmed : `sha256:${trimmed}`
}

export function buildMockDigestTagData(
  scenario: DockrevApiScenario,
  serviceId: string,
  imageTag: string,
  digestNorm: string,
  refreshed: boolean,
): { repoTags: string[]; tags: string[] } {
  const isVersionTagsDemoScenario =
    scenario === 'version-tags-popover-demo' ||
    scenario === 'version-tags-popover-same-digest' ||
    scenario === 'version-tags-popover-snapshot-pending'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const repoTags =
    serviceId === 'svc-prod-api'
      ? ['5.2.1', '5.2.3', '5.2.4', '5.3.0', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
      : serviceId === 'svc-prod-web'
        ? (() => {
            const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'stable', 'latest']
            for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
            return out
          })()
        : serviceId === 'svc-resolved-web'
          ? (() => {
              const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
              for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
              return out
            })()
          : isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
            ? ['v0.8.9-arm64', 'v0.8.8-arm64', 'v0.8.8', 'v0.8.7', '0.8.8', '0.8.7', 'stable', 'latest']
            : digestNorm === `sha256:${'a'.repeat(64)}`
              ? ['v0.1.8', '0.1.8']
              : [imageTag]

  const tags = !digestNorm
    ? []
    : serviceId === 'svc-version-tags' && isVersionTagsDemoScenario && digestNorm === d('a', 'b1')
      ? ['v0.8.7', '0.8.7', 'stable', 'latest']
      : serviceId === 'svc-version-tags' && isVersionTagsDemoScenario && digestNorm === d('b', '9f')
        ? refreshed
          ? ['v0.8.8', 'v0.8.8-arm64', '0.8.8', 'stable', 'latest']
          : ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
        : digestNorm === d('c', 'c2')
          ? ['v5.2.1', '5.2.1', '5.2', 'stable', 'latest']
          : digestNorm === d('a', 'b1') && serviceId === 'svc-resolved-web'
            ? ['5.2.1', 'v5.2.1', 'stable', 'latest']
            : digestNorm === d('b', '9f') && serviceId === 'svc-resolved-web'
              ? ['5.2.3', 'v5.2.3']
              : digestNorm === d('a', 'b1')
                ? ['5.2.1', 'v5.2.1']
                : digestNorm === d('b', '9f') && serviceId === 'svc-prod-api'
                  ? ['5.2.3', 'v5.2.3', 'stable', 'latest']
                  : digestNorm === `sha256:${'a'.repeat(64)}`
                    ? ['v0.1.8', '0.1.8']
                    : [imageTag]

  return { repoTags, tags }
}
