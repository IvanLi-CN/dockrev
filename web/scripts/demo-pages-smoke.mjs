import { createServer } from 'node:http'
import { access, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import { chromium } from 'playwright'

function usage() {
  console.error('Usage: demo-pages-smoke.mjs <pages_dir> [site_base]')
}

const pagesDir = process.argv[2]
const siteBaseInput = process.argv[3] ?? '/'
if (!pagesDir) {
  usage()
  process.exit(1)
}

const absolutePagesDir = path.resolve(pagesDir)
const normalizedSiteBase = (() => {
  const trimmed = siteBaseInput.trim()
  if (!trimmed || trimmed === '/') return '/'
  const withLeadingSlash = trimmed.startsWith('/') ? trimmed : `/${trimmed}`
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`
})()

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

function contentTypeFor(filePath) {
  return contentTypeByExtension.get(path.extname(filePath)) ?? 'application/octet-stream'
}

async function fileExists(filePath) {
  try {
    await access(filePath)
    return true
  } catch {
    return false
  }
}

function safePathname(requestPathname) {
  const decoded = decodeURIComponent(requestPathname)
  const normalized = path.posix.normalize(decoded)
  return normalized.startsWith('/') ? normalized : `/${normalized}`
}

async function resolveStaticFile(requestPathname) {
  const normalizedPath = safePathname(requestPathname)
  const joinedPath = path.join(absolutePagesDir, normalizedPath.replace(/^\/+/, ''))
  const resolvedPath = path.resolve(joinedPath)
  if (!resolvedPath.startsWith(absolutePagesDir)) return null

  if (await fileExists(resolvedPath)) {
    const resolvedStat = await stat(resolvedPath)
    if (resolvedStat.isFile()) return resolvedPath
    if (resolvedStat.isDirectory()) {
      const directoryIndexPath = path.join(resolvedPath, 'index.html')
      if (await fileExists(directoryIndexPath)) {
        return directoryIndexPath
      }
      return null
    }
  }

  const indexPath = path.join(resolvedPath, 'index.html')
  if (await fileExists(indexPath)) {
    return indexPath
  }
  return null
}

async function startStaticServer() {
  const notFoundPath = path.join(absolutePagesDir, '404.html')
  const server = createServer(async (req, res) => {
    try {
      const requestUrl = new URL(req.url ?? '/', 'http://127.0.0.1')
      const requestPathname = (() => {
        if (normalizedSiteBase === '/') return requestUrl.pathname
        if (requestUrl.pathname === normalizedSiteBase.slice(0, -1)) return '/'
        if (requestUrl.pathname.startsWith(normalizedSiteBase)) {
          return `/${requestUrl.pathname.slice(normalizedSiteBase.length)}`.replace(/\/{2,}/g, '/')
        }
        return requestUrl.pathname
      })()
      const filePath = await resolveStaticFile(requestPathname)
      const targetPath = filePath ?? notFoundPath
      const body = await readFile(targetPath)
      res.statusCode = filePath ? 200 : 404
      res.setHeader('Content-Type', contentTypeFor(targetPath))
      res.end(body)
    } catch (error) {
      res.statusCode = 500
      res.setHeader('Content-Type', 'text/plain; charset=utf-8')
      res.end(error instanceof Error ? error.message : 'unexpected error')
    }
  })

  await new Promise((resolve, reject) => {
    server.listen(0, '127.0.0.1', (error) => {
      if (error) reject(error)
      else resolve(undefined)
    })
  })

  const address = server.address()
  if (!address || typeof address === 'string') {
    throw new Error('Failed to determine demo smoke server address')
  }

  return {
    server,
    origin: `http://127.0.0.1:${address.port}`,
  }
}

async function assertDeepLink(page, origin, pathname) {
  await page.goto(`${origin}${pathname}`, { waitUntil: 'networkidle' })
  try {
    await page.waitForFunction(
      (expectedPathname) =>
        window.location.pathname === expectedPathname &&
        Boolean(window.__DOCKREV_MOCK_DEBUG__),
      pathname,
    )
  } catch (error) {
    const state = await page.evaluate(() => ({
      currentPathname: window.location.pathname,
      hasMock: Boolean(window.__DOCKREV_MOCK_DEBUG__),
      title: document.title,
    }))
    throw new Error(
      `Failed to restore demo deep link ${pathname}; currentPathname=${state.currentPathname}; hasMock=${state.hasMock}; title=${state.title}`,
      { cause: error },
    )
  }
  const unexpectedError = await page.evaluate(() => {
    const text = document.body.textContent ?? ''
    return text.includes('unhandled mock route') ? text : null
  })
  if (unexpectedError) {
    throw new Error(`Demo route ${pathname} rendered an unexpected mock error: ${unexpectedError}`)
  }
}

async function runUpdatePersistenceCheck(page, origin, siteBase) {
  const servicePathname = `${siteBase}demo/services/stack-prod/svc-prod-api`
  await page.goto(`${origin}${servicePathname}`, { waitUntil: 'networkidle' })
  const result = await page.evaluate(async () => {
    const stackResponse = await fetch('/api/stacks/stack-prod')
    const stackData = await stackResponse.json()
    const service = stackData.stack.services.find((item) => item.id === 'svc-prod-api')
    if (!service?.candidate) throw new Error('svc-prod-api candidate missing')

    const updateResponse = await fetch('/api/updates', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        scope: 'service',
        serviceId: service.id,
        mode: 'apply',
        targetTag: service.candidate.tag,
        targetDigest: service.candidate.digest,
        pullTags: [service.candidate.tag, `v${service.candidate.tag}`],
      }),
    })
    const updateData = await updateResponse.json()
    return {
      jobId: updateData.jobId,
      targetTag: service.candidate.tag,
    }
  })

  await page.waitForTimeout(2400)
  await page.reload({ waitUntil: 'networkidle' })
  const persisted = await page.evaluate(async (jobId) => {
    const [stackResponse, jobsResponse] = await Promise.all([
      fetch('/api/stacks/stack-prod'),
      fetch('/api/jobs'),
    ])
    const stackData = await stackResponse.json()
    const jobsData = await jobsResponse.json()
    const service = stackData.stack.services.find((item) => item.id === 'svc-prod-api')
    return {
      imageTag: service?.image?.tag ?? null,
      candidate: service?.candidate ?? null,
      jobIds: jobsData.jobs.map((job) => job.id),
      currentPathname: window.location.pathname,
    }
  }, result.jobId)

  if (persisted.currentPathname !== servicePathname) {
    throw new Error(`Expected service detail pathname after reload, got ${persisted.currentPathname}`)
  }
  if (persisted.imageTag !== result.targetTag) {
    throw new Error(`Expected svc-prod-api tag ${result.targetTag}, got ${persisted.imageTag}`)
  }
  if (persisted.candidate !== null) {
    throw new Error('Expected svc-prod-api candidate to settle to null after apply update')
  }
  if (!persisted.jobIds.includes(result.jobId)) {
    throw new Error(`Expected persisted jobs list to include ${result.jobId}`)
  }
}

