// Fail in the first second, not after the bridge's cargo build, when node_modules is not
// what package-lock.json describes. A fresh clone (or a `git clean -fdx`) has no
// node_modules at all, and `npm run build` then compiled the Rust bridge for half a minute
// before dying on "tsc: command not found". A checkout that pulled a lockfile bump without
// reinstalling fails later still, and less legibly.
//
// Missing entirely is unambiguous, so it is fixed here with `npm ci`. Out of date is
// reported, not repaired: `npm ci` deletes node_modules first, and doing that unasked to a
// tree someone may have linked or patched by hand is not this script's call.
import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

// Compares the packages package.json names directly against the versions the lockfile
// pins for them. Transitive packages are left to npm: platform-specific optional ones are
// absent by design, and a direct dependency out of step is what a stale install looks like.
export function checkDeps({ root = fileURLToPath(new URL('../', import.meta.url)) } = {}) {
  if (!existsSync(join(root, 'node_modules'))) return { missing: true, stale: [] }
  const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  const lock = JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8'))
  const names = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
  const stale = []
  for (const name of names) {
    const wanted = lock.packages?.[`node_modules/${name}`]?.version
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
    console.error('node_modules is missing; running npm ci first.')
    const npm = process.env.npm_execpath
      ? [process.execPath, [process.env.npm_execpath, 'ci']]
      : ['npm', ['ci']]
    const result = spawnSync(npm[0], npm[1], { cwd: root, stdio: 'inherit' })
    if (result.error || result.status !== 0) {
      console.error('npm ci failed; fix the error above, then build again.')
      process.exitCode = 1
    }
  } else if (stale.length) {
    console.error('node_modules does not match package-lock.json:')
    for (const { name, wanted, installed } of stale) {
      console.error(`  ${name}: installed ${installed ?? 'nothing'}, lockfile wants ${wanted ?? 'nothing'}`)
    }
    console.error('Run `npm ci` in ui/, then build again.')
    process.exitCode = 1
  }
}
