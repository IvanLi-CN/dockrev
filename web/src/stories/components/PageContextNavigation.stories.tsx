import { useState, type ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import {
  CleanupContextNavigation,
  OverviewContextNavigation,
  QueueContextNavigation,
  SettingsContextNavigation,
} from '../../components/PageContextNavigation'
import type { CleanupResourceKind } from '../../api'
import { KIND_LABEL } from '../../pages/cleanupPageModel'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta = {
  title: 'Components/PageContextNavigation',
  decorators: [withDockrevMockApi],
  parameters: { layout: 'fullscreen' },
}

export default meta
type Story = StoryObj<typeof meta>

function ContextStoryFrame(props: { children: ReactNode }) {
  return (
    <aside className="sidebarContextStoryFrame" data-visual-evidence-surface>
      <div data-visual-evidence-target>{props.children}</div>
    </aside>
  )
}

export const Overview: Story = {
  render: () => (
    <ContextStoryFrame>
      <OverviewContextNavigation
        groups={[
          { name: '平台', count: 3, active: true },
          { name: '业务', count: 5 },
          { name: '实验', count: 2 },
        ]}
        onSelectGroup={() => undefined}
      />
    </ContextStoryFrame>
  ),
}

export const Queue: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => (
    <ContextStoryFrame>
      <QueueContextNavigation />
    </ContextStoryFrame>
  ),
}

export const Cleanup: Story = {
  render: function CleanupContextStory() {
    const [scope, setScope] = useState('all')
    const [resourceKinds, setResourceKinds] = useState<CleanupResourceKind[]>([])
    const availableResourceKinds = (Object.keys(KIND_LABEL) as CleanupResourceKind[]).map((key) => ({
      key,
      label: KIND_LABEL[key],
    }))
    return (
      <ContextStoryFrame>
        <CleanupContextNavigation
          scope={scope}
          onScopeChange={setScope}
          resourceKinds={resourceKinds}
          availableResourceKinds={availableResourceKinds}
          onResourceKindsChange={setResourceKinds}
        />
      </ContextStoryFrame>
    )
  },
}

export const Settings: Story = {
  render: () => (
    <ContextStoryFrame>
      <SettingsContextNavigation section="notifications" />
    </ContextStoryFrame>
  ),
}
