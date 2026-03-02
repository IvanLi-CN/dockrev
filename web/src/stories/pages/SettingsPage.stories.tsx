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
  parameters: { dockrevApiScenario: 'settings-configured' },
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

export const DefaultLight: Story = {
  ...Default,
  globals: {
    theme: 'light',
  },
}

export const ResolveLoading: Story = {
  parameters: { dockrevApiScenario: 'settings-configured-resolve-slow' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'settings' }}
        title="系统设置"
        pageSubtitle="验证 GHCR 解析并添加按钮在慢响应下的加载反馈"
        topbarHint="系统设置"
      >
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const RepoPickerUx: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'settings' }}
        title="系统设置"
        pageSubtitle="验证 GHCR 仓库选择弹窗：默认最近活动排序、搜索筛选与拖动批量开关"
        topbarHint="系统设置"
      >
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const GhcrPreview: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'settings' }}
        title="系统设置"
        pageSubtitle="验证 GHCR Repos 区域仅预览前 6 条并通过“查看更多”进入维护页"
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

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'settings' }} title="系统设置" topbarHint="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
