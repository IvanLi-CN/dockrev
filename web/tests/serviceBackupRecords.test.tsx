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
  })
})
