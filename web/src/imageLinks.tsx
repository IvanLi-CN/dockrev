import type { ComponentProps } from 'react'
import { DockerIcon, GhcrIcon, GitHubIcon, GitLabIcon, IconLink, RegistryIcon, RepositoryIcon } from './ui'

type IconLinkClick = ComponentProps<'a'>['onClick']
type RepositoryIconKind = 'generic' | 'github' | 'gitlab'
type RegistryIconKind = 'docker' | 'generic' | 'ghcr'

function normalizeHost(host: string): string {
  return host.trim().toLowerCase().replace(/^www\./, '')
}

function isGitHubHost(host: string): boolean {
  return host === 'github.com' || host.endsWith('.github.com')
}

function isGitLabHost(host: string): boolean {
  return host === 'gitlab.com' || host.startsWith('gitlab.') || host.endsWith('.gitlab.com') || host.includes('.gitlab.')
}

function isDockerRegistryHost(host: string): boolean {
  return host === 'docker.io' || host === 'index.docker.io' || host === 'registry-1.docker.io'
}

export function splitImageRef(ref: string): { registry: string; name: string } {
  const s = ref.trim()
  const withoutDigest = s.includes('@') ? s.split('@', 1)[0] : s
  const lastSlash = withoutDigest.lastIndexOf('/')
  const lastColon = withoutDigest.lastIndexOf(':')
  const withoutTag = lastColon > lastSlash ? withoutDigest.slice(0, lastColon) : withoutDigest
  const firstSlash = withoutTag.indexOf('/')
  if (firstSlash < 0) {
    return { registry: 'docker.io', name: withoutTag }
  }
  const firstSeg = withoutTag.slice(0, firstSlash)
  const rest = withoutTag.slice(firstSlash + 1)
  const isRegistry = firstSeg.includes('.') || firstSeg.includes(':') || firstSeg === 'localhost'
  if (isRegistry) return { registry: firstSeg, name: rest }
  return { registry: 'docker.io', name: withoutTag }
}

export function splitImageNameForDisplay(
  name: string,
  tag: string | null | undefined,
): { base: string; suffix: string } {
  const n = name.trim()
  if (!n) return { base: '', suffix: '' }

  const at = n.indexOf('@')
  if (at >= 0) return { base: n.slice(0, at), suffix: n.slice(at) }

  const lastSlash = n.lastIndexOf('/')
  const lastColon = n.lastIndexOf(':')
  if (lastColon > lastSlash) return { base: n.slice(0, lastColon), suffix: n.slice(lastColon) }

  const t = (tag ?? '').trim()
  if (!t) return { base: n, suffix: '' }
  if (t.startsWith('sha256:')) return { base: n, suffix: `@${t}` }
  return { base: n, suffix: `:${t}` }
}

export function buildRegistryWebUrl(imageRef: string): string | null {
  const { registry, name } = splitImageRef(imageRef)
  const host = normalizeHost(registry)
  if (host === 'ghcr.io') return `https://ghcr.io/${name}`
  if (host === 'quay.io') return `https://quay.io/repository/${name}`
  if (!isDockerRegistryHost(host)) return null

  const parts = name
    .split('/')
    .map((part) => part.trim())
    .filter(Boolean)
  if (parts.length === 1) return `https://hub.docker.com/_/${parts[0]}`
  if (parts.length === 2 && parts[0] === 'library') return `https://hub.docker.com/_/${parts[1]}`
  if (parts.length === 2) return `https://hub.docker.com/r/${parts[0]}/${parts[1]}`
  return null
}

export function normalizeExternalHttpUrl(input: string | null | undefined): string | null {
  const value = (input ?? '').trim()
  if (!value) return null
  try {
    const parsed = new URL(value)
    if ((parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.host) {
      return value
    }
  } catch {
    return null
  }
  return null
}

function getExternalHost(input: string | null | undefined): string | null {
  const value = normalizeExternalHttpUrl(input)
  if (!value) return null
  try {
    return normalizeHost(new URL(value).hostname)
  } catch {
    return null
  }
}

export function getRepositoryIconKind(input: string | null | undefined): RepositoryIconKind {
  const host = getExternalHost(input)
  if (!host) return 'generic'
  if (isGitHubHost(host)) return 'github'
  if (isGitLabHost(host)) return 'gitlab'
  return 'generic'
}

export function getRegistryIconKind(imageRef: string): RegistryIconKind {
  const { registry } = splitImageRef(imageRef)
  const host = normalizeHost(registry)
  if (host === 'ghcr.io') return 'ghcr'
  if (isDockerRegistryHost(host)) return 'docker'
  return 'generic'
}

function renderRepositoryIcon(kind: RepositoryIconKind) {
  if (kind === 'github') return <GitHubIcon className="brandIcon" />
  if (kind === 'gitlab') return <GitLabIcon className="brandIcon" />
  return <RepositoryIcon className="brandIcon" />
}

function renderRegistryIcon(kind: RegistryIconKind) {
  if (kind === 'ghcr') return <GhcrIcon className="brandIcon" />
  if (kind === 'docker') return <DockerIcon className="brandIcon" />
  return <RegistryIcon className="brandIcon" />
}

function repositoryLinkTitle(kind: RepositoryIconKind): string {
  if (kind === 'github') return '打开 GitHub 仓库'
  if (kind === 'gitlab') return '打开 GitLab 仓库'
  return '打开代码仓库页面'
}

function registryLinkTitle(imageRef: string, kind: RegistryIconKind): string {
  if (kind === 'ghcr') return '打开 GHCR 页面'
  if (kind === 'docker') return '打开 Docker Hub 页面'
  const { registry } = splitImageRef(imageRef)
  if (normalizeHost(registry) === 'quay.io') return '打开 Quay 页面'
  return '打开镜像注册表页面'
}

export function RegistryLinkIcon(props: { imageRef: string; onClick?: IconLinkClick }) {
  const href = buildRegistryWebUrl(props.imageRef)
  if (!href) return null
  const iconKind = getRegistryIconKind(props.imageRef)
  return (
    <IconLink
      href={href}
      iconKind={iconKind}
      linkKind="registry"
      onClick={props.onClick}
      title={registryLinkTitle(props.imageRef, iconKind)}
    >
      {renderRegistryIcon(iconKind)}
    </IconLink>
  )
}

export function RepositoryLinkIcon(props: { repoUrl?: string | null; onClick?: IconLinkClick }) {
  const href = normalizeExternalHttpUrl(props.repoUrl)
  if (!href) return null
  const iconKind = getRepositoryIconKind(href)
  return (
    <IconLink
      href={href}
      iconKind={iconKind}
      linkKind="repo"
      onClick={props.onClick}
      title={repositoryLinkTitle(iconKind)}
    >
      {renderRepositoryIcon(iconKind)}
    </IconLink>
  )
}

export function ImageLinkIcons(props: {
  imageRef: string
  repoUrl?: string | null
  onClick?: IconLinkClick
  className?: string
}) {
  const registryHref = buildRegistryWebUrl(props.imageRef)
  const repoHref = normalizeExternalHttpUrl(props.repoUrl)
  if (!registryHref && !repoHref) return null

  return (
    <span className={props.className ?? 'imageLinkIcons'}>
      {registryHref ? <RegistryLinkIcon imageRef={props.imageRef} onClick={props.onClick} /> : null}
      {repoHref ? <RepositoryLinkIcon repoUrl={repoHref} onClick={props.onClick} /> : null}
    </span>
  )
}
