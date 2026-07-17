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
    expect(result.hint).toContain('通过 supervisor 进入候选 0.62.0 对应的自我升级流程')
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

    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toContain('当前只能通过 supervisor 进入候选 0.62.0 对应的自我升级流程')
    expect(result.hint).toContain('当前只能通过 supervisor 进入候选 0.62.0 对应的自我升级流程')
  })

  test('surfaces the real offline blocker on non-candidate releases before suggesting supervisor', () => {
    const result = describeDockrevVersionCardAction({
      candidateMatch: false,
      candidateDisplayVersion: '0.62.0',
      action: makeAction({
        disabled: true,
        disabledReason:
          '自我升级不可用（supervisor offline） · 2026-07-17T03:54:13.408Z · HTTP 503: supervisor offline',
        status: 'offline',
      }),
    })

    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toContain('自我升级不可用（supervisor offline）')
    expect(result.hint).toContain('自我升级不可用（supervisor offline）')
    expect(result.hint).not.toContain('当前只能通过 supervisor 进入候选 0.62.0 对应的自我升级流程')
  })
})
