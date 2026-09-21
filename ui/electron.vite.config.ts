import { defineConfig } from 'electron-vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'node:path'

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
    plugins: [react()],
  },
})
