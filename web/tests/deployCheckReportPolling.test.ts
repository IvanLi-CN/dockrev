import { describe, expect, test } from 'bun:test'

import type { DeployCheckReportEnvelope } from '../src/api'
import {
  hasBlockingDeployCheckFailure,
  shouldKeepDeployCheckLoading,
  shouldKeepPollingDeployCheckReport,
  shouldTriggerDeployCheckReportRefresh,
} from '../src/deployCheck'

describe('deploy-check report polling', () => {
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

  test('keeps polling while cached report is still marked refreshing', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'ready',
      refreshing: true,
      retryAfterMs: 450,
      report: {
        overall: {
          result: 'fail',
          blockingCheckIds: ['core.compose_access'],
          summary: 'cached report is visible while refresh continues',
        },
        generatedAt: '2026-06-26T14:22:00.000Z',
        checks: [],
      },
    }

    expect(shouldKeepPollingDeployCheckReport(envelope)).toBe(true)
  })

  test('stops polling once the report is ready and refreshing cleared', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'ready',
      refreshing: false,
      report: {
        overall: {
          result: 'pass',
          blockingCheckIds: [],
          summary: 'deploy checks settled',
        },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [],
      },
    }

    expect(shouldKeepPollingDeployCheckReport(envelope)).toBe(false)
  })

  test('triggers a background refresh when a cached report is ready but idle', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'ready',
      refreshing: false,
      report: {
        overall: {
          result: 'pass',
          blockingCheckIds: [],
          summary: 'cached report should kick off a background refresh on page open',
        },
        generatedAt: '2026-06-26T14:23:00.000Z',
        checks: [],
      },
    }

    expect(shouldTriggerDeployCheckReportRefresh(envelope)).toBe(true)
  })

  test('does not request a second refresh while a cached report is already refreshing', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'ready',
      refreshing: true,
      retryAfterMs: 450,
      report: {
        overall: {
          result: 'fail',
          blockingCheckIds: ['core.compose_access'],
          summary: 'cached report is visible while refresh continues',
        },
        generatedAt: '2026-06-26T14:22:00.000Z',
        checks: [],
      },
    }

    expect(shouldTriggerDeployCheckReportRefresh(envelope)).toBe(false)
  })

  test('keeps polling while the first refresh has not produced a report yet', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'pending',
      refreshing: true,
      retryAfterMs: 800,
      report: null,
    }

    expect(shouldKeepPollingDeployCheckReport(envelope)).toBe(true)
  })

  test('keeps the loading shell while no deploy-check report is available yet', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'pending',
      refreshing: true,
      retryAfterMs: 800,
      report: null,
    }

    expect(shouldKeepDeployCheckLoading(envelope)).toBe(true)
  })

  test('does not keep the loading shell once a cached report is already visible', () => {
    const envelope: DeployCheckReportEnvelope = {
      status: 'ready',
      refreshing: true,
      retryAfterMs: 450,
      report: {
        overall: {
          result: 'fail',
          blockingCheckIds: ['core.compose_access'],
          summary: 'cached report is visible while refresh continues',
        },
        generatedAt: '2026-06-26T14:22:00.000Z',
        checks: [],
      },
    }

    expect(shouldKeepDeployCheckLoading(envelope)).toBe(false)
  })
})
