import { createHash } from 'node:crypto'
import { createServer } from 'node:http'
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'

const webDir = fileURLToPath(new URL('..', import.meta.url))
const distDir = path.join(webDir, 'dist')
const fixtureRoot = await mkdtemp(path.join('/tmp', 'dockrev-pwa-update-'))
const v1Dir = path.join(fixtureRoot, 'v1')
const v2Dir = path.join(fixtureRoot, 'v2')

function assert(condition, message) {
  if (!condition) throw new Error(message)
}

function contentTypeFor(filePath) {
  const extension = path.extname(filePath)
  return {
    '.css': 'text/css; charset=utf-8',
    '.html': 'text/html; charset=utf-8',
    '.ico': 'image/x-icon',
    '.js': 'text/javascript; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.png': 'image/png',
    '.svg': 'image/svg+xml',
    '.webmanifest': 'application/manifest+json; charset=utf-8',
  }[extension] ?? 'application/octet-stream'
}

function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}

function fixtureIconName(icon, bytes) {
  const parsed = path.parse(new URL(icon.src, 'http://fixture.test/').pathname)
  const stem = parsed.name.replace(/-[0-9a-f]{12}$/, '')
  return `${stem}-${digest(bytes).slice(0, 12)}${parsed.ext}`
}

function iconPath(root, src) {
  const pathname = new URL(src, 'http://fixture.test/').pathname
  const relativePath = pathname.replace(/^\/+/, '')
  const resolved = path.resolve(root, relativePath)
  const rootPrefix = `${path.resolve(root)}${path.sep}`
  assert(resolved.startsWith(rootPrefix), `icon path escapes fixture: ${src}`)
  return resolved
}

async function addVersionMarker(root, version) {
  const indexPath = path.join(root, 'index.html')
  const index = await readFile(indexPath, 'utf8')
  assert(index.includes('</head>'), 'fixture index is missing </head>')
  await writeFile(
    indexPath,
    index.replace('</head>', `<meta name="dockrev-pwa-fixture" content="${version}" /></head>`),
  )
}

async function prepareFixtures() {
  await cp(distDir, v1Dir, { recursive: true })
  await cp(distDir, v2Dir, { recursive: true })
  await addVersionMarker(v1Dir, 'v1')
  await addVersionMarker(v2Dir, 'v2')

  const v2Manifest = JSON.parse(await readFile(path.join(v2Dir, 'manifest.webmanifest'), 'utf8'))
  const v1Manifest = structuredClone(v2Manifest)
  for (const icon of v1Manifest.icons) {
    const counterpart = v2Manifest.icons.find(
      (candidate) => candidate.sizes === icon.sizes && candidate.purpose !== icon.purpose,
    )
    assert(counterpart, `missing V2 counterpart for ${icon.src}`)
    const bytes = await readFile(iconPath(v2Dir, counterpart.src))
    const oldPath = iconPath(v1Dir, fixtureIconName(icon, bytes))
    await writeFile(oldPath, bytes)
    icon.src = `/${path.relative(v1Dir, oldPath).replaceAll(path.sep, '/')}`
  }
  await writeFile(path.join(v1Dir, 'manifest.webmanifest'), `${JSON.stringify(v1Manifest, null, 2)}\n`)

  const v2Worker = await readFile(path.join(v2Dir, 'sw.js'), 'utf8')
  assert(!v2Worker.includes('404.html'), 'not-found document must not be included in the precache manifest')
  const indexRevision = /(\{\\?"revision\\?":\\?")[^\"]+(\\?",\\?"url\\?":\\?"index\.html"\\?\})/
  assert(indexRevision.test(v2Worker), 'fixture worker is missing its index revision')
  const v1Worker = v2Worker.replace(indexRevision, '$1dockrev-pwa-fixture-v1$2')
  await writeFile(path.join(v1Dir, 'sw.js'), `${v1Worker}\n/* dockrev-pwa-fixture-v1 */\n`)
  return { v1Manifest, v2Manifest }
}

function cacheControlFor(pathname) {
  const fileName = path.posix.basename(pathname)
  if (['index.html', 'manifest.webmanifest', 'sw.js', 'registerSW.js'].includes(fileName)) return 'no-cache'
  if (/^(?:favicon|pwa-192|pwa-512|pwa-maskable-192|pwa-maskable-512)-[0-9a-f]{12}\.(?:svg|png|ico)$/.test(fileName)) {
    return 'public, max-age=31536000, immutable'
  }
  return null
}

