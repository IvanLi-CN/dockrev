import type { Meta, StoryObj } from '@storybook/react'
import { Input } from '../../ui'

const meta: Meta<typeof Input> = {
  title: 'Components/Input',
  component: Input,
  tags: ['autodocs'],
  args: {
    className: 'input',
    placeholder: '输入筛选条件',
    disabled: false,
  },
}

export default meta

type Story = StoryObj<typeof Input>

export const Default: Story = {}

export const WithValue: Story = {
  args: {
    value: 'ghcr.io/ivan/dockrev',
  },
}

export const Disabled: Story = {
  args: {
    disabled: true,
    value: '不可编辑',
  },
}
