import { createServer } from 'node:http'
import { access, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import { chromium } from 'playwright'

function usage() {
  console.error('Usage: demo-site-smoke.mjs <dist-demo-dir>')
}

const distDir = process.argv[2]
if (!distDir) {
  usage()
  process.exit(1)
}

const absoluteDist = path.resolve(distDir)

const contentTypeByExtension = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.mjs', 'text/javascript; charset=utf-8'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
  ['.txt', 'text/plain; charset=utf-8'],
  ['.webmanifest', 'application/manifest+json; charset=utf-8'],
  ['.woff2', 'font/woff2'],
])

async function fileExists(filePath) {
  try {
    await access(filePath)
    return true
  } catch {
    return false
  }
}

async function resolveFile(requestPathname) {
  const decoded = decodeURIComponent(requestPathname)
  const normalized = path.posix.normalize(decoded)
  const candidate = path.join(absoluteDist, normalized)
  if (!candidate.startsWith(absoluteDist)) return null
  if (!(await fileExists(candidate))) return null
  const info = await stat(candidate)
  return info.isFile() ? candidate : null
}

/**
 * Mirrors the EdgeOne rewrite contract published in `edgeone.json`: an app
 * route without an extension is served by index.html, while assets keep 404.
 */
async function startStaticServer() {
  const edgeone = JSON.parse(await readFile(path.join(absoluteDist, 'edgeone.json'), 'utf8'))
  const rewrites = (edgeone.rewrites ?? []).map((rule) => ({
    exact: !rule.source.endsWith('*'),
    prefix: rule.source.replace(/\*$/, ''),
    destination: rule.destination,
  }))

  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1')
    let filePath = await resolveFile(url.pathname)

    if (!filePath) {
      const rule = rewrites.find((candidate) =>
        candidate.exact ? url.pathname === candidate.prefix : url.pathname.startsWith(candidate.prefix),
      )
      if (rule) filePath = await resolveFile(rule.destination)
    }
    if (!filePath && url.pathname.endsWith('/')) {
      filePath = await resolveFile(`${url.pathname}index.html`)
    }
    if (!filePath) {
      filePath = path.join(absoluteDist, '404.html')
      if (!(await fileExists(filePath))) {
        res.statusCode = 404
        res.end('not found')
        return
      }
      res.statusCode = 404
    }

    const body = await readFile(filePath)
    res.setHeader('Content-Type', contentTypeByExtension.get(path.extname(filePath)) ?? 'application/octet-stream')
    res.end(body)
  })

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const address = server.address()
  if (!address || typeof address === 'string') throw new Error('failed to bind demo smoke server')
  return { server, origin: `http://127.0.0.1:${address.port}` }
}

async function assertScene(page, origin, { pathname, expectPath, expectTitle }) {
  await page.goto(`${origin}${pathname}`, { waitUntil: 'networkidle' })
  await page.waitForFunction(() => Boolean(window.__DOCKREV_MOCK_DEBUG__), null, { timeout: 15000 })
  const state = await page.evaluate(() => ({
    pathname: window.location.pathname,
    hasMock: Boolean(window.__DOCKREV_MOCK_DEBUG__),
    text: document.body.textContent ?? '',
  }))
  if (state.pathname !== expectPath) {
    throw new Error(`scene ${pathname} resolved to ${state.pathname}, expected ${expectPath}`)
  }
  if (state.text.includes('Web Demo 启动失败')) {
    throw new Error(`scene ${pathname} rendered the demo boot failure surface`)
  }
  if (state.text.includes('unhandled mock route')) {
    throw new Error(`scene ${pathname} rendered an unhandled mock route`)
  }
  if (expectTitle && !state.text.includes(expectTitle)) {
    throw new Error(`scene ${pathname} did not render expected content: ${expectTitle}`)
  }
}

async function runVersionCheck(page, origin) {
  const buildInfo = JSON.parse(await readFile(path.join(absoluteDist, 'build-info.json'), 'utf8'))
  await page.goto(`${origin}/`, { waitUntil: 'networkidle' })
  await page.waitForFunction(() => Boolean(window.__DOCKREV_MOCK_DEBUG__), null, { timeout: 15000 })
  const apiVersion = await page.evaluate(async () => {
    const response = await fetch('/api/version')
    return (await response.json()).version
  })
  if (apiVersion !== buildInfo.version) {
    throw new Error(`mock /api/version returned ${apiVersion}, but the bundle was built as ${buildInfo.version}`)
  }
  return buildInfo
}

