import type { Meta, StoryObj } from '@storybook/react'
import { Pill } from '../../ui'

const meta: Meta<typeof Pill> = {
  title: 'Components/Pill',
  component: Pill,
  args: {
    children: 'Pill',
    tone: 'ok',
  },
}

export default meta
type Story = StoryObj<typeof Pill>

export const Ok: Story = { args: { tone: 'ok', children: 'ok' } }
export const Warn: Story = { args: { tone: 'warn', children: 'warn' } }
export const Bad: Story = { args: { tone: 'bad', children: 'bad' } }
export const Muted: Story = { args: { tone: 'muted', children: 'muted' } }
