import { createServer } from 'node:http'
import { access, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'

const port = Number(process.env.DEMO_E2E_PORT ?? 50899)
const distDir = path.resolve(process.env.DEMO_E2E_DIST ?? 'dist-demo')

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
  ['.woff2', 'font/woff2'],
])

async function existingFile(requestPathname) {
  const candidate = path.join(distDir, path.posix.normalize(decodeURIComponent(requestPathname)))
  if (!candidate.startsWith(distDir)) return null
  try {
    await access(candidate)
    const info = await stat(candidate)
    return info.isFile() ? candidate : null
  } catch {
    return null
  }
}

// Mirrors the published EdgeOne rewrite contract in `edgeone.json`.
const edgeone = JSON.parse(await readFile(path.join(distDir, 'edgeone.json'), 'utf8'))
const rewrites = (edgeone.rewrites ?? []).map((rule) => ({
  exact: !rule.source.endsWith('*'),
  prefix: rule.source.replace(/\*$/, ''),
  destination: rule.destination,
}))

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? '/', 'http://127.0.0.1')
  let filePath = await existingFile(url.pathname)
  if (!filePath) {
    const rule = rewrites.find((candidate) =>
      candidate.exact ? url.pathname === candidate.prefix : url.pathname.startsWith(candidate.prefix),
    )
    if (rule) filePath = await existingFile(rule.destination)
  }
  if (!filePath && url.pathname.endsWith('/')) filePath = await existingFile(`${url.pathname}index.html`)
  if (!filePath) {
    res.statusCode = 404
    res.end('not found')
    return
  }
  res.setHeader('Content-Type', contentTypeByExtension.get(path.extname(filePath)) ?? 'application/octet-stream')
  res.end(await readFile(filePath))
})

server.listen(port, '127.0.0.1', () => {
  console.log(`[demo-e2e-server] serving ${distDir} on http://127.0.0.1:${port}`)
})
