import { defineConfig } from 'electron-vite'
import react from '@vitejs/plugin-react'
import { cpSync, createReadStream, existsSync, mkdirSync } from 'node:fs'
import { resolve } from 'node:path'
import type { Plugin } from 'vite'

/**
 * Serve and ship Leo's SVG icons so `<leo-icon>` mask-images resolve under both
 * Vite's localhost in dev and `file://` in a packaged app. Icons stay in
 * `node_modules/@brave/leo/icons`; nothing is committed under `src/`.
 */
function leoIcons(): Plugin {
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
        if (!file.startsWith(source) || !existsSync(file)) {
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

export default defineConfig({
  main: { build: { rollupOptions: { input: resolve(__dirname, 'src/main/index.ts') } } },
  // Two preloads and two pages: the window somebody types in, and the offscreen one a PDF
  // is printed from. Rollup names each chunk after its key, which is what puts them at the
  // paths `src/main/export.ts` loads them from.
  preload: {
    build: {
      rollupOptions: {
        input: {
          index: resolve(__dirname, 'src/preload/index.ts'),
          export: resolve(__dirname, 'src/preload/export.ts'),
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
        },
      },
    },
    plugins: [react(), leoIcons()],
  },
})
