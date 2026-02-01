import type { Meta, StoryObj } from '@storybook/react'
import { VersionTagsPopover } from '../../components/VersionTagsPopover'
import { ArrowRightIcon } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function d(fill: string, last2: string) {
  return `sha256:${fill.repeat(62)}${last2}`
}

function Demo(props: { serviceId: string; candidateTag: string; candidateDigest: string | null }) {
  return (
    <div style={{ padding: 16, maxWidth: 560, display: 'grid', gap: 12 }}>
      <div className="muted">Hover or click the version line to open the popover.</div>
      <div style={{ maxWidth: 360 }}>
        <div className="cellTwoLine">
          <VersionTagsPopover
            serviceId={props.serviceId}
            candidateTag={props.candidateTag}
            candidateDigest={props.candidateDigest}
            triggerTitle={`${props.candidateTag}${props.candidateDigest ? `@${props.candidateDigest}` : ''}`}
          >
            <>
              <span>?</span> <ArrowRightIcon className="inlineIcon" /> <span>{props.candidateTag}</span>
            </>
          </VersionTagsPopover>
          <div className="mono monoSecondary">latest</div>
        </div>
      </div>
      <div className="muted">Tip: click to pin; press ESC or click outside to close.</div>
    </div>
  )
}

const meta: Meta<typeof Demo> = {
  title: 'Components/VersionTagsPopover',
  component: Demo,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof Demo>

export const MultiTags: Story = {
  parameters: { dockrevApiScenario: 'version-tags-popover-demo' },
  args: {
    serviceId: 'svc-version-tags',
    candidateTag: 'v0.8.8-arm64',
    candidateDigest: d('b', '9f'),
  },
}

export const MissingDigest: Story = {
  parameters: { dockrevApiScenario: 'version-tags-popover-demo' },
  args: {
    serviceId: 'svc-version-tags',
    candidateTag: 'v0.8.8-arm64',
    candidateDigest: null,
  },
}

export const ApiError: Story = {
  parameters: { dockrevApiScenario: 'error' },
  args: {
    serviceId: 'svc-version-tags',
    candidateTag: 'v0.8.8-arm64',
    candidateDigest: d('b', '9f'),
  },
}

