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

function dockrevAppHtmlPlugin(pwaEnabled: boolean): Plugin {
  return {
    name: 'dockrev-app-html-contract',
    transformIndexHtml(html) {
      if (pwaEnabled) return html
      return html.replace(/^\s*<link rel="manifest" href="[^"]+" \/>\s*$/m, '')
    },
  }
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

  return {
    base,
    plugins: [
      dockrevAppHtmlPlugin(pwaEnabled),
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
        includeAssets: ['favicon.ico', 'favicon.png', 'apple-touch-icon.png', 'pwa-192.png', 'pwa-512.png'],
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
            { src: pwaAsset('pwa-192.png'), sizes: '192x192', type: 'image/png' },
            { src: pwaAsset('pwa-512.png'), sizes: '512x512', type: 'image/png' },
            { src: pwaAsset('pwa-512.png'), sizes: '512x512', type: 'image/png', purpose: 'any maskable' },
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
