import refresh from '@iconify-icons/mdi/refresh'
import trashCanOutline from '@iconify-icons/mdi/trash-can-outline'
import { Icon } from '@iconify/react'
import type { Meta, StoryObj } from '@storybook/react'
import { ResponsiveActionButton } from '../../ui'

const meta: Meta<typeof ResponsiveActionButton> = {
  title: 'Components/ResponsiveActionButton',
  component: ResponsiveActionButton,
  args: {
    variant: 'ghost',
    disabled: false,
    label: '重新注册',
    hint: '重新触发 webhook 注册任务',
  },
  argTypes: {
    icon: { control: false },
    onClick: { action: 'clicked' },
  },
  render: (args) => <ResponsiveActionButton {...args} icon={<Icon icon={refresh} aria-hidden="true" />} />,
}

export default meta
type Story = StoryObj<typeof ResponsiveActionButton>

export const Register: Story = {}

export const Delete: Story = {
  args: {
    variant: 'danger',
    label: '删除',
    hint: '先反注册 webhook，成功后移除记录',
  },
  render: (args) => <ResponsiveActionButton {...args} icon={<Icon icon={trashCanOutline} aria-hidden="true" />} />,
}

export const GroupPreview: Story = {
  args: {
    label: '重新注册',
    hint: '重新触发 webhook 注册任务',
  },
  render: () => (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
      <ResponsiveActionButton
        variant="ghost"
        label="重新注册"
        hint="重新触发 webhook 注册任务"
        icon={<Icon icon={refresh} aria-hidden="true" />}
      />
      <ResponsiveActionButton
        variant="danger"
        label="删除"
        hint="先反注册 webhook，成功后移除记录"
        icon={<Icon icon={trashCanOutline} aria-hidden="true" />}
      />
    </div>
  ),
}
