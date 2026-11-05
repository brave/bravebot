import { defineConfig } from 'electron-vite'
import react from '@vitejs/plugin-react'
import { copyFileSync, cpSync, createReadStream, existsSync, mkdirSync } from 'node:fs'
import { resolve, sep } from 'node:path'
import type { Plugin } from 'vite'

/**
 * Serve and ship Leo's SVG icons so `<leo-icon>` mask-images resolve under both
 * Vite's localhost in dev and `file://` in a packaged app. Icons stay in
 * `node_modules/@brave/leo/icons`; nothing is committed under `src/`.
 */
export function leoIcons(): Plugin {
  const source = resolve(__dirname, 'node_modules/@brave/leo/icons')

  return {
    name: 'leo-icons',
    configureServer(server) {
      server.middlewares.use('/nala-icons', (req, res, next) => {
        const name = (req.url ?? '').replace(/^\//, '').replace(/\?.*$/, '')
        if (!name || name.includes('..') || !name.endsWith('.svg')) {
          next()
          return
        }
        const file = resolve(source, name)
        if (!file.startsWith(source + sep) || !existsSync(file)) {
          next()
          return
        }
        res.setHeader('Content-Type', 'image/svg+xml')
        res.setHeader('Cache-Control', 'public, max-age=86400')
        createReadStream(file).pipe(res)
      })
    },
    writeBundle(options) {
      if (!options.dir || !existsSync(source)) return
      const dest = resolve(options.dir, 'nala-icons')
      mkdirSync(dest, { recursive: true })
      cpSync(source, dest, { recursive: true })
    },
  }
}

/**
 * Serve and ship the application's own icon, `build/icon.svg`, as `app-icon.svg` beside the page,
 * so the renderer names it with a plain relative URL. The file the app and installer icons are
 * rendered from stays the one copy; nothing is committed under `src/`, and nothing in the
 * renderer imports it, which keeps it out of the esbuild bundles the tests make.
 */
export function appIcon(): Plugin {
  const source = resolve(__dirname, 'build/icon.svg')

  return {
    name: 'app-icon',
    configureServer(server) {
      server.middlewares.use('/app-icon.svg', (_req, res) => {
        res.setHeader('Content-Type', 'image/svg+xml')
        createReadStream(source).pipe(res)
      })
    },
    writeBundle(options) {
      if (!options.dir) return
      copyFileSync(source, resolve(options.dir, 'app-icon.svg'))
    },
  }
}

export default defineConfig({
  main: { build: { rollupOptions: { input: resolve(__dirname, 'src/main/index.ts') } } },
  // Four preloads and four pages: the window somebody types in, the offscreen one a PDF is
  // printed from, the one a sign-in's URL or code is shown in, and the one a key is typed in.
  // Rollup names each chunk after its key, which is what puts them at the paths
  // `src/main/export.ts` loads them from.
  preload: {
    build: {
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'src/preload/index.ts'),
          export: resolve(__dirname, 'src/preload/export.ts'),
          'sign-in': resolve(__dirname, 'src/preload/sign-in.ts'),
          'key-window': resolve(__dirname, 'src/preload/key-window.ts'),
        },
      },
    },
  },
  renderer: {
    root: resolve(__dirname, 'src/renderer'),
    build: {
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'src/renderer/index.html'),
          export: resolve(__dirname, 'src/renderer/export.html'),
          'sign-in': resolve(__dirname, 'src/renderer/sign-in.html'),
          'key-window': resolve(__dirname, 'src/renderer/key-window.html'),
        },
      },
    },
    plugins: [react(), leoIcons(), appIcon()],
  },
})
