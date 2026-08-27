import { describe, expect, test } from 'bun:test'
import { renderToStaticMarkup } from 'react-dom/server'

import type { ServiceBackupRecordItem } from '../src/api'
import { BackupRecordList } from '../src/components/ServiceBackupRecords'

describe('BackupRecordList', () => {
  test('renders legacy backup records without asset details', () => {
    const legacyRecord = {
      backupId: 'bkp_legacy',
      jobId: 'job_legacy',
      scope: 'service',
      status: 'skipped',
      createdAt: '2026-06-28T18:15:24.960797189Z',
      finishedAt: '2026-06-28T18:15:24.960797189Z',
    } as unknown as ServiceBackupRecordItem

    const html = renderToStaticMarkup(<BackupRecordList records={[legacyRecord]} />)

    expect(html).toContain('备份时间')
    expect(html).toContain('已跳过')
    expect(html).toContain('未记录资产明细')
    expect(html).toContain('serviceBackupRecordHeading')
  })

  test('renders auditable cleanup states and keeps execution errors separate', () => {
    const records: ServiceBackupRecordItem[] = [
      {
        backupId: 'bkp-delayed',
        jobId: 'job-delayed',
        scope: 'service',
        status: 'success',
        createdAt: '2026-07-28T08:00:00.000Z',
        cleanupAfter: '2026-08-01T08:00:00.000Z',
        lastCleanupAttemptAt: '2026-08-27T08:10:00.000Z',
        lastCleanupError: 'managed storage temporarily unavailable',
        error: null,
        assets: [],
      },
      {
        backupId: 'bkp-deleted',
        jobId: 'job-deleted',
        scope: 'stack',
        status: 'success',
        createdAt: '2026-07-20T08:00:00.000Z',
        cleanupAfter: '2026-07-21T08:00:00.000Z',
        deletedAt: '2026-08-27T08:11:00.000Z',
        error: null,
        assets: [],
      },
      {
        backupId: 'bkp-missing',
        jobId: 'job-missing',
        scope: 'service',
        status: 'success',
        createdAt: '2026-07-19T08:00:00.000Z',
        cleanupAfter: '2026-07-20T08:00:00.000Z',
        missingAt: '2026-08-27T08:12:00.000Z',
        error: null,
        assets: [],
      },
      {
        backupId: 'bkp-failed',
        jobId: 'job-failed',
        scope: 'service',
        status: 'failed',
        createdAt: '2026-07-18T08:00:00.000Z',
        error: 'zstd exited with status 1',
        assets: [],
      },
    ]

    const html = renderToStaticMarkup(<BackupRecordList records={records} />)

    expect(html).toContain('清理延迟')
    expect(html).toContain('managed storage temporarily unavailable')
    expect(html).toContain('data-slot="alert"')
    expect(html).toContain('<svg')
    expect(html).toContain('已删除')
    expect(html).toContain('删除时间')
    expect(html).toContain('文件已缺失（已核实）')
    expect(html).toContain('核实缺失时间')
    expect(html).toContain('备份执行错误：zstd exited with status 1')
    expect(html).not.toContain('清理延迟：zstd exited with status 1')
  })
})
