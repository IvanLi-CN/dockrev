export type DockrevBuildVariant = 'live' | 'demo'
export type DockrevBuildChannel = 'release' | 'main' | 'pr' | 'local'

/**
 * Identity of this bundle, resolved once by `vite.config.ts` at build time.
 * A demo bundle has no backend, so its version must come from here and never
 * from `/api/version`.
 */
export type DockrevBuildInfo = {
  variant: DockrevBuildVariant
  version: string
  channel: DockrevBuildChannel
  sourceSha: string | null
  builtAt: string
}

declare const __DOCKREV_BUILD_INFO__: DockrevBuildInfo | undefined

export const UNKNOWN_BUILD_VERSION = 'unknown'

function fallbackBuildInfo(): DockrevBuildInfo {
  return {
    // Only a `vite --mode demo` build injects the demo variant. Anything that
    // runs without the injected constant (unit tests, tooling) is not a demo.
    variant: 'live',
    version: UNKNOWN_BUILD_VERSION,
    channel: 'local',
    sourceSha: null,
    builtAt: '',
  }
}

function normalizeBuildInfo(input: unknown): DockrevBuildInfo {
  if (!input || typeof input !== 'object') return fallbackBuildInfo()
  const candidate = input as Partial<DockrevBuildInfo>
  const version = typeof candidate.version === 'string' ? candidate.version.trim() : ''
  if (!version) return fallbackBuildInfo()
  return {
    variant: candidate.variant === 'demo' ? 'demo' : 'live',
    version,
    channel:
      candidate.channel === 'release' ||
      candidate.channel === 'main' ||
      candidate.channel === 'pr' ||
      candidate.channel === 'local'
        ? candidate.channel
        : 'local',
    sourceSha: typeof candidate.sourceSha === 'string' && candidate.sourceSha ? candidate.sourceSha : null,
    builtAt: typeof candidate.builtAt === 'string' ? candidate.builtAt : '',
  }
}

export function dockrevBuildInfo(): DockrevBuildInfo {
  if (typeof __DOCKREV_BUILD_INFO__ === 'undefined') return fallbackBuildInfo()
  return normalizeBuildInfo(__DOCKREV_BUILD_INFO__)
}

export function isDemoBundle(): boolean {
  return dockrevBuildInfo().variant === 'demo'
}

/**
 * Only a release build points at a real GitHub release tag, and the repository
 * publishes those as `v<version>`. `main+<sha>`, `0.0.0-pr.<sha>` and local
 * builds have no release page, so they stay unlinked instead of emitting a
 * broken tag URL.
 */
export function releaseTagForVersion(info: DockrevBuildInfo = dockrevBuildInfo()): string | null {
  if (info.channel !== 'release') return null
  return /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(info.version) ? `v${info.version}` : null
}
