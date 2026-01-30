import { Fragment } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import type { Service } from '../../api'
import { StatusRemark } from '../../ui'
import { serviceRowStatus } from '../../updateStatus'

const meta: Meta<typeof StatusRemark> = {
  title: 'Components/StatusRemark',
  component: StatusRemark,
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

export const AllStatuses: Story = {
  render: () => {
    const updatable = {
      ...baseService(),
      id: 'svc-updatable',
      name: 'updatable',
      image: { ref: 'ghcr.io/acme/api', tag: '5.2.1', digest: d('a', 'b1') },
      candidate: { tag: '5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
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
    } satisfies Service

    const hint = {
      ...baseService(),
      id: 'svc-hint',
      name: 'hint',
      image: { ref: 'ghcr.io/grafana/loki', tag: '2.9.0', digest: d('1', '11') },
      candidate: { tag: '2.9.1', digest: d('2', '22'), archMatch: 'unknown', arch: ['linux/amd64', 'linux/arm64'] },
    } satisfies Service

    const crossTag = {
      ...baseService(),
      id: 'svc-cross-tag',
      name: '跨标签',
      image: { ref: 'docker.io/library/postgres', tag: '16', digest: d('p', '16') },
      candidate: { tag: '18.1', digest: d('p', '18'), archMatch: 'match', arch: ['linux/amd64'] },
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

    const list = [updatable, updatableForceBackup, hint, crossTag, archMismatch, blocked]

    return (
      <div className="card" style={{ width: 520 }}>
        <div className="title">状态 / 备注</div>
        <div className="muted" style={{ marginTop: 6 }}>
          多状态对照（绿/黄/灰/红）
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
