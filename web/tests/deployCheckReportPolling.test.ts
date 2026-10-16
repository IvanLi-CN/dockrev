import { describe, expect, test } from 'bun:test'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import {
  deployCheckOperationImpactReason,
  hasActiveCoreFailure,
  hasBlockingDeployCheckFailure,
  hasCurrentStartupCoreFailure,
} from '../src/deployCheck'

const healthyCoreCheck = {
  id: 'core.update_executor_ready',
  title: '更新执行器可用',
  group: 'core' as const,
  required: true,
  status: 'pass' as const,
  summary: 'executor available',
  impact: 'writes allowed',
  evidence: 'Compose V2',
  recommendation: '',
}

describe('server-owned deploy-check entry', () => {
  test('uses the current startup projection for automatic diagnostic display', () => {
    const report = {
      overall: { result: 'fail' as const, blockingCheckIds: ['core.update_executor_ready'], summary: 'core failure' },
      generatedAt: '2026-10-16T00:00:00.000Z',
      checks: [healthyCoreCheck],
      serverBootId: 'boot-1',
      reportBootId: 'boot-1',
      startupStatus: 'fail' as const,
      startupFailureKey: 'boot-1:core.update_executor_ready',
      operationImpacts: [{
        checkId: 'core.update_executor_ready',
        scope: 'global',
        objectIds: [],
        operations: ['update'],
        reason: 'writes blocked',
      }],
    }

    expect(hasBlockingDeployCheckFailure(report)).toBe(true)
    expect(hasCurrentStartupCoreFailure(report)).toBe(true)
    expect(hasActiveCoreFailure(report)).toBe(true)
  })

  test('does not turn optional feature failures or unknown reports into a site gate', () => {
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'fail', blockingCheckIds: [], summary: 'optional failure' },
        generatedAt: '2026-10-16T00:00:00.000Z',
        checks: [healthyCoreCheck, {
          id: 'feature.notifications.telegram',
          title: '通知能力：Telegram',
          group: 'feature',
          required: false,
          status: 'fail',
          summary: 'missing token',
          impact: 'telegram unavailable',
          evidence: 'missing botToken',
          recommendation: 'configure Telegram',
        }],
      }),
    ).toBe(false)
    expect(
      hasBlockingDeployCheckFailure({
        overall: { result: 'fail', blockingCheckIds: [], summary: 'unknown' },
        generatedAt: '2026-10-16T00:00:00.000Z',
        checks: [],
      }),
    ).toBe(false)
  })

  test('settled background failures stay visible without startup auto-open privilege', () => {
    const report = {
      overall: { result: 'fail' as const, blockingCheckIds: ['core.docker_engine'], summary: 'docker unavailable' },
      generatedAt: '2026-10-16T00:00:00.000Z',
      checks: [healthyCoreCheck],
      serverBootId: 'boot-1',
      reportBootId: 'boot-1',
      startupStatus: 'settled' as const,
      startupFailureKey: null,
      operationImpacts: [{
        checkId: 'core.docker_engine',
        scope: 'global',
        objectIds: [],
        operations: ['update', 'cleanup'],
        reason: 'Docker unavailable',
      }],
    }
    expect(hasBlockingDeployCheckFailure(report)).toBe(true)
    expect(hasCurrentStartupCoreFailure(report)).toBe(false)
    expect(hasActiveCoreFailure(report)).toBe(true)
  })

  test('entry reads never trigger a browser refresh or full-page gate', () => {
    const appSource = readFileSync(resolve(import.meta.dir, '..', 'src/App.tsx'), 'utf8')
    expect(appSource).toContain('loadDeployCheckReport()')
    expect(appSource).toContain('claimDeployCheckAutoOpen')
    expect(appSource).not.toContain('refreshDeployCheckReport()')
    expect(appSource).not.toContain('deployGateLoading')
    expect(appSource).not.toContain('setDeployCheckGate')
  })

  test('diagnostic route always keeps continue browsing available', () => {
    const source = readFileSync(resolve(import.meta.dir, '..', 'src/pages/DeployWelcomePage.tsx'), 'utf8')
    expect(source).not.toContain('Boolean(reportRefreshError)')
    expect(source).toContain("{saving ? '打开中…' : '继续浏览'}")
    expect(source).toContain('disabled={saving}')
    expect(source).not.toContain('disabled={saving || hasBlockingFailures}')
  })

  test('projects operation impacts onto matching targets without disabling unrelated objects', () => {
    const report = {
      overall: { result: 'fail' as const, blockingCheckIds: ['core.compose_access'], summary: 'compose unavailable' },
      generatedAt: '2026-10-16T00:00:00.000Z',
      checks: [],
      operationImpacts: [
        {
          checkId: 'core.compose_access',
          scope: 'affected_projects',
          objectIds: ['stack-a'],
          operations: ['update', 'restart'],
          reason: 'Compose 配置不可访问',
        },
        {
          checkId: 'core.docker_engine',
          scope: 'global',
          objectIds: [],
          operations: ['cleanup'],
          reason: 'Docker 引擎不可用',
        },
        {
          checkId: 'core.service_image_ref_valid',
          scope: 'affected_services',
          objectIds: ['service-a'],
          operations: ['update'],
          reason: '服务镜像引用无效',
        },
      ],
    }

    expect(deployCheckOperationImpactReason(report, 'update', 'stack-a')).toBe('Compose 配置不可访问')
    expect(deployCheckOperationImpactReason(report, 'update', 'stack-b')).toBeNull()
    expect(deployCheckOperationImpactReason(report, 'restart', ['service-a', 'stack-a'])).toBe('Compose 配置不可访问')
    expect(deployCheckOperationImpactReason(report, 'update', ['service-a', 'stack-a'])).toBe('Compose 配置不可访问')
    expect(deployCheckOperationImpactReason(report, 'update', ['service-a', 'stack-b'])).toBe('服务镜像引用无效')
    expect(deployCheckOperationImpactReason(report, 'cleanup')).toBe('Docker 引擎不可用')
  })
})
