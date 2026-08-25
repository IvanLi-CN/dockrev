import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import {
  hasBlockingDeployCheckFailure,
} from '../src/deployCheck'

describe('deploy-check cached gate', () => {
  test('treats any required core failure as a hard gate', () => {
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'pass', blockingCheckIds: [], summary: 'inconsistent fixture' },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [
          {
            id: 'core.update_executor_ready',
            title: '更新执行器可用',
            group: 'core',
            required: true,
            status: 'fail',
            summary: 'compose_v2_required',
            impact: 'writes blocked',
            evidence: 'Compose V1',
            recommendation: 'install Compose V2+',
          },
        ],
      }),
    ).toBe(true)
  })

  test('treats a required core check that is not PASS as a hard gate', () => {
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'pass', blockingCheckIds: [], summary: 'inconsistent fixture' },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [
          {
            id: 'core.update_executor_ready',
            title: '更新执行器可用',
            group: 'core',
            required: true,
            status: 'na',
            summary: 'executor status unavailable',
            impact: 'writes blocked',
            evidence: 'no report evidence',
            recommendation: 'install Compose V2+',
          },
        ],
      }),
    ).toBe(true)
  })

  test('fails closed when required core evidence is missing or non-canonical', () => {
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'pass', blockingCheckIds: [], summary: 'missing checks' },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [],
      }),
    ).toBe(true)
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'pass', blockingCheckIds: [], summary: 'legacy group' },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [
          {
            id: 'core.update_executor_ready',
            title: '更新执行器可用',
            group: 'legacy',
            required: true,
            status: 'pass',
            summary: 'executor available',
            impact: 'writes blocked',
            evidence: 'Compose V2',
            recommendation: '',
          },
        ],
      }),
    ).toBe(false)
  })

  test('permits a cached passing report while it is refreshing', () => {
    expect(
      hasBlockingDeployCheckFailure({
        overall: {
          result: 'pass',
          blockingCheckIds: [],
          summary: 'cached pass remains visible during background refresh',
        },
        generatedAt: '2026-06-26T14:22:00.000Z',
        checks: [
          {
            id: 'core.update_executor_ready',
            title: '更新执行器可用',
            group: 'core',
            required: true,
            status: 'pass',
            summary: 'executor available',
            impact: 'writes allowed',
            evidence: 'Compose V2',
            recommendation: '',
          },
        ],
      }),
    ).toBe(false)
  })

  test('does not turn a cached pass into a blocking failure when its refresh errors', () => {
    const source = readFileSync(
      resolve(import.meta.dir, '..', 'src/pages/DeployWelcomePage.tsx'),
      'utf8',
    )

    expect(source).toMatch(
      /const hasBlockingFailures = report\s*\? hasBlockingDeployCheckFailure\(report\)\s*:\s*Boolean\(reportRefreshError\)/,
    )
  })

  test('revalidates the cached gate when the tab returns to the foreground', () => {
    const source = readFileSync(resolve(import.meta.dir, '..', 'src/App.tsx'), 'utf8')

    expect(source).not.toContain('document.addEventListener("visibilitychange", refreshOnForeground)')
    expect(source).toContain('useManagementEventBatch(({ events, resyncRequired }) => {')
    expect(source).toContain('if (\n      resyncRequired ||')
    expect(source).toContain('void refreshDeployCheckGate(resyncRequired).catch(() => {})')
    expect(source).toContain('deployCheckBackgroundRefreshInFlightRef.current = true')
    expect(source).toContain('deployCheckBackgroundRefreshInFlightRef.current = false')
    expect(source).not.toContain('deployCheckBackgroundRefreshRequestedRef')
  })
})
