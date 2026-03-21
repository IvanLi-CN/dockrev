import { Fragment } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import type {
  NewVersionDiscoveryTimelineResponse,
  Service,
} from '../../api'
import { StatusRemark } from '../../ui'
import { serviceRowStatus } from '../../updateStatus'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof StatusRemark> = {
  title: 'Components/StatusRemark',
  tags: ['autodocs'],
  component: StatusRemark,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof StatusRemark>

const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

function baseService(): Service {
  return {
    id: 'svc',
    name: 'svc',
    image: { ref: 'ghcr.io/acme/app', tag: '1.0.0', digest: d('a', 'b1') },
    candidate: null,
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
    archived: false,
  }
}

const discoveryTimelineByServiceId = {
  'svc-updatable': {
    items: [
      { kind: 'currentCandidate', version: '5.2.3', occurredAt: '2026-03-22T03:00:00+08:00' },
      { kind: 'historicalCandidate', version: '5.2.2', occurredAt: '2026-03-22T02:23:00+08:00' },
      { kind: 'historicalCandidate', version: '5.2.2-rc.1', occurredAt: '2026-03-21T23:42:00+08:00' },
      { kind: 'currentRunning', version: '5.2.1', occurredAt: '2026-03-21T21:08:00+08:00' },
    ],
  },
  'svc-updatable-force': {
    items: [
      { kind: 'currentCandidate', version: '5.2.7', occurredAt: '2026-03-22T01:46:00+08:00' },
      { kind: 'historicalCandidate', version: '5.2.6', occurredAt: '2026-03-21T22:14:00+08:00' },
      { kind: 'currentRunning', version: '5.2', occurredAt: '2026-03-21T18:32:00+08:00' },
    ],
  },
  'svc-hint': {
    items: [
      { kind: 'currentCandidate', version: '2.9.1', occurredAt: '2026-03-22T00:56:00+08:00' },
      { kind: 'historicalCandidate', version: '2.9.1-rc.3', occurredAt: '2026-03-21T22:48:00+08:00' },
      { kind: 'historicalCandidate', version: '2.9.1-rc.2', occurredAt: '2026-03-21T20:11:00+08:00' },
      { kind: 'historicalCandidate', version: '2.9.1-rc.1', occurredAt: '2026-03-21T18:05:00+08:00' },
      { kind: 'currentRunning', version: '2.9.0', occurredAt: '2026-03-21T14:40:00+08:00' },
    ],
  },
} satisfies Record<string, NewVersionDiscoveryTimelineResponse>

export const AllStatuses: Story = {
  parameters: {
    dockrevApiScenario: 'default',
    dockrevDiscoveryTimelineByServiceId: discoveryTimelineByServiceId,
  },
  render: () => {
    const updatable = {
      ...baseService(),
      id: 'svc-updatable',
      name: 'updatable',
      image: { ref: 'ghcr.io/acme/api', tag: '5.2.1', digest: d('a', 'b1') },
      candidate: { tag: '5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
      newVersionDiscoveryCount: 3,
    } satisfies Service

    const updatableForceBackup = {
      ...baseService(),
      id: 'svc-updatable-force',
      name: 'updatable(force backup)',
      image: { ref: 'harbor.local/ops/web', tag: '5.2', digest: d('c', 'c2') },
      candidate: { tag: '5.2.7', digest: d('d', '7a'), archMatch: 'match', arch: ['linux/amd64'] },
      settings: {
        autoRollback: true,
        backupTargets: {
          bindPaths: { '/var/lib/web/uploads': 'force' },
          volumeNames: {},
        },
      },
      newVersionDiscoveryCount: 2,
    } satisfies Service

    const hint = {
      ...baseService(),
      id: 'svc-hint',
      name: 'hint',
      image: { ref: 'ghcr.io/grafana/loki', tag: '2.9.0', digest: d('1', '11') },
      candidate: { tag: '2.9.1', digest: d('2', '22'), archMatch: 'unknown', arch: ['linux/amd64', 'linux/arm64'] },
      newVersionDiscoveryCount: 4,
    } satisfies Service

    const archMismatch = {
      ...baseService(),
      id: 'svc-arch-mismatch',
      name: 'arch mismatch',
      image: { ref: 'quay.io/prometheus/prometheus', tag: '2.49.0', digest: d('3', '33') },
      candidate: { tag: '2.50.0', digest: d('4', '44'), archMatch: 'mismatch', arch: ['linux/arm64'] },
    } satisfies Service

    const blocked = {
      ...baseService(),
      id: 'svc-blocked',
      name: 'blocked',
      image: { ref: 'ghcr.io/acme/worker', tag: '5.2.0', digest: d('e', 'aa') },
      candidate: { tag: '5.2.2', digest: d('f', '0d'), archMatch: 'match', arch: ['linux/amd64'] },
      ignore: { matched: true, ruleId: 'ignore-prod-worker', reason: '备份失败（fail-closed）' },
    } satisfies Service

    const list = [updatable, updatableForceBackup, hint, archMismatch, blocked]

    return (
      <div className="card" style={{ width: 520 }}>
        <div className="title">状态 / 备注</div>
        <div className="muted" style={{ marginTop: 6 }}>
          多状态对照（Iconify + 状态色）
        </div>
        <div
          style={{
            marginTop: 14,
            display: 'grid',
            gridTemplateColumns: '220px 1fr',
            columnGap: 14,
            rowGap: 16,
          }}
        >
          {list.map((svc) => (
            <Fragment key={svc.id}>
              <div
                className="mono"
                style={{
                  paddingTop: 8,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
                title={svc.name}
              >
                {svc.name}
              </div>
              <div>
                <StatusRemark service={svc} status={serviceRowStatus(svc)} />
              </div>
            </Fragment>
          ))}
        </div>
      </div>
    )
  },
}

export const AllStatusesLight: Story = {
  ...AllStatuses,
  globals: {
    theme: 'light',
    backgrounds: { value: 'light' },
  },
}
