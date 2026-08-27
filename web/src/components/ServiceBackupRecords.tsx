import { TriangleAlert } from 'lucide-react'

import type { ServiceBackupRecordAsset, ServiceBackupRecordItem } from '../api'
import { formatBytes } from '../pages/settings/helpers'
import { Pill } from '../ui'
import { AsyncDataSkeleton } from './AsyncDataRegion'
import { Alert, AlertDescription, AlertTitle } from './ui/alert'

function formatDateTime(value: string | null | undefined): string {
  const date = value ? new Date(value) : null
  if (!date || Number.isNaN(date.getTime())) return value?.trim() || '-'
  return date.toLocaleString()
}

function isCleanupDue(record: ServiceBackupRecordItem): boolean {
  if (!record.cleanupAfter || record.deletedAt || record.missingAt) return false
  const cleanupAt = new Date(record.cleanupAfter)
  return !Number.isNaN(cleanupAt.getTime()) && cleanupAt.getTime() <= Date.now()
}

function backupRecordStatusMeta(record: ServiceBackupRecordItem): { label: string; tone: 'ok' | 'warn' | 'bad' | 'muted' | 'info' } {
  if (record.deletedAt) return { label: '已删除', tone: 'muted' }
  if (record.missingAt) return { label: '文件已缺失（已核实）', tone: 'info' }
  if (isCleanupDue(record)) return { label: '清理延迟', tone: 'warn' }
  if (record.status === 'success') return { label: '成功', tone: 'ok' }
  if (record.status === 'failed') return { label: '失败', tone: 'bad' }
  if (record.status === 'running') return { label: '进行中', tone: 'info' }
  if (record.status === 'skipped') return { label: '已跳过', tone: 'warn' }
  return { label: record.status || '未知', tone: 'muted' }
}

function backupAssetTargetLabel(asset: ServiceBackupRecordAsset): string {
  if (asset.target.kind === 'docker-volume') return asset.target.name
  return asset.target.path
}

function backupAssetStatusLabel(asset: ServiceBackupRecordAsset): string {
  if (asset.status === 'included') {
    if (asset.policy === 'stop_related_services') return '已纳入 · 停机备份'
    if (asset.policy === 'live_backup') return '已纳入 · 在线备份'
    return '已纳入'
  }
  switch (asset.reason) {
    case 'skipped_by_user':
      return '已跳过 · 当前服务未启用'
    case 'skipped_by_size':
      return '已跳过 · 体积超阈值'
    case 'skipped_by_probe_error':
      return '已跳过 · 体积探测失败'
    default:
      return '已跳过'
  }
}

export function BackupRecordList(props: { records: ServiceBackupRecordItem[]; loading?: boolean }) {
  if (props.loading) {
    return <AsyncDataSkeleton className="serviceBackupRecordsLoading" lines={3} />
  }
  if (props.records.length === 0) {
    return (
      <div className="serviceBackupRecordsEmpty" data-service-backup-records-state="empty">
        当前服务暂无实际备份记录。
      </div>
    )
  }

  return (
    <div className="serviceBackupRecordsList" data-service-backup-records-state="ready">
      {props.records.map((record) => {
        const status = backupRecordStatusMeta(record)
        const cleanupDelayed = status.label === '清理延迟'
        const assets = Array.isArray(record.assets) ? record.assets : []
        return (
          <div
            key={record.backupId}
            className="serviceBackupRecordCard"
            data-service-backup-record-status={status.label}
          >
            <div className="serviceBackupRecordHead">
              <div className="serviceBackupRecordHeading">
                <span className="title">备份时间</span>
                <span className="serviceBackupRecordPrimary">{formatDateTime(record.createdAt)}</span>
              </div>
              <Pill tone={status.tone}>{status.label}</Pill>
            </div>
            <div className="serviceBackupRecordMetaGrid">
              <div>
                <div className="muted">备份包体积</div>
                <div>{record.sizeBytes != null ? formatBytes(record.sizeBytes) : '体积未知'}</div>
              </div>
              <div>
                <div className="muted">计划删除时间</div>
                <div>{record.cleanupAfter ? formatDateTime(record.cleanupAfter) : '未计划删除'}</div>
              </div>
              <div>
                <div className="muted">触发范围</div>
                <div className="mono">{record.scope}</div>
              </div>
              {record.deletedAt ? (
                <div>
                  <div className="muted">删除时间</div>
                  <div>{formatDateTime(record.deletedAt)}</div>
                </div>
              ) : null}
              {record.missingAt ? (
                <div>
                  <div className="muted">核实缺失时间</div>
                  <div>{formatDateTime(record.missingAt)}</div>
                </div>
              ) : null}
              {cleanupDelayed ? (
                <div>
                  <div className="muted">最近清理尝试</div>
                  <div>
                    {record.lastCleanupAttemptAt ? formatDateTime(record.lastCleanupAttemptAt) : '等待下一轮清理尝试'}
                  </div>
                </div>
              ) : null}
            </div>
            {record.error ? <div className="serviceBackupRecordError">备份执行错误：{record.error}</div> : null}
            {cleanupDelayed ? (
              <Alert className="serviceBackupCleanupAlert" variant="warning">
                <TriangleAlert aria-hidden="true" size={16} strokeWidth={2.1} />
                <div>
                  <AlertTitle>清理延迟</AlertTitle>
                  <AlertDescription>{record.lastCleanupError || '等待下一轮清理尝试'}</AlertDescription>
                </div>
              </Alert>
            ) : null}
            <div className="serviceBackupAssetList">
              {assets.length === 0 ? (
                <div className="muted">未记录资产明细。</div>
              ) : (
                assets.map((asset, index) => (
                  <div key={`${record.backupId}-${index}-${backupAssetTargetLabel(asset)}`} className="serviceBackupAssetRow">
                    <div>
                      <div className="mono">{backupAssetTargetLabel(asset)}</div>
                      <div className="muted">{backupAssetStatusLabel(asset)}</div>
                    </div>
                    <div className="serviceBackupAssetSize">
                      {asset.sizeBytes != null ? formatBytes(asset.sizeBytes) : '体积未知'}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )
      })}
    </div>
  )
}
