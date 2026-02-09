import type { Meta, StoryObj } from '@storybook/react'
import { CurrentVersionPopover } from '../../components/CurrentVersionPopover'
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
  if (r) return r
  const t = (tag ?? '').trim()
  if (t && isStrictSemverTag(t)) return t
  return '-'
}

function Demo(props: {
  displayTag: string
  imageTag: string
  imageDigest?: string | null
  resolvedTag?: string | null
  resolvedTags?: string[] | null
}) {
  const explicitDisplay = props.displayTag.trim()
  const effectiveDisplayTag = explicitDisplay || inferredTagForDisplay(props.imageTag, props.resolvedTag)
  const rawTrim = (props.imageTag ?? '').trim()
  const showRawTag = Boolean(rawTrim && rawTrim !== effectiveDisplayTag)
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
                displayTag={props.imageTag}
                imageTag={props.imageTag}
                imageDigest={props.imageDigest}
                resolvedTag={props.resolvedTag}
                resolvedTags={props.resolvedTags}
                preferSource="rawTag"
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
    imageTag: '5.2',
    imageDigest: null,
    resolvedTag: null,
    resolvedTags: null,
  },
}

export const FloatingLatest: Story = {
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
    displayTag: 'v5.2.1',
    imageTag: 'latest',
    imageDigest: d('c', 'c2'),
    resolvedTag: 'v5.2.1',
    resolvedTags: ['v5.2.1', '5.2.1'],
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
