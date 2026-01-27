import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { UpdateTargetSelect, type SelectedTarget } from '../../components/UpdateTargetSelect'
import { Mono } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function Demo(props: { serviceId: string; currentTag: string }) {
  const [sel, setSel] = useState<SelectedTarget>({ tag: '-', digest: null })
  return (
    <div style={{ padding: 16, maxWidth: 720, display: 'grid', gap: 12 }}>
      <div className="muted">
        selected: <Mono>{sel.tag}</Mono>
      </div>
      <UpdateTargetSelect
        serviceId={props.serviceId}
        currentTag={props.currentTag}
        onChange={(next) => {
          setSel(next)
        }}
      />
    </div>
  )
}

const meta: Meta<typeof Demo> = {
  title: 'Components/UpdateTargetSelect',
  component: Demo,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof Demo>

export const MultipleCandidates: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  args: { serviceId: 'svc-prod-api', currentTag: '5.2.1' },
}

export const SingleCandidate: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  args: { serviceId: 'svc-prod-web', currentTag: '5.2' },
}

