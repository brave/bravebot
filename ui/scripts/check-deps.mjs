// Fail in the first second, not after the bridge's cargo build, when node_modules is not
// what pnpm-lock.yaml describes. A fresh clone (or a `git clean -fdx`) has no
// node_modules at all, and `pnpm run build` then compiled the Rust bridge for half a minute
// before dying on "tsc: command not found". A checkout that pulled a lockfile bump without
// reinstalling fails later still, and less legibly.
//
// Missing entirely is unambiguous, so it is fixed here with `pnpm install --frozen-lockfile`.
// Out of date is reported, not repaired: reinstalling rewrites node_modules, and doing that
// unasked to a tree someone may have linked or patched by hand is not this script's call.
import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const unquote = (key) => key.replace(/^'(.*)'$/, '$1').replace(/^"(.*)"$/, '$1')

// Reads the versions pnpm-lock.yaml pins for the root project's direct dependencies. The file
// is machine-written and regular, so a line scan is enough: the root importer's
// `dependencies`, `devDependencies` and `optionalDependencies` hold `name:` then `version:`.
// A version that is a URL (a git dependency) is resolved to the version its `packages:` entry
// records, which is what lands in node_modules/<name>/package.json.
export function lockedVersions(text) {
  const lines = text.split(/\r?\n/)
  const locked = new Map()
  const urls = new Map()
  let area = ''
  let section = ''
  let name = ''
  let entry = ''
  for (const line of lines) {
    if (/^\S/.test(line)) {
      area = line.replace(/:.*$/, '')
      section = ''
      continue
    }
    if (area === 'importers') {
      if (/^ {2}\S/.test(line)) section = line.trim() === '.:' ? '.' : ''
      else if (section === '.') {
        const key = /^ {4}(\w+):$/.exec(line)
        if (key) name = ''
        const dep = /^ {6}(\S.*):$/.exec(line)
        if (dep) name = unquote(dep[1])
        const version = /^ {8}version: (\S+)$/.exec(line)
        if (version && name) locked.set(name, version[1].replace(/\(.*$/, ''))
      }
    } else if (area === 'packages') {
      const key = /^ {2}(\S.*):$/.exec(line)
      if (key) entry = unquote(key[1])
      const version = /^ {4}version: (\S+)$/.exec(line)
      if (version && entry) urls.set(entry, version[1])
    }
  }
  for (const [dep, version] of locked) {
    if (version.includes('://')) locked.set(dep, urls.get(`${dep}@${version}`))
  }
  return locked
}

// Compares the packages package.json names directly against the versions the lockfile
// pins for them. Transitive packages are left to pnpm: platform-specific optional ones are
// absent by design, and a direct dependency out of step is what a stale install looks like.
export function checkDeps({ root = fileURLToPath(new URL('../', import.meta.url)) } = {}) {
  if (!existsSync(join(root, 'node_modules'))) return { missing: true, stale: [] }
  const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  const locked = lockedVersions(readFileSync(join(root, 'pnpm-lock.yaml'), 'utf8'))
  const names = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
  const stale = []
  for (const name of names) {
    const wanted = locked.get(name)
    const installedPath = join(root, 'node_modules', name, 'package.json')
    const installed = existsSync(installedPath)
      ? JSON.parse(readFileSync(installedPath, 'utf8')).version
      : undefined
    if (wanted !== installed) stale.push({ name, wanted, installed })
  }
  return { missing: false, stale }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = fileURLToPath(new URL('../', import.meta.url))
  const { missing, stale } = checkDeps({ root })
  if (missing) {
    console.error('node_modules is missing; running pnpm install --frozen-lockfile first.')
    const args = ['install', '--frozen-lockfile']
    const pnpm = process.env.npm_execpath
      ? [process.execPath, [process.env.npm_execpath, ...args]]
      : ['pnpm', args]
    const result = spawnSync(pnpm[0], pnpm[1], { cwd: root, stdio: 'inherit' })
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
