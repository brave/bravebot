import { app } from 'electron'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'

/** Main-process-only transport to the UI's descriptor-relative file helper. */
function request(root: string, path: string, fields: Record<string, unknown>): Record<string, unknown> {
  const binary = app.isPackaged
    ? join(process.resourcesPath, 'bravebot-ui-files')
    : join(app.getAppPath(), 'target', 'debug', 'bravebot-ui-files')
  const result = spawnSync(binary, [], {
    // macOS's immutable system aliases are the only links normalized here. Resolving an
    // entire project path first would let a swapped project directory redefine the boundary.
    input: JSON.stringify({ root: process.platform === 'darwin' ? root.replace(/^\/(tmp|var)(?=\/|$)/, '/private/$1') : root, path, ...fields }),
    encoding: 'utf8', timeout: 5000, maxBuffer: 2 * 1024 * 1024,
    windowsHide: true,
  })
  if (result.error || result.status !== 0) throw new Error('Secure file access unavailable. Rebuild or reinstall Brave Bot.')
  const response = JSON.parse(result.stdout)
  if (response.error) throw new Error(response.error)
  return response.ok
}

export function readProjectText(root: string, path: string, limit = 128 * 1024): { text: string; truncated: boolean } | null {
  try {
    const value = request(root, path, { operation: 'read', limit })
    return typeof value.text === 'string' && typeof value.truncated === 'boolean'
      ? { text: value.text, truncated: value.truncated } : null
  } catch { return null }
}

export function replaceProjectMemory(root: string, path: string, text: string, expected: string | null): string | null {
  const value = request(root, path, { operation: 'replace', text, expected })
  if (value.previous !== null && typeof value.previous !== 'string') throw new Error('Invalid memory response')
  return value.previous
}

/** Only the main process supplies the agent home; these operations accept one fixed filename. */
export function readAgentHooks(home: string): string | null {
  const value = request(home, 'hooks.json', { operation: 'hooks.read' })
  if (value.text !== null && typeof value.text !== 'string') throw new Error('Invalid hooks response')
  return value.text
}
export function replaceAgentHooks(home: string, text: string, expected: string | null): void {
  request(home, 'hooks.json', { operation: 'hooks.replace', text, expected })
}
