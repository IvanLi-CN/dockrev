import { describe, expect, test } from 'bun:test'

import {
  formatJobReadableDisplay,
  formatJobMachineName,
  formatJobReadableName,
  formatJobScopeLabel,
  formatJobTypeLabel,
} from '../src/jobDisplay'

describe('jobDisplay', () => {
  test('maps known job type and scope to readable Chinese labels', () => {
    expect(formatJobTypeLabel('update')).toBe('更新任务')
    expect(formatJobTypeLabel('runtime_scan')).toBe('运行时扫描')
    expect(formatJobTypeLabel('check')).toBe('检查任务')
    expect(formatJobScopeLabel('all')).toBe('全局')
    expect(formatJobScopeLabel('stack')).toBe('Stack')
    expect(formatJobScopeLabel('service')).toBe('服务')
  })

  test('builds readable name with mapped labels', () => {
    expect(formatJobReadableName('update', 'service')).toBe('更新任务 · 服务')
    expect(formatJobReadableName('runtime_scan', 'all')).toBe('运行时扫描 · 全局')
  })

  test('splits readable name into title and optional scope tag', () => {
    expect(formatJobReadableDisplay('update', 'service')).toEqual({
      primaryLabel: '更新任务',
      scopeTag: '服务',
      typeTone: 'update',
    })
    expect(formatJobReadableDisplay('runtime_scan', 'stack')).toEqual({
      primaryLabel: '运行时扫描',
      scopeTag: 'Stack',
      typeTone: 'runtimeScan',
    })
    expect(formatJobReadableDisplay('update', ' ')).toEqual({ primaryLabel: '更新任务', scopeTag: null, typeTone: 'update' })
    expect(formatJobReadableDisplay('unknown_type', 'all')).toEqual({ primaryLabel: 'unknown_type', scopeTag: '全局', typeTone: 'default' })
    expect(formatJobReadableDisplay(' ', 'all')).toEqual({ primaryLabel: '全局', scopeTag: null, typeTone: 'default' })
    expect(formatJobReadableDisplay(' ', ' ')).toEqual({ primaryLabel: '-', scopeTag: null, typeTone: 'default' })
  })

  test('falls back to raw value for unknown labels and to dash for empty values', () => {
    expect(formatJobTypeLabel('custom_job')).toBe('custom_job')
    expect(formatJobScopeLabel('tenant')).toBe('tenant')
    expect(formatJobReadableName(' custom_job ', ' tenant ')).toBe('custom_job · tenant')
    expect(formatJobReadableName(' ', ' ')).toBe('-')
  })

  test('keeps machine name for troubleshooting', () => {
    expect(formatJobMachineName('update', 'service')).toBe('update.service')
    expect(formatJobMachineName('custom_job', 'tenant')).toBe('custom_job.tenant')
    expect(formatJobMachineName(' ', 'service')).toBe('service')
    expect(formatJobMachineName('', '')).toBe('-')
  })
})