const { server, origin } = await startStaticServer()
const browser = await chromium.launch({ headless: true })
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })

try {
  for (const scene of [
    { pathname: '/', expectPath: '/', expectTitle: '导航概览' },
    { pathname: '/services', expectPath: '/services' },
    { pathname: '/queue', expectPath: '/queue' },
    { pathname: '/settings', expectPath: '/settings' },
    { pathname: '/settings/ghcr-webhooks', expectPath: '/settings/ghcr-webhooks' },
    { pathname: '/cleanup', expectPath: '/cleanup' },
    { pathname: '/deploy-check', expectPath: '/deploy-check' },
  ]) {
    await assertScene(page, origin, scene)
  }

  const buildInfo = await runVersionCheck(page, origin)

  const inspector = page.getByRole('region', { name: 'Demo 控制面板' })
  await page.goto(`${origin}/queue?demo=queue`, { waitUntil: 'networkidle' })
  await page.waitForFunction(() => Boolean(window.__DOCKREV_MOCK_DEBUG__), null, { timeout: 15000 })
  const openBubble = page.getByRole('button', { name: '打开 Web Demo 调试控制面板' })
  if (await openBubble.count()) await openBubble.first().click()
  if (!(await inspector.isVisible())) {
    throw new Error('demo inspector is not visible on the queue scene')
  }

  // Scene switching is client-side: no document reload, URL keeps the contract.
  await page.evaluate(() => {
    window.__demoSmokeMarker__ = 'alive'
  })
  await inspector.getByRole('combobox', { name: '选择 Demo 场景' }).click()
  await page.getByRole('option', { name: 'Cleanup', exact: true }).click()
  await page.waitForFunction(() => window.location.pathname === '/cleanup', null, { timeout: 15000 })
  const marker = await page.evaluate(() => window.__demoSmokeMarker__)
  if (marker !== 'alive') throw new Error('scene switch reloaded the document')

  const url = new URL(page.url())
  if (url.searchParams.get('demo') !== 'cleanup') {
    throw new Error(`scene switch lost the demo parameter: ${page.url()}`)
  }

  // Environment policy is real behaviour, not URL decoration. Offline pages
  // never reach networkidle because every product read fails on purpose.
  await page.goto(`${origin}/services?demo=services&d_connection=offline&d_controls=hidden`, { waitUntil: 'domcontentloaded' })
  await page.waitForFunction(() => Boolean(window.__DOCKREV_MOCK_DEBUG__), null, { timeout: 15000 })
  const offlineResult = await page.evaluate(async () => {
    try {
      await fetch('/api/settings')
      return 'resolved'
    } catch (error) {
      return error instanceof TypeError ? 'type-error' : String(error)
    }
  })
  if (offlineResult !== 'type-error') {
    throw new Error(`offline demo request did not fail like a browser: ${offlineResult}`)
  }

  // The hidden-controls contract is the clean screenshot mode.
  await page.goto(`${origin}/services?demo=services&d_controls=hidden`, { waitUntil: 'networkidle' })
  const hiddenPanel = await page.getByRole('region', { name: 'Demo 控制面板' }).count()
  if (hiddenPanel !== 0) throw new Error('d_controls=hidden did not hide the inspector')

  // Mobile has its own entry: bubble to drawer, never a desktop-only absence.
  const mobile = await browser.newPage({ viewport: { width: 390, height: 844 } })
  await mobile.goto(`${origin}/cleanup?demo=cleanup`, { waitUntil: 'networkidle' })
  await mobile.waitForFunction(() => Boolean(window.__DOCKREV_MOCK_DEBUG__), null, { timeout: 15000 })
  const bubble = mobile.getByRole('button', { name: '打开 Web Demo 调试控制面板' })
  await bubble.click()
  await mobile.getByRole('region', { name: 'Demo 控制面板' }).waitFor({ timeout: 5000 })
  await mobile.close()

  const hasDemoPrefix = await page.evaluate(async () => {
    const response = await fetch('/build-info.json')
    return (await response.text()).includes('"/demo/')
  })
  if (hasDemoPrefix) throw new Error('build-info still carries a /demo base')

  console.log(`[demo-site-smoke] ok (${buildInfo.variant} ${buildInfo.version} ${buildInfo.channel})`)
} catch (error) {
  const screenshotPath = path.join(absoluteDist, 'demo-site-smoke-failure.png')
  await page.screenshot({ path: screenshotPath, fullPage: true }).catch(() => {})
  throw error
} finally {
  await browser.close()
  server.close()
}
