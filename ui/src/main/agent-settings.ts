import { mkdirSync, lstatSync } from 'node:fs'
import { join } from 'node:path'
import { parseHooks, type HooksDocument } from '../shared/agent-settings'
import { readAgentHooks, replaceAgentHooks } from './project-files'

function homeExists(home: string): boolean {
  try {
    const stat = lstatSync(home)
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('The agent state directory must be a real directory.')
    return true
  } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false; throw error }
}
export function readHooks(home: string): HooksDocument {
  const text = homeExists(home) ? readAgentHooks(home) : null
  return { path: join(home, 'hooks.json'), text, hooks: text === null ? [] : parseHooks(text) }
}
export function saveHooks(home: string, text: string, expected: string | null): HooksDocument {
  parseHooks(text)
  mkdirSync(home, { recursive: true, mode: 0o700 })
  homeExists(home)
  if (readHooks(home).text !== expected) throw new Error('Hooks changed on disk. Reload before saving to avoid overwriting another edit.')
  // Pins each directory and atomically replaces the fixed leaf, refusing symlinks and stale text.
  replaceAgentHooks(home, text, expected)
  return readHooks(home)
}
