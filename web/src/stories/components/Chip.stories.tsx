import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { Chip } from '../../ui'

const meta: Meta<typeof Chip> = {
  title: 'Components/Chip',
  component: Chip,
  args: {
    children: 'Chip',
  },
}

export default meta
type Story = StoryObj<typeof Chip>

export const Inactive: Story = {}

export const Active: Story = { args: { active: true } }

function ToggleExample() {
  const [active, setActive] = useState(false)
  return (
    <Chip active={active} onClick={() => setActive((x) => !x)} title="click to toggle">
      Click me
    </Chip>
  )
}

export const Toggle: Story = {
  render: () => {
    return <ToggleExample />
  },
}
