/**
 * The address of the development server that serves the renderer pages, or null where the pages
 * are loaded from the app's own files.
 *
 * `ELECTRON_RENDERER_URL` is the build tool's development setting and comes from the person's
 * environment, so a packaged app can be started with it set. The page it names would be loaded
 * into a window that holds the preload, and the main-process handlers accept a call from any page
 * the main frame holds. A packaged app therefore never reads it.
 */
export function rendererUrl(app: { isPackaged: boolean }, env: NodeJS.ProcessEnv = process.env): string | null {
  if (app.isPackaged) return null
  return env.ELECTRON_RENDERER_URL || null
}
