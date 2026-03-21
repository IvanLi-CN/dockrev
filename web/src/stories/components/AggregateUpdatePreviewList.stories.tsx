import type { Meta, StoryObj } from '@storybook/react'

import type { Service } from '../../api'
import { AggregateUpdatePreviewList, type AggregateUpdatePreviewListItem } from '../../components/AggregateUpdatePreviewList'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof AggregateUpdatePreviewList> = {
  title: 'Components/AggregateUpdatePreviewList',
  tags: ['autodocs'],
  component: AggregateUpdatePreviewList,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof AggregateUpdatePreviewList>

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

export const AllStates: Story = {
  parameters: { dockrevApiScenario: 'default' },
  render: () => {
    const api = {
      ...baseService(),
      id: 'svc-api',
      name: 'api',
      image: { ref: 'ghcr.io/acme/api', tag: '5.2.1', digest: d('b', '11') },
      candidate: { tag: '5.2.3', digest: d('c', '22'), archMatch: 'match', arch: ['linux/amd64'] },
      newVersionDiscoveryCount: 3,
    } satisfies Service

    const worker = {
      ...baseService(),
      id: 'svc-worker',
      name: 'worker',
      image: { ref: 'ghcr.io/acme/worker', tag: '5.2.0', digest: d('d', '33') },
      candidate: { tag: '5.2.2', digest: d('e', '44'), archMatch: 'unknown', arch: ['linux/amd64', 'linux/arm64'] },
      newVersionDiscoveryCount: 4,
    } satisfies Service

    const dockrev = {
      ...baseService(),
      id: 'svc-dockrev',
      name: 'dockrev',
      image: { ref: 'ghcr.io/ivan/dockrev', tag: '0.14.0', digest: d('f', '55') },
      candidate: { tag: '0.14.1', digest: d('1', '66'), archMatch: 'match', arch: ['linux/amd64'] },
      newVersionDiscoveryCount: 2,
    } satisfies Service

    const items: AggregateUpdatePreviewListItem[] = [
      { svc: api, status: 'updatable' },
      { svc: worker, status: 'hint' },
      {
        svc: dockrev,
        status: 'updatable',
        guardedDockrev: true,
        displayName: 'dockrev (guarded)',
      },
    ]

    return (
      <div className="card" style={{ width: 760 }}>
        <div className="title">聚合更新预览</div>
        <div className="muted" style={{ marginTop: 6 }}>
          状态标签与发现次数计数并排展示
        </div>
        <div style={{ marginTop: 14 }}>
          <AggregateUpdatePreviewList
            items={items}
            dockrevGuardHint="Dockrev 自升级不参与聚合更新，需改走 Supervisor。"
          />
        </div>
      </div>
    )
  },
}

export const AllStatesLight: Story = {
  ...AllStates,
  globals: {
    theme: 'light',
    backgrounds: { value: 'light' },
  },
}
