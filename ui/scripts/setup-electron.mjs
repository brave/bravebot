// electron-vite reads path.txt without loading Electron's lazy runtime installer.
// Run the dependency's own idempotent installer before preview/dev need that file.
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

export function setupElectron({ root = fileURLToPath(new URL('../', import.meta.url)), env = process.env } = {}) {
  // Typecheck-only CI deliberately installs the package without its runtime.
  if (env.ELECTRON_SKIP_BINARY_DOWNLOAD) return
  const require = createRequire(join(root, 'package.json'))
  const installer = join(dirname(require.resolve('electron/package.json')), 'install.js')
  const result = spawnSync(process.execPath, [installer], { env, stdio: 'inherit' })
  if (result.error || result.status !== 0) {
    throw new Error('Electron runtime setup failed. Check the download error above, then retry npm run setup:electron.', {
      cause: result.error,
    })
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    setupElectron()
  } catch (error) {
    console.error(error.message)
    process.exitCode = 1
  }
}
