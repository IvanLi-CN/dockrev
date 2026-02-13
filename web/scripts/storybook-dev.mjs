import { spawn } from 'node:child_process'

const DEFAULT_PORT = 50886

function parsePort(value, fallback) {
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function hasFlag(argv, ...flags) {
  return argv.some((arg) => flags.includes(arg))
}

function hasFlagPrefix(argv, ...prefixes) {
  return argv.some((arg) => prefixes.some((p) => arg === p || arg.startsWith(`${p}=`)))
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
    })
    child.on('exit', (code) => resolve(code ?? 1))
  })
}

async function main() {
  const passthrough = process.argv.slice(2)
  const hasPort = hasPortFlag(passthrough)
  const hasExactPort = hasFlag(passthrough, '--exact-port')
  const hasVersionUpdatesFlag = hasFlagPrefix(passthrough, '--no-version-updates', '--version-updates')
  const port = parsePort(process.env.DOCKREV_STORYBOOK_PORT, DEFAULT_PORT)

  const args = ['dev']
  if (!hasPort) {
    args.push('--port', String(port))
  }
  if (!hasExactPort) {
    args.push('--exact-port')
  }
  // Reduce noise in dev logs; can be overridden via passthrough flags.
  if (!hasVersionUpdatesFlag) {
    args.push('--no-version-updates')
  }
  args.push(...passthrough)

  const code = await run('bun', ['--bun', './node_modules/.bin/storybook', ...args])
  process.exit(code)
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
