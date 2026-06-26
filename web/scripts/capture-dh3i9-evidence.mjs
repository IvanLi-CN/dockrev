import path from 'node:path'
import { mkdir } from 'node:fs/promises'
import { chromium } from 'playwright'

function parseArgs(argv) {
  const out = { url: null, outdir: null }
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i]
    if (a === '--url') {
      out.url = argv[i + 1] ?? null
      i += 1
      continue
    }
    if (a === '--outdir') {
      out.outdir = argv[i + 1] ?? null
      i += 1
    }
  }
  return out
}

function normalizeBaseUrl(input) {
  const url = new URL(input)
  url.search = ''
  url.hash = ''
  if (url.pathname.endsWith('/iframe.html') || url.pathname.endsWith('/index.html')) {
    url.pathname = url.pathname.replace(/[^/]+$/, '')
  }
  if (!url.pathname.endsWith('/')) url.pathname += '/'
  return url.toString()
}

function iframeUrl(baseUrl, storyId) {
  const base = normalizeBaseUrl(baseUrl)
  const url = new URL('iframe.html', base)
  url.searchParams.set('id', storyId)
  url.searchParams.set('viewMode', 'story')
  return url.toString()
}

async function openStory(context, baseUrl, id, viewport = { width: 1440, height: 960 }) {
  const page = await context.newPage()
  await page.setViewportSize(viewport)
  await page.goto(iframeUrl(baseUrl, id), { waitUntil: 'domcontentloaded' })
  await page.waitForFunction(
    () => {
      const root = document.querySelector('#storybook-root, #root')
      return Boolean(root && root.childElementCount > 0)
    },
    null,
    { timeout: 60_000 },
  )
  await page.waitForTimeout(400)
  return page
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  if (!args.url) throw new Error('Missing required --url')

  const repoRoot = path.resolve(process.cwd(), '..')
  const outDir = path.resolve(
    args.outdir ??
      path.join(repoRoot, 'docs/specs/dh3i9-edgeone-origin-timeout-hardening/assets'),
  )
  await mkdir(outDir, { recursive: true })

  const browser = await chromium.launch()
  const context = await browser.newContext({
    viewport: { width: 1440, height: 960 },
    deviceScaleFactor: 2,
  })

  const shots = [
    {
      id: 'pages-cleanuppage--scanning-state',
      file: 'cleanup-scanning-state.png',
      screenshot: async (page, filePath) => {
        const root = page.locator('.cleanupPage')
        await root.waitFor({ timeout: 20_000 })
        await root.screenshot({ path: filePath })
      },
    },
    {
      id: 'pages-deploywelcomepage--cached-report-refreshing',
      file: 'deploy-check-cached-refreshing.png',
      screenshot: async (page, filePath) => {
        const root = page.locator('.deployWelcomeRoot')
        await root.waitFor({ timeout: 20_000 })
        await root.screenshot({ path: filePath })
      },
    },
  ]

  try {
    for (const shot of shots) {
      const page = await openStory(context, args.url, shot.id)
      try {
        const filePath = path.join(outDir, shot.file)
        await shot.screenshot(page, filePath)
        console.log(filePath)
      } finally {
        await page.close().catch(() => {})
      }
    }
  } finally {
    await browser.close().catch(() => {})
  }
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
