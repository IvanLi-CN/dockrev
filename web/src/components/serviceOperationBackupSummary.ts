import type { ServiceBackupRecordItem } from '../api'
import { formatBytes } from '../pages/settings/helpers'

export type ServiceOperationBackupSummary =
  | {
      state: 'empty'
    }
  | {
      state: 'partial'
      targetCount: number
    }
  | {
      state: 'ready'
      targetCount: number
      sizeLabel: string
    }

export function backupTargetCountLabel(count: number): string {
  return `${count} 个目标`
}

export function backupSummaryValue(summary: ServiceOperationBackupSummary): string | null {
  if (summary.state === 'empty') return null
  const targetLabel = backupTargetCountLabel(summary.targetCount)
  return summary.state === 'ready' ? `${targetLabel} · ${summary.sizeLabel}` : `${targetLabel} · --`
}

export function summarizeServiceOperationBackups(records: ServiceBackupRecordItem[]): Map<string, ServiceOperationBackupSummary> {
  const summaryByJobId = new Map<string, ServiceOperationBackupSummary>()
  const recordsByJobId = new Map<string, ServiceBackupRecordItem[]>()

  for (const record of records) {
    const jobRecords = recordsByJobId.get(record.jobId)
    if (jobRecords) jobRecords.push(record)
    else recordsByJobId.set(record.jobId, [record])
  }

  for (const [jobId, jobRecords] of recordsByJobId.entries()) {
    const includedAssets = jobRecords.flatMap((record) =>
      Array.isArray(record.assets) ? record.assets.filter((asset) => asset.status === 'included') : [],
    )

    if (includedAssets.length === 0) {
      summaryByJobId.set(jobId, { state: 'empty' })
      continue
    }

    const hasMissingSize = includedAssets.some((asset) => asset.sizeBytes == null)
    if (hasMissingSize) {
      summaryByJobId.set(jobId, {
        state: 'partial',
        targetCount: includedAssets.length,
      })
      continue
    }

    const totalSizeBytes = includedAssets.reduce((sum, asset) => sum + (asset.sizeBytes ?? 0), 0)
    summaryByJobId.set(jobId, {
      state: 'ready',
      targetCount: includedAssets.length,
      sizeLabel: formatBytes(totalSizeBytes),
    })
  }

  return summaryByJobId
}
