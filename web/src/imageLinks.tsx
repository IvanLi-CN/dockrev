import type { ComponentProps } from 'react'
import { ExternalLinkIcon, GitHubIcon, IconLink } from './ui'

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
  if (registry === 'ghcr.io') return `https://ghcr.io/${name}`
  if (registry !== 'docker.io') return null

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

export function isGitHubRepoUrl(input: string | null | undefined): boolean {
  const value = normalizeExternalHttpUrl(input)
  if (!value) return false
  try {
    const parsed = new URL(value)
    const host = parsed.hostname.toLowerCase()
    return host === 'github.com' || host === 'www.github.com'
  } catch {
    return false
  }
}

type IconLinkClick = ComponentProps<'a'>['onClick']

export function RegistryLinkIcon(props: { imageRef: string; onClick?: IconLinkClick }) {
  const href = buildRegistryWebUrl(props.imageRef)
  if (!href) return null
  return (
    <IconLink href={href} onClick={props.onClick} title="打开镜像注册表页面">
      <ExternalLinkIcon className="inlineIcon" />
    </IconLink>
  )
}

export function RepositoryLinkIcon(props: { repoUrl?: string | null; onClick?: IconLinkClick }) {
  const href = normalizeExternalHttpUrl(props.repoUrl)
  if (!href) return null
  return (
    <IconLink href={href} onClick={props.onClick} title="打开代码仓库页面">
      {isGitHubRepoUrl(href) ? <GitHubIcon className="inlineIcon" /> : <ExternalLinkIcon className="inlineIcon" />}
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
