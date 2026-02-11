import type { Meta, StoryObj } from '@storybook/react'
import { CurrentVersionPopover } from '../../components/CurrentVersionPopover'
import { VersionTagsPopover } from '../../components/VersionTagsPopover'
import { ArrowRightIcon } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function d(fill: string, last2: string) {
  return `sha256:${fill.repeat(62)}${last2}`
}

function isStrictSemverTag(tag: string): boolean {
  const t = tag.trim()
  if (!t) return false
  return /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(t)
}

function inferredTagForDisplay(tag: string, resolvedTag: string | null | undefined): string {
  const r = (resolvedTag ?? '').trim()
  if (r && isStrictSemverTag(r)) return r
  const t = (tag ?? '').trim()
  if (t && isStrictSemverTag(t)) return t
  return '-'
}

function Demo(props: { serviceId: string; candidateTag: string; candidateDigest: string | null }) {
  const imageTag = '0.8'
  const imageDigest = props.candidateDigest
  const currentDisplayTag = inferredTagForDisplay(imageTag, null)
  const showRawTag = Boolean(imageTag.trim() && imageTag.trim() !== currentDisplayTag)
  return (
    <div style={{ padding: 16, maxWidth: 560, display: 'grid', gap: 12 }}>
      <div style={{ maxWidth: 360 }}>
        <div className="cellTwoLine">
          <div className="versionLine">
            <CurrentVersionPopover
              serviceId={props.serviceId}
              displayTag=""
              imageTag={imageTag}
              imageDigest={imageDigest}
              resolvedTag={null}
              resolvedTags={null}
            />
            <ArrowRightIcon className="inlineIcon" />
            <VersionTagsPopover
              serviceId={props.serviceId}
              candidateTag={props.candidateTag}
              candidateDigest={props.candidateDigest}
            >
              {props.candidateTag}
            </VersionTagsPopover>
          </div>
          {showRawTag ? (
            <div>
              <CurrentVersionPopover
                serviceId={props.serviceId}
                displayTag={imageTag}
                imageTag={imageTag}
                imageDigest={imageDigest}
                resolvedTag={null}
                resolvedTags={null}
                preferSource="rawTag"
                triggerClassName="versionTagsTrigger mono monoSecondary"
              >
                {imageTag}
              </CurrentVersionPopover>
            </div>
          ) : null}
        </div>
      </div>
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
