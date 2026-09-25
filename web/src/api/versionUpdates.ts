import { apiFetch } from '../api'
import type {
  PreviewServiceVersionUpdateResponse,
  ServiceVersionTagObservationsResponse,
  TriggerServiceVersionUpdateInput,
  TriggerServiceVersionUpdateResponse,
} from './versionUpdateTypes'

export async function getServiceVersionTagObservations(serviceId: string): Promise<ServiceVersionTagObservationsResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/version-update-observations`)
  return (await resp.json()) as ServiceVersionTagObservationsResponse
}

export async function previewServiceVersionUpdate(serviceId: string, releaseTag: string): Promise<PreviewServiceVersionUpdateResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/version-update/preview`, {
    method: 'POST', body: JSON.stringify({ releaseTag }),
  })
  return (await resp.json()) as PreviewServiceVersionUpdateResponse
}

export async function triggerServiceVersionUpdate(serviceId: string, input: TriggerServiceVersionUpdateInput): Promise<TriggerServiceVersionUpdateResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/version-update`, {
    method: 'POST', body: JSON.stringify(input),
  })
  return (await resp.json()) as TriggerServiceVersionUpdateResponse
}
