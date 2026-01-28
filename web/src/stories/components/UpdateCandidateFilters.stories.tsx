import { useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { UpdateCandidateFilters, type UpdateCandidateFilter } from '../../components/UpdateCandidateFilters'

const meta: Meta<typeof UpdateCandidateFilters> = {
  title: 'Components/UpdateCandidateFilters',
  component: UpdateCandidateFilters,
  args: {
    total: 18,
    counts: { updatable: 5, hint: 2, crossTag: 1, archMismatch: 0, blocked: 0 },
  },
}

export default meta
type Story = StoryObj<typeof UpdateCandidateFilters>

function Interactive(props: { total: number; counts: { updatable: number; hint: number; crossTag: number; archMismatch: number; blocked: number } }) {
  const [value, onChange] = useState<UpdateCandidateFilter>('all')
  return <UpdateCandidateFilters value={value} onChange={onChange} total={props.total} counts={props.counts} />
}

export const Default: Story = {
  render: (args) => <Interactive total={args.total} counts={args.counts} />,
}

export const Zeros: Story = {
  args: { total: 12, counts: { updatable: 0, hint: 0, crossTag: 0, archMismatch: 0, blocked: 0 } },
  render: (args) => <Interactive total={args.total} counts={args.counts} />,
}

export const Mixed: Story = {
  args: { total: 23, counts: { updatable: 0, hint: 3, crossTag: 4, archMismatch: 2, blocked: 1 } },
  render: (args) => <Interactive total={args.total} counts={args.counts} />,
}

