import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { VitePWA, type ExtendManifestEntriesHook, type VitePluginPWAAPI } from 'vite-plugin-pwa'
import { buildRouteContract } from './src/routeContract'

type BuildVariant = 'live' | 'demo'
type BuildChannel = 'release' | 'main' | 'pr' | 'local'

type ResolvedBuildInfo = {
  variant: BuildVariant
  version: string
  channel: BuildChannel
  sourceSha: string | null
  builtAt: string
}

/**
 * Route patterns that must resolve to the SPA entry when the demo site is
 * hosted without a rewrite-capable web server. Derived from the same route
 * contract the app uses, so a new page cannot silently lose its deep link.
 */
const DEMO_REWRITE_PATTERNS = [
  '/queue',
  '/queue/*',
  '/services',
  '/services/*',
  '/cleanup',
  '/deploy-check',
  '/settings',
  '/settings/*',
] as const

function readGitSha(): string | null {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() || null
  } catch {
    return null
  }
}

function readGitDirty(): boolean {
  try {
    return execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim().length > 0
  } catch {
    return false
  }
}

function resolveBuildInfo(
  variant: BuildVariant,
  env: Record<string, string>,
  command: 'build' | 'serve',
): ResolvedBuildInfo {
  const explicitVersion = (env.DOCKREV_BUILD_VERSION ?? '').trim()
  const declaredChannel = (env.DOCKREV_BUILD_CHANNEL ?? '').trim().toLowerCase()
  const sourceSha = (env.DOCKREV_SOURCE_SHA ?? process.env.GITHUB_SHA ?? readGitSha() ?? '').trim() || null
  const shortSha = sourceSha ? sourceSha.slice(0, 7) : null
  const builtAt = new Date().toISOString()

  if (explicitVersion) {
    return {
      variant,
      version: explicitVersion,
      channel: declaredChannel === 'main' || declaredChannel === 'pr' ? declaredChannel : 'release',
      sourceSha,
      builtAt,
    }
  }
  if (declaredChannel === 'main' && shortSha) {
    return { variant, version: `main+${shortSha}`, channel: 'main', sourceSha, builtAt }
  }
  if (declaredChannel === 'pr' && shortSha) {
    return { variant, version: `0.0.0-pr.${shortSha}`, channel: 'pr', sourceSha, builtAt }
  }
  const localVersion = shortSha
    ? `0.0.0-local${command === 'build' && readGitDirty() ? '.dirty' : ''}+${shortSha}`
    : '0.0.0-local'
  return { variant, version: localVersion, channel: 'local', sourceSha, builtAt }
}

function buildInfoPlugin(info: ResolvedBuildInfo): Plugin {
  return {
    name: 'dockrev-build-info',
    apply: 'build',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'build-info.json',
        source: `${JSON.stringify(info, null, 2)}\n`,
      })
    },
  }
}

function demoEdgeOneRewritesPlugin(enabled: boolean): Plugin {
  return {
    name: 'dockrev-demo-edgeone-rewrites',
    apply: 'build',
    generateBundle() {
      if (!enabled) return
      this.emitFile({
        type: 'asset',
        fileName: 'edgeone.json',
        source: `${JSON.stringify(
          {
            rewrites: DEMO_REWRITE_PATTERNS.map((source) => ({ source, destination: '/index.html' })),
          },
          null,
          2,
        )}\n`,
      })
    },
  }
}

const INSTALL_ICON_SOURCES = [
  'favicon.svg',
  'favicon.png',
  'favicon.ico',
  'pwa-192.png',
  'pwa-512.png',
  'pwa-maskable-192.png',
  'pwa-maskable-512.png',
] as const

type InstallIconAsset = {
  source: (typeof INSTALL_ICON_SOURCES)[number]
  fileName: string
  contents: Buffer
}