async function runGhcrPersistenceCheck(page, origin, siteBase) {
  const settingsPathname = `${siteBase}demo/settings/ghcr-webhooks`
  const queuePathname = `${siteBase}demo/queue/ghcr-webhooks`
  await page.goto(`${origin}${settingsPathname}`, { waitUntil: 'networkidle' })
  const result = await page.evaluate(async () => {
    const response = await fetch('/api/github-packages/webhook/sync-all', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    })
    return response.json()
  })

  if (!result?.jobId) {
    throw new Error('Expected GHCR sync-all mock response to return a jobId')
  }

  await page.goto(`${origin}${queuePathname}`, { waitUntil: 'networkidle' })
  await page.reload({ waitUntil: 'networkidle' })
  const persisted = await page.evaluate(async (jobId) => {
    const jobsResponse = await fetch('/api/jobs')
    const jobsData = await jobsResponse.json()
    return {
      currentPathname: window.location.pathname,
      hasJob: jobsData.jobs.some((job) => job.id === jobId),
    }
  }, result.jobId)

  if (persisted.currentPathname !== queuePathname) {
    throw new Error(`Expected GHCR queue pathname after reload, got ${persisted.currentPathname}`)
  }
  if (!persisted.hasJob) {
    throw new Error(`Expected persisted GHCR jobs to include ${result.jobId}`)
  }
}

const { server, origin } = await startStaticServer()
const browser = await chromium.launch({ headless: true })
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })

try {
  for (const pathname of [
    `${normalizedSiteBase}demo/`,
    `${normalizedSiteBase}demo/services`,
    `${normalizedSiteBase}demo/services/stack-prod/svc-prod-api`,
    `${normalizedSiteBase}demo/services/stack-prod/svc-prod-api/history`,
    `${normalizedSiteBase}demo/queue`,
    `${normalizedSiteBase}demo/settings`,
    `${normalizedSiteBase}demo/settings/ghcr-webhooks`,
    `${normalizedSiteBase}demo/cleanup`,
    `${normalizedSiteBase}demo/deploy-check`,
  ]) {
    await assertDeepLink(page, origin, pathname)
  }

  await runUpdatePersistenceCheck(page, origin, normalizedSiteBase)
  await runGhcrPersistenceCheck(page, origin, normalizedSiteBase)
  console.log('[demo-pages-smoke] ok')
} catch (error) {
  const screenshotPath = path.join(absolutePagesDir, 'demo-pages-smoke-failure.png')
  await page.screenshot({ path: screenshotPath, fullPage: true }).catch(() => {})
  throw error
} finally {
  await browser.close()
  await new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) reject(error)
      else resolve(undefined)
    })
  })
}
