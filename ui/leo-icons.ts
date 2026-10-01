import { cpSync, createReadStream, existsSync, mkdirSync } from 'node:fs'
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
