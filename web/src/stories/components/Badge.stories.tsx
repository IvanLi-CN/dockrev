import type { Meta, StoryObj } from '@storybook/react'
import { Badge } from '../../components/ui/badge'

const meta: Meta<typeof Badge> = {
  title: 'Components/Badge',
  component: Badge,
  tags: ['autodocs'],
  args: {
    children: 'Active',
    variant: 'default',
  },
}

export default meta

type Story = StoryObj<typeof Badge>

export const Default: Story = {}
export const Secondary: Story = { args: { variant: 'secondary', children: 'Queued' } }
export const Destructive: Story = { args: { variant: 'destructive', children: 'Blocked' } }
export const Outline: Story = { args: { variant: 'outline', children: 'Preview' } }
