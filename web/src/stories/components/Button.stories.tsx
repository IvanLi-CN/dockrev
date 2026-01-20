import type { Meta, StoryObj } from '@storybook/react'
import { Button } from '../../ui'

const meta: Meta<typeof Button> = {
  title: 'Components/Button',
  component: Button,
  args: {
    children: 'Button',
    disabled: false,
    variant: 'primary',
  },
}

export default meta
type Story = StoryObj<typeof Button>

export const Primary: Story = { args: { variant: 'primary' } }
export const Ghost: Story = { args: { variant: 'ghost' } }
export const Danger: Story = { args: { variant: 'danger' } }
export const Disabled: Story = { args: { disabled: true } }
