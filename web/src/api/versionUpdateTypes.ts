export type ServiceVersionUpdateClassification = 'normal' | 'forced'

export type ServiceVersionTagObservation = { version: string; digest: string; observedAt: string }

export type ServiceVersionTagObservationsResponse = {
  imageRepo: string
  configuredTag: string
  observations: ServiceVersionTagObservation[]
}

export type PreviewServiceVersionUpdateResponse = {
  releaseTag: string
  classification: ServiceVersionUpdateClassification
  targetDigest: string
  currentDigest: string
  currentVersion: string
  imageReference: string
  imageRepo: string
  configuredTag: string
}

export type TriggerServiceVersionUpdateInput = PreviewServiceVersionUpdateResponse & {
  forceConfirmed: boolean
  backupMode: 'inherit' | 'skip' | 'force'
}

export type TriggerServiceVersionUpdateResponse = { jobId: string }
