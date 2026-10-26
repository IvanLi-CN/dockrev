import type { Fixture } from '../shared'
import { json } from '../shared'

export function handleStackRecordRoutes(args: {
  method: string
  urlPath: string
  urlPathWithQuery: string
  fixture: Fixture
}): Response | null {
  const { method, urlPath, urlPathWithQuery, fixture } = args
  if (method === 'GET' && (urlPathWithQuery === '/api/stacks' || urlPathWithQuery.startsWith('/api/stacks?'))) {
    const query = urlPathWithQuery.includes('?') ? urlPathWithQuery.split('?')[1] : ''
    const params = new URLSearchParams(query)
    const archived = params.get('archived') ?? 'exclude'
    const state = params.get('state') ?? 'present'
    let stacks = fixture.stacks
    if (state === 'missing') stacks = stacks.filter((stack) => Boolean(stack.missing))
    if (state === 'present') stacks = stacks.filter((stack) => !stack.missing)
    if (archived === 'only') stacks = stacks.filter((stack) => Boolean(stack.archived))
    if (archived === 'exclude') stacks = stacks.filter((stack) => !stack.archived)
    return json({ stacks })
  }
  if (method === 'GET' && urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/record-deletion-preview')) {
    const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
    const item = fixture.stacks.find((stack) => stack.id === id)
    if (!item) return json({ error: { code: 'not_found', message: 'stack not found' } }, { status: 404 })
    return json({
      stackId: id,
      stackName: item.name,
      missing: Boolean(item.missing),
      canDelete: Boolean(item.missing),
      backupArtifacts: [],
      dockerResources: { status: 'unknown', containers: 0, images: 0, volumes: 0, networks: 0 },
    })
  }
  if (method === 'DELETE' && urlPath.startsWith('/api/stacks/')) {
    const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
    const index = fixture.stacks.findIndex((stack) => stack.id === id)
    if (index < 0) return json({ stackId: id, outcome: 'already_absent' })
    if (!fixture.stacks[index]?.missing) {
      return json({ error: { code: 'conflict', message: 'stack is no longer missing' } }, { status: 409 })
    }
    fixture.stacks.splice(index, 1)
    delete fixture.stackById[id]
    return json({ stackId: id, outcome: 'deleted' })
  }
  return null
}
