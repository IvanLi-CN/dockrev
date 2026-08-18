import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

function normalizeBasePath(basePath: string | undefined): string {
  const raw = (basePath ?? '/').trim()
  if (!raw || raw === '/') return '/'
  const withLeadingSlash = raw.startsWith('/') ? raw : `/${raw}`
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`
}

function dockrevAppHtmlPlugin(pwaEnabled: boolean, iconVersion: string): Plugin {
  return {
    name: 'dockrev-app-html-contract',
    transformIndexHtml(html) {
      const versionedHtml = html.replaceAll('%INSTALL_ICON_VERSION%', iconVersion)
      if (pwaEnabled) return versionedHtml
      return versionedHtml.replace(/^\s*<link rel="manifest" href="[^"]+" \/>\s*$/m, '')
    },
  }
}

function installIconVersion(): string {
  const hash = createHash('sha256')
  for (const asset of [
    'apple-touch-icon.png',
    'favicon.ico',
    'favicon.png',
    'pwa-192.png',
    'pwa-512.png',
    'pwa-maskable-192.png',
    'pwa-maskable-512.png',
  ]) {
    hash.update(readFileSync(resolve('public', asset)))
  }
  return hash.digest('hex').slice(0, 12)
}

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
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
  const pwaAsset = (assetPath: string) => `${base}${assetPath}`
  const iconVersion = installIconVersion()
  const versionedPwaAsset = (assetPath: string) => `${pwaAsset(assetPath)}?v=${iconVersion}`

  return {
    base,
    plugins: [
      dockrevAppHtmlPlugin(pwaEnabled, iconVersion),
      react(),
      tailwindcss(),
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
        },
        includeAssets: [
          'favicon.ico',
          'favicon.png',
          'apple-touch-icon.png',
          'pwa-192.png',
          'pwa-512.png',
          'pwa-maskable-192.png',
          'pwa-maskable-512.png',
        ],
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
            { src: versionedPwaAsset('pwa-192.png'), sizes: '192x192', type: 'image/png', purpose: 'any' },
            { src: versionedPwaAsset('pwa-512.png'), sizes: '512x512', type: 'image/png', purpose: 'any' },
            { src: versionedPwaAsset('pwa-maskable-192.png'), sizes: '192x192', type: 'image/png', purpose: 'maskable' },
            { src: versionedPwaAsset('pwa-maskable-512.png'), sizes: '512x512', type: 'image/png', purpose: 'maskable' },
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