function normalizeBasePath(basePath: string | undefined): string {
  const raw = (basePath ?? '/').trim()
  if (!raw || raw === '/') return '/'
  const withLeadingSlash = raw.startsWith('/') ? raw : `/${raw}`
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`
}

function contentHashedFileName(source: string, contents: Buffer): string {
  const extensionIndex = source.lastIndexOf('.')
  const digest = createHash('sha256').update(contents).digest('hex').slice(0, 12)
  return `${source.slice(0, extensionIndex)}-${digest}${source.slice(extensionIndex)}`
}

function resolveInstallIconAssets(publicDir: string): InstallIconAsset[] {
  return INSTALL_ICON_SOURCES.map((source) => {
    const contents = readFileSync(resolve(publicDir, source))
    return { source, contents, fileName: contentHashedFileName(source, contents) }
  })
}

function installIconAssetPlugin(assets: InstallIconAsset[]): Plugin {
  return {
    name: 'dockrev-install-icon-assets',
    apply: 'build',
    generateBundle() {
      for (const asset of assets) {
        this.emitFile({ type: 'asset', fileName: asset.fileName, source: asset.contents })
      }
    },
  }
}

function dockrevAppHtmlPlugin(assets: InstallIconAsset[], base: string, command: 'build' | 'serve'): Plugin {
  const assetUrls = new Map(
    assets.map((asset) => [asset.source, `${base}${command === 'build' ? asset.fileName : asset.source}`]),
  )

  return {
    name: 'dockrev-app-html-contract',
    transformIndexHtml(html) {
      return html
        .replaceAll('%DOCKREV_FAVICON_SVG%', assetUrls.get('favicon.svg') ?? '')
        .replaceAll('%DOCKREV_FAVICON_PNG%', assetUrls.get('favicon.png') ?? '')
        .replaceAll('%DOCKREV_FAVICON_ICO%', assetUrls.get('favicon.ico') ?? '')
    },
  }
}

type VitePwaPlugin = Plugin & { api?: VitePluginPWAAPI }

function excludeManifestFromPrecache(enabled: boolean): Plugin {
  return {
    name: 'dockrev-pwa-precache-contract',
    enforce: 'post',
    buildStart() {
      if (!enabled) return
      const config = this.environment.config
      const pwaPlugin = config.plugins.find((plugin) => plugin.name === 'vite-plugin-pwa') as
        | VitePwaPlugin
        | undefined
      if (!pwaPlugin?.api) throw new Error('vite-plugin-pwa API is unavailable')

      const filterManifestEntries: ExtendManifestEntriesHook = (entries) =>
        entries.filter((entry) => {
          const url = typeof entry === 'string' ? entry : entry.url
          return url !== 'manifest.webmanifest'
        })
      pwaPlugin.api.extendManifestEntries(filterManifestEntries)
    },
  }
}

function routeContractPlugin(base: string): Plugin {
  return {
    name: 'dockrev-route-contract',
    apply: 'build',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: '.dockrev-route-contract.json',
        source: JSON.stringify(buildRouteContract(base), null, 2),
      })
    },
  }
}

const notFoundHtmlPlugin: Plugin = {
  name: 'dockrev-not-found-html-contract',
  enforce: 'post',
  transformIndexHtml(html, ctx) {
    if (!ctx.filename.endsWith('/404.html')) return html
    return html
      .replace(/\s*<link rel="manifest"[^>]*>/g, '')
      .replaceAll('<!-- DOCKREV_RUNTIME_CONFIG -->', '')
  },
  generateBundle(_options, bundle) {
    const entry = Object.values(bundle).find((item) => item.type === 'asset' && item.fileName === '404.html')
    if (!entry || entry.type !== 'asset' || typeof entry.source !== 'string') return
    entry.source = entry.source.replace(/\s*<link rel="manifest"[^>]*>/g, '')
  },
}

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.VITE_API_PROXY_TARGET || 'http://127.0.0.1:50883'
  const parsePort = (value: string | undefined, fallback: number) => {
    const parsed = Number(value)
    return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
  }
  const variant: BuildVariant = mode === 'demo' ? 'demo' : 'live'
  // The demo dev/preview servers are separate surfaces, so they never share a
  // port with the live app or Storybook.
  const devPort = parsePort(
    variant === 'demo' ? env.DOCKREV_DEMO_DEV_PORT : env.DOCKREV_WEB_DEV_PORT,
    variant === 'demo' ? 50888 : 50884,
  )
  const previewPort = parsePort(
    variant === 'demo' ? env.DOCKREV_DEMO_PREVIEW_PORT : env.DOCKREV_WEB_PREVIEW_PORT,
    variant === 'demo' ? 50889 : 50885,
  )
  const buildInfo = resolveBuildInfo(variant, env, command)
  const base = normalizeBasePath(variant === 'demo' ? (env.DOCKREV_DEMO_BASE ?? '/') : env.DOCKREV_WEB_BASE)
  // The demo site is a static artifact with no service worker and no install contract.
  const pwaEnabled =
    variant !== 'demo' && !['0', 'false', 'off'].includes((env.DOCKREV_PWA ?? '').trim().toLowerCase())
  const publicDir = fileURLToPath(new URL('./public', import.meta.url))
  const installIconAssets = resolveInstallIconAssets(publicDir)
  const installIconUrl = (source: (typeof INSTALL_ICON_SOURCES)[number]) => {
    const asset = installIconAssets.find((candidate) => candidate.source === source)
    if (!asset) throw new Error(`Missing install icon asset: ${source}`)
    return `${base}${command === 'build' ? asset.fileName : asset.source}`
  }

  const pwaPlugins = VitePWA({
    disable: !pwaEnabled,
    base,
    strategies: 'injectManifest',
    srcDir: 'src',
    filename: 'sw.ts',
    injectRegister: false,
    registerType: 'prompt',
    injectManifest: {
      globIgnores: ['404.html'],
      maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
      globPatterns: ['**/*.{js,css,html}'],
    },
    includeManifestIcons: false,
    manifest: {
      id: base,
      name: 'Dockrev',
      short_name: 'Dockrev',
      description: 'A calm, exact Docker Compose operations console with offline-first readonly snapshots.',
      theme_color: '#061227',
      background_color: '#061227',
      display: 'standalone',
      start_url: base,
      scope: base,
      icons: [
        { src: installIconUrl('pwa-192.png'), sizes: '192x192', type: 'image/png', purpose: 'any' },
        { src: installIconUrl('pwa-512.png'), sizes: '512x512', type: 'image/png', purpose: 'any' },
        { src: installIconUrl('pwa-maskable-192.png'), sizes: '192x192', type: 'image/png', purpose: 'maskable' },
        { src: installIconUrl('pwa-maskable-512.png'), sizes: '512x512', type: 'image/png', purpose: 'maskable' },
      ],
    },
  })

  return {
    base,
    appType: 'mpa',
    define: {
      __DOCKREV_BUILD_INFO__: JSON.stringify(buildInfo),
      'import.meta.env.VITE_DOCKREV_VARIANT': JSON.stringify(variant),
    },
    plugins: [
      dockrevAppHtmlPlugin(installIconAssets, base, command),
      react(),
      tailwindcss(),
      installIconAssetPlugin(installIconAssets),
      ...pwaPlugins,
      excludeManifestFromPrecache(pwaEnabled),
      routeContractPlugin(base),
      notFoundHtmlPlugin,
      buildInfoPlugin(buildInfo),
      demoEdgeOneRewritesPlugin(variant === 'demo'),
    ],
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    server: {
      port: devPort,
      strictPort: true,
      proxy: {
        '/api': {
          target,
          changeOrigin: true,
        },
      },
    },
    preview: {
      port: previewPort,
      strictPort: true,
    },
    build: {
      outDir: variant === 'demo' ? 'dist-demo' : 'dist',
      rollupOptions: {
        input: {
          index: fileURLToPath(new URL('./index.html', import.meta.url)),
          // The demo site rewrites every app route to index.html through
          // edgeone.json; only the live app keeps the static 404 entry.
          ...(variant === 'demo'
            ? {}
            : { '404': fileURLToPath(new URL('./404.html', import.meta.url)) }),
        },
      },
    },
  }
})
