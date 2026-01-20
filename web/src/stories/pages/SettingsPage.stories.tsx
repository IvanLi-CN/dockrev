import type { Meta, StoryObj } from '@storybook/react'
import { SettingsPage } from '../../pages/SettingsPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof SettingsPage> = {
  title: 'Pages/SettingsPage',
  component: SettingsPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof SettingsPage>

export const Default: Story = {
  render: () => {
    return (
      <PageHarness
        route={{ name: 'settings' }}
        title="系统设置"
        pageSubtitle="单用户 / Forward Header · 认证配置 · 通知配置 · 备份默认策略"
        topbarHint="系统设置"
      >
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'settings' }} title="系统设置" topbarHint="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
