import { app } from 'electron'
import { mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { bot, memory, memoryPath } from './bots'
import { replaceProjectMemory } from './project-files'

export interface MemoryRevision { at: number; text: string; source: 'agent' | 'user' }
const historyFile = (slug: string) => join(app.getPath('userData'), 'bots', slug, 'memory-history.json')

export function memoryHistory(slug: unknown): MemoryRevision[] {
  const held = bot(slug)
  if (!held) return []
  try {
    const value: unknown = JSON.parse(readFileSync(historyFile(held.slug), 'utf8'))
    return Array.isArray(value) ? value.filter((entry): entry is MemoryRevision => entry && typeof entry.text === 'string' && typeof entry.at === 'number' && ['agent', 'user'].includes(entry.source)).slice(-30) : []
  } catch { return [] }
}

export function snapshotMemory(slug: string, source: MemoryRevision['source'] = 'agent'): void {
  const held = bot(slug)
  if (!held) return
  const text = memory(slug)
  if (text === null) return
  recordMemory(slug, text, source)
}

function recordMemory(slug: string, text: string, source: MemoryRevision['source']): void {
  const history = memoryHistory(slug)
  if (history.at(-1)?.text === text) return
  history.push({ at: Date.now(), text, source })
  mkdirSync(join(app.getPath('userData'), 'bots', slug), { recursive: true })
  writeFileSync(historyFile(slug), JSON.stringify(history.slice(-30)), { mode: 0o600 })
}

/** Delete only app-owned copies; never remove the project's memory file or session store. */
export function removeMemoryHistory(slug: unknown): void {
  const held = bot(slug)
  if (!held) return
  rmSync(historyFile(held.slug), { force: true })
  rmSync(join(app.getPath('userData'), 'bots', held.slug, 'ground.md'), { force: true })
}

/** Compare before replacing, so a memory edit cannot overwrite a concurrent agent update. */
export function editMemory(slug: unknown, text: unknown, expected: unknown): string {
  const held = bot(slug)
  if (!held || typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > 64 * 1024 || text.includes('\0')) throw new Error('Memory must be text under 64 KB.')
  if (expected !== null && typeof expected !== 'string') throw new Error('Invalid expected memory')
  const previous = replaceProjectMemory(held.directory, memoryPath(held.slug), text, expected)
  if (previous !== null) recordMemory(held.slug, previous, 'agent')
  recordMemory(held.slug, text, 'user')
  return text
}
