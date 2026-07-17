import { describe, expect, test } from 'bun:test'
import { renderToStaticMarkup } from 'react-dom/server'

import type { ServiceReleaseNoteItem } from '../src/api'
import { ServiceVersionCard, type ServiceVersionCardModel } from '../src/components/ServiceVersionCard'

const releaseItem: ServiceReleaseNoteItem = {
  id: 'release-v0.81.1',
  tagName: 'v0.81.1',
  name: 'v0.81.1',
  originalBody: [
    "## What's Changed",
    '',
    '- fix dashboard badge',
    '- improve release note rendering',
    '',
    '**Full Changelog**: https://github.com/IvanLi-CN/tavily-hikari/compare/v0.81.0...v0.81.1',
  ].join('\n'),
  htmlUrl: 'https://github.com/IvanLi-CN/tavily-hikari/releases/tag/v0.81.1',
  draft: false,
  prerelease: false,
  publishedAt: '2026-07-17T11:07:00.000Z',
  createdAt: '2026-07-17T11:00:00.000Z',
}

function buildCardModel(body: string): ServiceVersionCardModel {
  return {
    item: releaseItem,
    body,
    bodyMissing: false,
    currentMatch: false,
    candidateMatch: true,
    deployedHistorical: false,
    rollbackTargetMatch: false,
    olderThanCurrent: false,
    showUpdate: false,
    showRollback: false,
    updateDisabled: false,
    updateActionLabel: '更新',
    updateActionHint: '发起更新',
    updateDisabledReason: null,
    updateActionVariant: 'primary',
    updateActionPresentation: 'default',
    rollbackDisabledReason: null,
  }
}

describe('ServiceVersionCard markdown rendering', () => {
  test('renders GitHub release markdown as structured HTML', () => {
    const html = renderToStaticMarkup(
      <ServiceVersionCard
        card={buildCardModel(releaseItem.originalBody ?? '')}
        candidateDisplayVersion="v0.81.1"
        rollbackTarget={null}
        rollbackBackupSummary={{ state: 'empty' }}
        viewLabel="原文"
        sourceLabel="GitHub Releases"
        expanded
        onToggleExpanded={() => {}}
        onApplyUpdate={() => {}}
        onRollback={() => {}}
        onOpenRollbackExplanation={() => {}}
      />,
    )

    expect(html).toContain('<h2>')
    expect(html).toContain("What&#x27;s Changed")
    expect(html).toContain('<ul>')
    expect(html).toContain('<li>fix dashboard badge</li>')
    expect(html).toContain(
      '<a href="https://github.com/IvanLi-CN/tavily-hikari/compare/v0.81.0...v0.81.1"',
    )
    expect(html).not.toContain("## What&#x27;s Changed")
  })
})
