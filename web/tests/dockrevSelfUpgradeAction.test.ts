import { describe, expect, test } from 'bun:test'

import {
  describeDockrevVersionCardAction,
  type DockrevSelfUpgradeActionDescriptor,
} from '../src/pages/serviceDetailUtils'

function makeAction(
  overrides: Partial<DockrevSelfUpgradeActionDescriptor> = {},
): DockrevSelfUpgradeActionDescriptor {
  return {
    label: '升级 Dockrev',
    disabled: false,
    disabledReason: null,
    status: 'ok',
    retryVisible: false,
    retryDisabled: false,
    open: () => undefined,
    retry: () => undefined,
    ...overrides,
  }
}

describe('describeDockrevVersionCardAction', () => {
  test('keeps the candidate card clickable when self-upgrade is available', () => {
    const result = describeDockrevVersionCardAction({
      candidateMatch: true,
      candidateDisplayVersion: '0.62.0',
      action: makeAction(),
    })

    expect(result.label).toBe('升级 Dockrev')
    expect(result.disabled).toBe(false)
    expect(result.disabledReason).toBeNull()
    expect(result.hint).toContain('当前候选 0.62.0 已就绪')
    expect(result.buttonVariant).toBe('primary')
    expect(result.presentation).toBe('candidate')
  })

  test('falls back to the shared busy reason when the descriptor is disabled without a hint', () => {
    const result = describeDockrevVersionCardAction({
      candidateMatch: true,
      candidateDisplayVersion: '0.62.0',
      action: makeAction({
        disabled: true,
        disabledReason: null,
      }),
    })

    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toBe('服务动作处理中，请稍后再试。')
    expect(result.hint).toBe('服务动作处理中，请稍后再试。')
  })

  test('keeps newer non-candidate releases disabled with the candidate-only supervisor explanation', () => {
    const result = describeDockrevVersionCardAction({
      candidateMatch: false,
      candidateDisplayVersion: '0.62.0',
      action: makeAction(),
    })

    expect(result.label).toBe('仅候选可升级')
    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toContain('这个版本不是当前候选；当前只能升级候选 0.62.0。')
    expect(result.hint).toContain('这个版本不是当前候选；当前只能升级候选 0.62.0。')
    expect(result.buttonVariant).toBe('ghost')
    expect(result.presentation).toBe('candidateOnly')
  })

  test('surfaces the real offline blocker on non-candidate releases before suggesting supervisor', () => {
    const result = describeDockrevVersionCardAction({
      candidateMatch: false,
      candidateDisplayVersion: '0.62.0',
      action: makeAction({
        disabled: true,
        disabledReason: '自我升级不可用（supervisor offline） · 2026-07-17T03:54:13.408Z · HTTP 503: supervisor offline',
        status: 'offline',
      }),
    })

    expect(result.label).toBe('仅候选可升级')
    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toContain('自我升级不可用（supervisor offline）')
    expect(result.hint).toContain('自我升级不可用（supervisor offline）')
    expect(result.hint).toContain('当前候选为 0.62.0，此版本不能直接升级。')
    expect(result.hint).not.toContain('通过 supervisor')
    expect(result.buttonVariant).toBe('ghost')
    expect(result.presentation).toBe('candidateOnly')
  })
})
