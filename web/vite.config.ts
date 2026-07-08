import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv, type Connect, type Plugin } from 'vite'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

function appDemoSinglePathPlugin(enabled: boolean): Plugin | null {
  if (!enabled) return null
  const installLegacyDemoPathReject = (middlewares: Connect.Server) => {
    middlewares.use((req, res, next) => {
      const url = new URL(req.url ?? '/', 'http://127.0.0.1')
      const pathname = url.pathname
      if (
        pathname === '/demo' ||
        pathname.startsWith('/demo/') ||
        url.searchParams.has('demo') ||
        url.searchParams.has('dockrev-demo')
      ) {
        res.statusCode = 404
        res.setHeader('Content-Type', 'text/plain; charset=utf-8')
        res.end('Dockrev app demo is served only at / without a demo query.')
        return
      }
      next()
    })
  }
  return {
    name: 'dockrev-homepage-demo-single-path',
    configureServer(server) {
      installLegacyDemoPathReject(server.middlewares)
    },
    configurePreviewServer(server) {
      installLegacyDemoPathReject(server.middlewares)
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
  const demoMode = (env.VITE_DOCKREV_DEMO ?? '').trim().toLowerCase()
  const appDemoEnabled = demoMode === 'app' || demoMode === 'true' || demoMode === '1'

  return {
    plugins: [
      appDemoSinglePathPlugin(appDemoEnabled),
      react(),
      tailwindcss(),
      VitePWA({
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
          id: '/',
          name: 'Dockrev',
          short_name: 'Dockrev',
          description: 'A calm, exact Docker Compose operations console with offline-first readonly snapshots.',
          theme_color: '#061227',
          background_color: '#061227',
          display: 'standalone',
          start_url: '/',
          scope: '/',
          icons: [
            { src: '/pwa-192.png', sizes: '192x192', type: 'image/png' },
            { src: '/pwa-512.png', sizes: '512x512', type: 'image/png' },
            { src: '/pwa-512.png', sizes: '512x512', type: 'image/png', purpose: 'any maskable' },
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