async function startFixtureServer() {
  let release = 'v1'
  const requests = []
  const roots = { v1: v1Dir, v2: v2Dir }
  const server = createServer(async (request, response) => {
    const requestUrl = new URL(request.url ?? '/', 'http://127.0.0.1')
    const pathname = decodeURIComponent(requestUrl.pathname)
    requests.push({ release, pathname })

    if (pathname.startsWith('/api/')) {
      response.statusCode = 200
      response.setHeader('Content-Type', 'application/json; charset=utf-8')
      response.setHeader('Cache-Control', 'no-cache')
      response.end(JSON.stringify({ version: release }))
      return
    }

    const root = roots[release]
    if (pathname === '/404.html') {
      const body = await readFile(path.join(root, '404.html'))
      response.statusCode = 404
      response.setHeader('Content-Type', 'text/html; charset=utf-8')
      response.setHeader('Cache-Control', 'no-store')
      response.end(body)
      return
    }

    const relativePath = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '')
    const filePath = path.resolve(root, relativePath)
    const rootPrefix = `${path.resolve(root)}${path.sep}`
    if (!filePath.startsWith(rootPrefix)) {
      response.statusCode = 400
      response.end('bad path')
      return
    }

    try {
      const body = await readFile(filePath)
      response.statusCode = 200
      response.setHeader('Content-Type', contentTypeFor(filePath))
      const cacheControl = cacheControlFor(pathname)
      if (cacheControl) response.setHeader('Cache-Control', cacheControl)
      response.end(body)
    } catch {
      response.statusCode = 404
      response.end('not found')
    }
  })

  await new Promise((resolve, reject) => {
    server.listen(0, '127.0.0.1', (error) => (error ? reject(error) : resolve()))
  })
  const address = server.address()
  assert(address && typeof address !== 'string', 'fixture server did not expose a port')

  return {
    origin: `http://127.0.0.1:${address.port}`,
    requests,
    setRelease(nextRelease) {
      assert(nextRelease === 'v1' || nextRelease === 'v2', `unknown fixture release: ${nextRelease}`)
      release = nextRelease
    },
    close() {
      return new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())))
    },
  }
}

async function fetchManifestAndIcons(page) {
  return page.evaluate(async () => {
    const manifestResponse = await fetch('/manifest.webmanifest', { cache: 'no-store' })
    const manifest = await manifestResponse.json()
    const icons = await Promise.all(
      manifest.icons.map(async (icon) => {
        const response = await fetch(icon.src, { cache: 'no-store' })
        const bytes = new Uint8Array(await response.arrayBuffer())
        const hashBytes = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))
        const hash = [...hashBytes].map((value) => value.toString(16).padStart(2, '0')).join('')
        return {
          cacheControl: response.headers.get('cache-control'),
          hash,
          status: response.status,
          src: icon.src,
        }
      }),
    )
    return {
      icons,
      manifestStatus: manifestResponse.status,
      manifest,
      manifestCacheControl: manifestResponse.headers.get('cache-control'),
    }
  })
}

function assertIdentity(previous, next) {
  for (const field of ['id', 'scope', 'start_url']) {
    assert(previous[field] === next[field], `manifest ${field} changed across the update`)
  }
}

