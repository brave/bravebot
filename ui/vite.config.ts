/**
 * Stub for the shadcn CLI, which looks for vite.config.* to detect Vite + Tailwind.
 * electron-vite reads electron.vite.config.ts; this file is not used at build time.
 */
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'node:path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src/renderer'),
    },
  },
})
