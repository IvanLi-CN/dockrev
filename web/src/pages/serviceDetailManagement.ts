import { normalizeDigest } from '../components/digest'
import { imageRepoFromImageRef } from '../imageRepo'
import type { Service } from '../api'
import type { ManagementEvent } from '../managementEvents'

export function managementEventAffectsServiceDetail(
  event: ManagementEvent,
  stackId: string,
  serviceId: string,
  service: Service | null,
): boolean {
  if (
    event.summary.stackId === stackId ||
    event.summary.serviceId === serviceId ||
    event.entities.some((entity) =>
      (entity.entityType === 'stack' && entity.id === stackId) ||
      (entity.entityType === 'service' && entity.id === serviceId),
    )
  ) {
    return true
  }
  if (event.domain !== 'version_inference' || event.summary.phase !== 'finished' || !service) {
    return false
  }

  const imageRepo = typeof event.summary.imageRepo === 'string'
    ? event.summary.imageRepo.trim().toLowerCase()
    : ''
  const digest = typeof event.summary.digest === 'string'
    ? normalizeDigest(event.summary.digest)?.toLowerCase()
    : null
  const serviceRepo = imageRepoFromImageRef(service.image.ref)
  const currentDigest = normalizeDigest(service.image.digest)?.toLowerCase()
  const candidateDigest = normalizeDigest(service.candidate?.digest)?.toLowerCase()
  return Boolean(imageRepo && digest && serviceRepo === imageRepo && (
    currentDigest === digest || candidateDigest === digest
  ))
}