let browser
let fixtureServer
try {
  const { v1Manifest, v2Manifest } = await prepareFixtures()
  fixtureServer = await startFixtureServer()
  browser = await chromium.launch({ headless: true })
  const context = await browser.newContext()
  const page = await context.newPage()

  await page.goto(`${fixtureServer.origin}/`, { waitUntil: 'domcontentloaded' })
  await page.waitForFunction(() => Boolean(navigator.serviceWorker?.controller), undefined, { timeout: 15000 })
  const directNotFound = await page.evaluate(() =>
    fetch('/404.html', { cache: 'no-store' }).then((response) => ({
      cacheControl: response.headers.get('cache-control'),
      status: response.status,
    })),
  )
  assert(directNotFound.status === 404, 'not-found document must preserve its HTTP 404 status')
  assert(directNotFound.cacheControl === 'no-store', 'not-found document must not be cached')
  const legacyAppleTouchIconStatus = await page.evaluate(() =>
    fetch('/apple-touch-icon.png', { cache: 'no-store' }).then((response) => response.status),
  )
  assert(legacyAppleTouchIconStatus === 404, 'product build must not serve a root Apple touch icon fallback')
  const initialMarker = await page.locator('meta[name="dockrev-pwa-fixture"]').getAttribute('content')
  assert(initialMarker === 'v1', 'browser did not start from the V1 shell')
  const initialRegistration = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready
    return { scope: registration.scope, scriptURL: registration.active?.scriptURL ?? null }
  })
  const unknownNavigation = await page.goto(`${fixtureServer.origin}/made-up-deep-link`, { waitUntil: 'domcontentloaded' })
  assert(unknownNavigation?.status() === 404, 'unknown document navigation must not receive the app shell')
  const unknownAssetNavigation = await page.goto(`${fixtureServer.origin}/assets/missing.js`, { waitUntil: 'domcontentloaded' })
  assert(unknownAssetNavigation?.status() === 404, 'unknown static resource navigation must not receive the app shell')
  await page.goto(`${fixtureServer.origin}/`, { waitUntil: 'domcontentloaded' })
  await context.setOffline(true)
  const offlineContractNavigation = await page.goto(`${fixtureServer.origin}/queue`, { waitUntil: 'domcontentloaded' })
  assert(offlineContractNavigation?.status() === 200, 'contract navigation must resolve from the offline app shell')
  const offlineMarker = await page.locator('meta[name="dockrev-pwa-fixture"]').getAttribute('content')
  assert(offlineMarker === 'v1', 'offline contract navigation did not resolve the V1 shell')
  await context.setOffline(false)
  await page.goto(`${fixtureServer.origin}/`, { waitUntil: 'domcontentloaded' })
  const initial = await fetchManifestAndIcons(page)
  assert(initial.manifestStatus === 200, 'V1 manifest was not fetched successfully')
  assert(initial.manifestCacheControl === 'no-cache', 'V1 manifest must be revalidated')
  assert(initial.manifest.icons[0].src === v1Manifest.icons[0].src, 'browser did not start from the V1 manifest')
  assert(initial.manifest.id === v1Manifest.id, 'V1 manifest identity was not served')
  for (const [index, icon] of initial.icons.entries()) {
    const expectedIcon = v1Manifest.icons[index]
    const expectedHash = digest(await readFile(iconPath(v1Dir, expectedIcon.src)))
    assert(icon.src === expectedIcon.src, `V1 icon ${index} URL is stale`)
    assert(icon.status === 200, `V1 icon ${icon.src} was not fetched successfully`)
    assert(icon.hash === expectedHash, `V1 icon ${icon.src} bytes are stale`)
    assert(icon.cacheControl === 'public, max-age=31536000, immutable', `V1 icon ${icon.src} is not immutable`)
  }

  fixtureServer.setRelease('v2')
  const updateState = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.ready
    let updateFound = false
    const onUpdateFound = () => {
      updateFound = true
    }
    registration.addEventListener('updatefound', onUpdateFound)
    const controllerChanged = new Promise((resolve, reject) => {
      const timeout = window.setTimeout(() => reject(new Error('timed out waiting for controllerchange')), 15000)
      navigator.serviceWorker.addEventListener(
        'controllerchange',
        () => {
          window.clearTimeout(timeout)
          resolve()
        },
        { once: true },
      )
    })

    await registration.update()
    let waiting = registration.waiting
    for (let attempt = 0; !waiting && attempt < 150; attempt += 1) {
      await new Promise((resolve) => window.setTimeout(resolve, 100))
      waiting = registration.waiting
    }
    if (!waiting) throw new Error('normal update check did not produce a waiting worker')

    waiting.postMessage({ type: 'SKIP_WAITING' })
    await controllerChanged
    registration.removeEventListener('updatefound', onUpdateFound)
    return { pathname: window.location.pathname, updateFound }
  })
  assert(updateState.updateFound, 'normal update check did not discover the V2 worker')
  assert(updateState.pathname === '/', 'PWA update changed the active page unexpectedly')
  await page.reload({ waitUntil: 'domcontentloaded' })
  await page.waitForFunction(() => document.querySelector('meta[name="dockrev-pwa-fixture"]')?.content === 'v2')

  const updated = await fetchManifestAndIcons(page)
  assertIdentity(initial.manifest, updated.manifest)
  assert(updated.manifestStatus === 200, 'V2 manifest was not fetched successfully')
  assert(updated.manifestCacheControl === 'no-cache', 'V2 manifest must be revalidated')
  assert(updated.manifest.icons[0].src === v2Manifest.icons[0].src, 'browser did not fetch the V2 manifest')
  for (const [index, icon] of updated.icons.entries()) {
    const expectedIcon = v2Manifest.icons[index]
    const expectedHash = digest(await readFile(iconPath(v2Dir, expectedIcon.src)))
    assert(icon.src === expectedIcon.src, `V2 icon ${index} URL is stale`)
    assert(icon.status === 200, `V2 icon ${icon.src} was not fetched successfully`)
    assert(icon.hash === expectedHash, `V2 icon ${icon.src} bytes are stale`)
    assert(icon.cacheControl === 'public, max-age=31536000, immutable', `V2 icon ${icon.src} is not immutable`)
  }
  assert(
    updated.icons.every((icon, index) => icon.src !== initial.icons[index].src),
    'V2 icon URLs did not change across V1 to V2',
  )
  assert(
    updated.icons.every((icon, index) => icon.hash !== initial.icons[index].hash),
    'V2 icon bytes did not change across V1 to V2',
  )
  assert(initialRegistration.scope === (await page.evaluate(() => navigator.serviceWorker.ready.then((registration) => registration.scope))), 'service worker scope changed')
  assert(context.pages().length === 1, 'update unexpectedly created another page')

  const v2Paths = new Set(['/sw.js', '/manifest.webmanifest', ...v2Manifest.icons.map((icon) => new URL(icon.src, fixtureServer.origin).pathname)])
  for (const pathname of v2Paths) {
    assert(
      fixtureServer.requests.some((request) => request.release === 'v2' && request.pathname === pathname),
      `V2 update did not request ${pathname}`,
    )
  }

  console.log('PWA V1-to-V2 update passed without uninstalling or reinstalling the Chromium PWA.')
} finally {
  await browser?.close()
  await fixtureServer?.close()
  await rm(fixtureRoot, { recursive: true, force: true })
}
