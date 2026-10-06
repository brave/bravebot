// Fail in the first second, not after the bridge's cargo build, when node_modules is not
// what pnpm-lock.yaml describes. A fresh clone (or a `git clean -fdx`) has no
// node_modules at all, and `pnpm run build` then compiled the Rust bridge for half a minute
// before dying on "tsc: command not found". A checkout that pulled a lockfile bump without
// reinstalling fails later still, and less legibly.
//
// Missing entirely is unambiguous, so it is fixed here with a frozen install. Out of date is
// reported, not repaired: an install that re-links node_modules first would replace whatever
// a tree someone may have linked or patched by hand, and doing that unasked is not this
// script's call.
import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

// Compares the packages package.json names directly against the versions the lockfile's
// importer block pins for them. Transitive packages are left to pnpm: platform-specific
// optional ones are absent by design, and a direct dependency out of step is what a stale
// install looks like.
//
// The importer block is plain YAML shallow enough to walk with indentation alone. A version
// that is not a semver range answer -- a git-hosted tarball URL, an `npm:` alias, or a
// version followed by a peer parenthetical -- is only held to existing: the lockfile pins it
// and a frozen install is the gate, so exactness here would only re-implement pnpm.
export function checkDeps({ root = fileURLToPath(new URL('../', import.meta.url)) } = {}) {
  if (!existsSync(join(root, 'node_modules'))) return { missing: true, stale: [] }
  const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  const lock = readFileSync(join(root, 'pnpm-lock.yaml'), 'utf8')
  const pinned = importerVersions(lock)
  const names = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
  const stale = []
  for (const name of names) {
    const wanted = pinned[name]
    const exact = typeof wanted === 'string' && /^\d+\.\d+\.\d+/.test(wanted)
      ? wanted.split('(')[0]
      : undefined
    const installedPath = join(root, 'node_modules', name, 'package.json')
    const installed = existsSync(installedPath)
      ? JSON.parse(readFileSync(installedPath, 'utf8')).version
      : undefined
    if (!wanted && installed === undefined) {
      stale.push({ name, wanted: undefined, installed: undefined })
    } else if (exact !== undefined && exact !== installed) {
      stale.push({ name, wanted: exact, installed })
    } else if (installed === undefined) {
      stale.push({ name, wanted, installed })
    }
  }
  return { missing: false, stale }
}

// The `importers: .:` blocks of a pnpm lockfile: `      name:` then `        version: value`.
// Quoted names (scoped packages, anything starting with @) lose their quotes. pnpm 12 writes
// the lockfile as two documents, the first holding pnpm's own packageManagerDependencies and
// the second the project's importers, and both are read: a `packages:` section ends one walk
// and the next `importers:` line arms the next.
export function importerVersions(lockText) {
  const lines = lockText.split(/\r?\n/)
  const pinned = {}
  let inImporters = false
  let inRoot = false
  let name = null
  for (const line of lines) {
    if (!inImporters) {
      inImporters = line === 'importers:'
      continue
    }
    if (/^ {2}\S/.test(line)) inRoot = line.startsWith('  .:')
    if (!inRoot) continue
    const group = line.match(/^ {6}(.+):\s*$/)
    if (group) name = group[1].replace(/^'|'$/g, '')
    const version = line.match(/^ {8}version: (.+)$/)
    if (name && version) pinned[name] = version[1].trim()
  }
  return pinned
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = fileURLToPath(new URL('../', import.meta.url))
  const { missing, stale } = checkDeps({ root })
  if (missing) {
    console.error('node_modules is missing; running pnpm install --frozen-lockfile first.')
    const result = spawnSync('pnpm', ['install', '--frozen-lockfile'], {
      cwd: root, stdio: 'inherit', shell: process.platform === 'win32',
    })
    if (result.error || result.status !== 0) {
      console.error('pnpm install failed; fix the error above, then build again.')
      process.exitCode = 1
    }
  } else if (stale.length) {
    console.error('node_modules does not match pnpm-lock.yaml:')
    for (const { name, wanted, installed } of stale) {
      console.error(`  ${name}: installed ${installed ?? 'nothing'}, lockfile wants ${wanted ?? 'nothing'}`)
    }
    console.error('Run `pnpm install --frozen-lockfile` in ui/, then build again.')
    process.exitCode = 1
  }
}
