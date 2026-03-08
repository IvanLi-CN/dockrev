import type { Meta, StoryObj } from '@storybook/react'
import { ArrowRightIcon, IconButton, RefreshIcon, TrashIcon } from '../../ui'

const meta: Meta<typeof IconButton> = {
  title: 'Components/IconButton',
  component: IconButton,
  tags: ['autodocs'],
  args: {
    title: '打开详情',
    disabled: false,
    variant: 'ghost',
    children: <ArrowRightIcon className="inlineIcon" />,
  },
  argTypes: {
    children: { control: false },
    onClick: { action: 'clicked' },
  },
}

export default meta

type Story = StoryObj<typeof IconButton>

export const Default: Story = {}

export const Danger: Story = {
  args: {
    title: '删除服务',
    variant: 'danger',
    children: <TrashIcon className="inlineIcon" />,
  },
}

export const Disabled: Story = {
  args: {
    disabled: true,
    title: '刷新',
    children: <RefreshIcon className="inlineIcon" />,
  },
}
