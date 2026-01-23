import { spawn } from 'node:child_process'

const DEFAULT_PORT = 50886

function parsePort(value, fallback) {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function hasFlag(argv, ...flags) {
  return argv.some((arg) => flags.includes(arg))
}

function hasPortFlag(argv) {
  return argv.some(
    (arg) =>
      arg === '--port' ||
      arg === '-p' ||
      arg.startsWith('--port=') ||
      arg.startsWith('-p=') ||
      /^-p\d+/.test(arg)
  )
}

function run(command, args) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      shell: process.platform === 'win32',
    })
    child.on('exit', (code) => resolve(code ?? 1))
  })
}

async function main() {
  const passthrough = process.argv.slice(2)
  const hasPort = hasPortFlag(passthrough)
  const hasExactPort = hasFlag(passthrough, '--exact-port')
  const port = parsePort(process.env.DOCKREV_STORYBOOK_PORT, DEFAULT_PORT)

  const args = ['dev']
  if (!hasPort) {
    args.push('--port', String(port))
  }
  if (!hasExactPort) {
    args.push('--exact-port')
  }
  args.push(...passthrough)

  const code = await run('storybook', args)
  process.exit(code)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
