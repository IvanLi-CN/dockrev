import type { Meta, StoryObj } from '@storybook/react'
import { Button } from '../../ui'

const meta: Meta<typeof Button> = {
  title: 'Components/Button',
  component: Button,
  tags: ['autodocs'],
  args: {
    children: '保存设置',
    disabled: false,
    variant: 'primary',
  },
  argTypes: {
    onClick: { action: 'clicked' },
  },
}

export default meta

type Story = StoryObj<typeof Button>

export const Default: Story = {}

export const Ghost: Story = {
  args: {
    variant: 'ghost',
    children: '次要操作',
  },
}

export const Danger: Story = {
  args: {
    variant: 'danger',
    children: '删除',
  },
}

export const Loading: Story = {
  args: {
    children: '保存中',
    loading: true,
  },
}

export const Disabled: Story = {
  args: {
    disabled: true,
  },
}
