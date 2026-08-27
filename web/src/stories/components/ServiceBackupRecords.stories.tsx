import type { Meta, StoryObj } from '@storybook/react'
import { BackupRecordList } from '../../components/ServiceBackupRecords'
import type { ServiceBackupRecordItem } from '../../api'

const records: ServiceBackupRecordItem[] = [
  {
    backupId: 'bkp-cleanup-delayed',
    jobId: 'job-cleanup-delayed',
    scope: 'service',
    status: 'success',
    createdAt: '2026-07-28T08:00:00.000Z',
    finishedAt: '2026-07-28T08:00:04.000Z',
    artifactPath: '/srv/dockrev/backups/stack-prod/20260728-080000.tar.zst',
    sizeBytes: 12_000_000,
    cleanupAfter: '2026-08-01T08:00:00.000Z',
    lastCleanupAttemptAt: '2026-08-27T08:10:00.000Z',
    lastCleanupError: 'managed storage temporarily unavailable',
    assets: [],
  },
  {
    backupId: 'bkp-cleanup-deleted',
    jobId: 'job-cleanup-deleted',
    scope: 'stack',
    status: 'success',
    createdAt: '2026-07-20T08:00:00.000Z',
    finishedAt: '2026-07-20T08:00:04.000Z',
    artifactPath: '/srv/dockrev/backups/stack-prod/20260720-080000.tar.zst',
    sizeBytes: 10_000_000,
    cleanupAfter: '2026-07-21T08:00:00.000Z',
    deletedAt: '2026-08-27T08:11:00.000Z',
    assets: [],
  },
  {
    backupId: 'bkp-cleanup-missing',
    jobId: 'job-cleanup-missing',
    scope: 'service',
    status: 'success',
    createdAt: '2026-07-19T08:00:00.000Z',
    finishedAt: '2026-07-19T08:00:04.000Z',
    artifactPath: '/srv/dockrev/backups/stack-prod/20260719-080000.tar.zst',
    sizeBytes: 9_000_000,
    cleanupAfter: '2026-07-20T08:00:00.000Z',
    missingAt: '2026-08-27T08:12:00.000Z',
    assets: [],
  },
]

const meta: Meta<typeof BackupRecordList> = {
  title: 'Components/ServiceBackupRecords',
  component: BackupRecordList,
  parameters: { layout: 'padded' },
}

export default meta
type Story = StoryObj<typeof meta>

export const CleanupStates: Story = {
  args: { records },
  render: (args) => (
    <div
      className="serviceBackupEvidenceFrame"
      style={{ padding: 36, borderRadius: 16, background: 'var(--dockrev-surface-strong)' }}
    >
      <BackupRecordList {...args} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const text = canvasElement.textContent ?? ''
    if (!text.includes('清理延迟')) throw new Error('cleanup delayed status missing')
    if (!text.includes('已删除')) throw new Error('deleted status missing')
    if (!text.includes('文件已缺失（已核实）')) throw new Error('verified missing status missing')
    if (!text.includes('managed storage temporarily unavailable')) throw new Error('cleanup error missing')
    if (canvasElement.querySelectorAll('[data-slot="alert"]').length !== 1) throw new Error('cleanup delay should use one Alert')
    if (!canvasElement.querySelector('[data-slot="alert"] svg')) throw new Error('cleanup delay Alert should include an icon')
    const cards = canvasElement.querySelectorAll('[data-service-backup-record-status]')
    if (cards.length !== 3) throw new Error(`expected 3 cleanup state cards, got ${cards.length}`)
  },
}

export const RetainedDueBackup: Story = {
  args: {
    keepLast: 1,
    records: [{
      ...records[0],
      backupId: 'bkp-retained-due',
      createdAt: '2026-08-27T08:00:00.000Z',
      cleanupAfter: '2026-08-01T08:00:00.000Z',
      lastCleanupAttemptAt: null,
      lastCleanupError: null,
    }],
  },
  play: async ({ canvasElement }) => {
    if (!canvasElement.textContent?.includes('成功')) throw new Error('retained backup should remain successful')
    if (canvasElement.querySelector('[data-slot="alert"]')) throw new Error('retained backup should not show cleanup delay')
  },
}
