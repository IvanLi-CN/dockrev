import {
  ApiError,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type Service,
  type UpdateServiceTargetInput,
} from './api'
import { isStrictSemverTag } from './versionDisplay'

function uniqueTags(tags: string[], excludeTag: string): string[] {
  const normalizedExclude = excludeTag.trim()
  const out: string[] = []
  const seen = new Set<string>()
  for (const rawTag of tags) {
    const tag = rawTag.trim()
    if (!tag || tag === normalizedExclude || seen.has(tag)) continue
    seen.add(tag)
    out.push(tag)
  }
  return out
}

async function loadSnapshotTags(service: Service): Promise<string[]> {
  const digest = service.candidate?.digest?.trim() ?? ''
  if (!digest) return []
  try {
    const snapshot = await getServiceDigestTagsSnapshot(service.id, digest)
    if (isServiceDigestTagsSnapshotPending(snapshot)) return []
    return Array.isArray(snapshot.tags) ? snapshot.tags.filter((tag) => isStrictSemverTag(tag)) : []
  } catch (error: unknown) {
    if (error instanceof ApiError && error.status === 404) return []
    return []
  }
}

export async function buildUpdateServiceTarget(service: Service): Promise<UpdateServiceTargetInput> {
  const serviceId = service.id.trim()
  const targetTag = service.image.tag.trim()
  const targetDigest = service.candidate?.digest?.trim() ?? ''
  if (!serviceId || !targetTag || !targetDigest) {
    throw new Error('service update 缺少必要参数（serviceId/targetTag/targetDigest）')
  }

  const resolvedTag = service.candidate?.resolvedTag ?? ''
  const tags = [isStrictSemverTag(resolvedTag) ? resolvedTag : '', ...(await loadSnapshotTags(service))]
  return {
    serviceId,
    targetTag,
    targetDigest,
    pullTags: uniqueTags(tags, targetTag),
  }
}

export async function buildUpdateServiceTargets(services: Service[]): Promise<UpdateServiceTargetInput[]> {
  return Promise.all(services.map((service) => buildUpdateServiceTarget(service)))
}
