import { describe, expect, test } from 'bun:test'
import { formatJobReadableDisplay, formatJobTypeLabel } from '../src/jobDisplay'

describe('service lifecycle job labels', () => {
  test.each([
    ['start', '启动任务'],
    ['stop', '停止任务'],
    ['restart', '重启任务'],
  ] as const)('uses the %s action', (action, expected) => {
    expect(formatJobTypeLabel('service_lifecycle', { action })).toBe(expected)
    expect(formatJobReadableDisplay('service_lifecycle', 'service', { action }).primaryLabel).toBe(expected)
  })

  test('keeps a fallback for legacy summaries', () => {
    expect(formatJobTypeLabel('service_lifecycle', {})).toBe('服务生命周期')
  })
})
