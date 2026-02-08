import type { Meta, StoryObj } from '@storybook/react'
import { CurrentVersionPopover } from '../../components/CurrentVersionPopover'
import { ArrowRightIcon } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function Demo(props: {
  displayTag: string
  imageTag: string
  imageDigest?: string | null
  resolvedTag?: string | null
  resolvedTags?: string[] | null
}) {
  const resolvedTrim = (props.resolvedTag ?? '').trim()
  const rawTrim = (props.imageTag ?? '').trim()
  const showRawTag = Boolean(resolvedTrim && rawTrim && resolvedTrim !== rawTrim)
  return (
    <div style={{ padding: 16, maxWidth: 560, display: 'grid', gap: 12 }}>
      <div style={{ maxWidth: 360 }}>
        <div className="cellTwoLine">
          <div className="versionLine">
            <CurrentVersionPopover
              serviceId="svc-prod-web"
              displayTag={props.displayTag}
              imageTag={props.imageTag}
              imageDigest={props.imageDigest}
              resolvedTag={props.resolvedTag}
              resolvedTags={props.resolvedTags}
            />
            <ArrowRightIcon className="inlineIcon" />
            <span className="mono monoPrimary">v0.1.9</span>
          </div>
          {showRawTag ? (
            <div>
              <CurrentVersionPopover
                serviceId="svc-prod-web"
                displayTag={props.displayTag}
                imageTag={props.imageTag}
                imageDigest={props.imageDigest}
                resolvedTag={props.resolvedTag}
                resolvedTags={props.resolvedTags}
                triggerClassName="versionTagsTrigger mono monoSecondary"
              >
                {props.imageTag}
              </CurrentVersionPopover>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  )
}

const meta: Meta<typeof Demo> = {
  title: 'Components/CurrentVersionPopover',
  component: Demo,
  decorators: [withDockrevMockApi],
  parameters: { dockrevApiScenario: 'dashboard-demo' },
}

export default meta
type Story = StoryObj<typeof Demo>

export const Unknown: Story = {
  args: {
    displayTag: '',
    imageTag: 'latest',
    imageDigest: null,
    resolvedTag: null,
    resolvedTags: null,
  },
}

export const Resolved: Story = {
  args: {
    displayTag: 'v0.1.8',
    imageTag: 'latest',
    imageDigest: 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    resolvedTag: 'v0.1.8',
    resolvedTags: ['v0.1.8', '0.1.8'],
  },
}

export const SemverTag: Story = {
  args: {
    displayTag: 'v1.2.3',
    imageTag: 'v1.2.3',
    imageDigest: 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    resolvedTag: null,
    resolvedTags: null,
  },
}
