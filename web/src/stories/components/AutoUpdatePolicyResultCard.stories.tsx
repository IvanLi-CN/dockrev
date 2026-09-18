import type { Meta, StoryObj } from '@storybook/react'

import type { AutoUpdatePolicy } from '../../api'
import { AutoUpdatePolicyResultCard } from '../../components/AutoUpdatePolicyResultCard'

const policy: AutoUpdatePolicy = {
  mode: 'override',
  enabled: true,
  rules: [
    {
      id: 'stable',
      name: 'Stable releases',
      enabled: true,
      matcher: { type: 'semver', pattern: '>=1, <2' },
      action: 'delayed',
      delay: { minAgeSeconds: 3600, minVersionLag: 1 },
    },
  ],
  updatedAt: null,
}

type ProjectionStatus =
  | 'waiting_inference'
  | 'rule_not_matched'
  | 'delayed'
  | 'queued'
  | 'running'
  | 'completed'
  | 'failed'
  | 'skipped'
  | 'unresolved'

const projectionCopy: Record<ProjectionStatus, string> = {
  waiting_inference: 'latest 尚未获得 digest 绑定的版本证据',
  rule_not_matched: '版本证据已就绪，但没有启用规则匹配',
  delayed: '规则已命中，正在等待时间或版本滞后条件',
  queued: '已通过 claim，更新任务已创建',
  running: '更新任务正在执行服务部署',
  completed: '服务已完成更新并结算',
  failed: '更新任务失败，候选仍保留用于诊断',
  skipped: '候选已被替代或被保护条件跳过',
  unresolved: '版本证据无法解析，SemVer 策略已停止',
}

const meta: Meta<typeof AutoUpdatePolicyResultCard> = {
  title: 'Components/AutoUpdatePolicyResultCard',
  component: AutoUpdatePolicyResultCard,
  tags: ['autodocs'],
  parameters: { layout: 'padded' },
}

export default meta
type Story = StoryObj<typeof AutoUpdatePolicyResultCard>

function renderCard(status: ProjectionStatus) {
  return (
    <AutoUpdatePolicyResultCard
      onOpenSettings={() => undefined}
      policy={policy}
      projection={{
        policyStatus: status,
        reason: projectionCopy[status],
        ruleId: status === 'rule_not_matched' || status === 'unresolved' ? null : 'stable',
        evaluatedAt: '2026-09-16T10:00:00Z',
      }}
      scope="stack"
      candidateSettlement={
        status === 'waiting_inference'
          ? {
              status: 'awaiting_inference',
              rawTag: 'latest',
              candidateDigest: 'sha256:awaiting',
              reason: 'version_inference_pending',
              attempts: 1,
              retryAt: '2026-09-16T10:05:00Z',
              discoveredAt: '2026-09-16T10:00:00Z',
              source: 'github_webhook',
              sourceJobId: 'check-webhook',
              hydrationOrigin: 'discovery_history',
            }
          : status === 'completed'
            ? {
                status: 'ready',
                rawTag: 'latest',
                candidateDigest: 'sha256:completed',
                resolvedVersion: 'v1.2.0',
                reason: 'digest_bound_version',
                attempts: 1,
                retryAt: null,
                discoveredAt: '2026-09-16T09:55:00Z',
                source: 'schedule',
                sourceJobId: 'check-schedule',
                hydrationOrigin: 'discovery_history',
              }
          : status === 'unresolved'
            ? {
                status: 'unresolved',
                rawTag: 'latest',
                candidateDigest: 'sha256:unresolved',
                reason: 'version_inference_unresolved',
                attempts: 3,
                retryAt: null,
                discoveredAt: '2026-09-16T10:00:00Z',
              }
            : null
      }
    />
  )
}

export const StateGallery: Story = {
  render: () => (
    <div
      data-visual-evidence-surface
      style={{ background: 'var(--bg)', display: 'grid', gap: 16, padding: 48 }}
    >
      <div data-visual-evidence-target style={{ display: 'grid', gap: 16 }}>
        {(Object.keys(projectionCopy) as ProjectionStatus[]).map((status) => (
          <div key={status} style={{ display: 'grid', gap: 6 }}>
            <div className="label">{status}</div>
            {renderCard(status)}
          </div>
        ))}
      </div>
    </div>
  ),
  play: async ({ canvasElement }) => {
    const cards = canvasElement.querySelectorAll('.autoPolicyResultCard')
    if (cards.length !== Object.keys(projectionCopy).length) {
      throw new Error('policy state gallery should render every stable policy status')
    }
    for (const status of Object.keys(projectionCopy)) {
      if (!canvasElement.textContent?.includes(projectionCopy[status as ProjectionStatus])) {
        throw new Error(`policy state reason missing for ${status}`)
      }
    }
    if (!canvasElement.textContent?.includes('下次重试 2026-09-16T10:05:00Z')) {
      throw new Error('waiting inference retry detail missing')
    }
    if (!canvasElement.textContent?.includes('尝试 3 次')) {
      throw new Error('unresolved inference attempts missing')
    }
    if (!canvasElement.textContent?.includes('来源任务 check-webhook')) {
      throw new Error('candidate source job provenance missing')
    }
    if (!canvasElement.textContent?.includes('回填 discovery_history')) {
      throw new Error('candidate hydration origin missing')
    }
  },
}

export const AwaitingInference: Story = {
  render: () => renderCard('waiting_inference'),
}

export const Delayed: Story = {
  render: () => renderCard('delayed'),
}

export const UpdateCompleted: Story = {
  render: () => renderCard('completed'),
}

export const AmbiguousHistory: Story = {
  render: () => (
    <div
      data-visual-evidence-surface
      style={{ background: 'var(--bg)', padding: 48 }}
    >
      <div data-visual-evidence-target>
        <AutoUpdatePolicyResultCard
          onOpenSettings={() => undefined}
          policy={policy}
          projection={{
            policyStatus: 'skipped',
            reason: 'migration_ambiguous_history',
            ruleId: null,
            evaluatedAt: '2026-09-16T10:00:00Z',
          }}
          scope="stack"
          candidateHydration={{
            status: 'ambiguous_history',
            reason: 'migration_ambiguous_history',
            candidateDigest: 'sha256:ambiguous',
          }}
          candidateSettlement={{
            status: 'unresolved',
            rawTag: 'latest',
            candidateDigest: 'sha256:ambiguous',
            reason: 'migration_ambiguous_history',
            attempts: 0,
            discoveredAt: '2026-09-16T10:00:00Z',
          }}
        />
      </div>
    </div>
  ),
}

export const StateGalleryMobile: Story = {
  ...StateGallery,
  parameters: { viewport: { defaultViewport: 'dockrevMobile' } },
}
