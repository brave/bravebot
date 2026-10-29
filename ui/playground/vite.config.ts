import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import { leoIcons } from '../electron.vite.config'

/**
 * The playground: the real renderer in a browser tab, against a bridge that answers from
 * fixtures. Nothing here is part of the app's build, so a packaged app carries none of it.
 */
export default defineConfig({
  root: resolve(__dirname),
  plugins: [react(), leoIcons()],
  resolve: { alias: { '@renderer': resolve(__dirname, '../src/renderer') } },
  server: { port: 5199, strictPort: true, fs: { allow: [resolve(__dirname, '..')] } },
})
