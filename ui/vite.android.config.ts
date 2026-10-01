import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'
import { leoIcons } from './leo-icons'

/**
 * The renderer, built for the Android app's WebView.
 *
 * The same React tree as the desktop window with a different first module: `src/android/main.ts`
 * installs `window.bravebot` over the app's message channel and then runs `src/renderer/main.tsx`.
 * The output lands in the app's assets, where `WebViewAssetLoader` serves it from an https origin.
 */
export default defineConfig({
  root: resolve(__dirname, 'src/android'),
  base: './',
  plugins: [react(), leoIcons()],
  build: {
    outDir: resolve(__dirname, '../android/app/src/main/assets/renderer'),
    emptyOutDir: true,
  },
})
