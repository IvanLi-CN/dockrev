import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { Switch } from '../../ui'

const meta: Meta<typeof Switch> = {
  title: 'Components/Switch',
  component: Switch,
}

export default meta
type Story = StoryObj<typeof Switch>

export const Off: Story = {
  render: () => {
    return <Switch checked={false} onChange={() => {}} />
  },
}

export const On: Story = {
  render: () => {
    return <Switch checked onChange={() => {}} />
  },
}

export const Disabled: Story = {
  render: () => {
    return <Switch checked disabled onChange={() => {}} />
  },
}

function SwitchExample() {
  const [checked, setChecked] = useState(false)
  return <Switch checked={checked} onChange={setChecked} />
}

export const Interactive: Story = {
  render: () => {
    return <SwitchExample />
  },
}
