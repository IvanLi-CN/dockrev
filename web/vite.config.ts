import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

const INSTALL_ICON_SOURCES = [
  'favicon.svg',
  'favicon.png',
  'favicon.ico',
  'apple-touch-icon.png',
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
        .replaceAll('%DOCKREV_APPLE_TOUCH_ICON%', assetUrls.get('apple-touch-icon.png') ?? '')
    },
  }
}

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.VITE_API_PROXY_TARGET || 'http://127.0.0.1:50883'
  const parsePort = (value: string | undefined, fallback: number) => {
    const parsed = Number(value)
    return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
  }
  const devPort = parsePort(env.DOCKREV_WEB_DEV_PORT, 50884)
  const previewPort = parsePort(env.DOCKREV_WEB_PREVIEW_PORT, 50885)
  const base = normalizeBasePath(env.DOCKREV_WEB_BASE)
  const pwaEnabled = !['0', 'false', 'off'].includes((env.DOCKREV_PWA ?? '').trim().toLowerCase())
  const publicDir = fileURLToPath(new URL('./public', import.meta.url))
  const installIconAssets = resolveInstallIconAssets(publicDir)
  const installIconUrl = (source: (typeof INSTALL_ICON_SOURCES)[number]) => {
    const asset = installIconAssets.find((candidate) => candidate.source === source)
    if (!asset) throw new Error(`Missing install icon asset: ${source}`)
    return `${base}${command === 'build' ? asset.fileName : asset.source}`
  }

  return {
    base,
    plugins: [
      dockrevAppHtmlPlugin(installIconAssets, base, command),
      react(),
      tailwindcss(),
      installIconAssetPlugin(installIconAssets),
      VitePWA({
        disable: !pwaEnabled,
        base,
        strategies: 'injectManifest',
        srcDir: 'src',
        filename: 'sw.ts',
        injectRegister: false,
        registerType: 'prompt',
        injectManifest: {
          maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
          globPatterns: [
            '**/*.{js,css,html}',
            'favicon-????????????.{svg,png,ico}',
            'apple-touch-icon-????????????.png',
            'pwa-*-????????????.png',
          ],
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
      }),
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
  }
})
