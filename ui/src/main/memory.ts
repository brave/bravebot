import { app } from 'electron'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { bot, folderOf, memory, memoryPath } from './bots'
import { replaceProjectMemory } from './project-files'

export interface MemoryRevision { at: number; text: string; source: 'agent' | 'user' }
/** One history per folder, because a bot keeps one memory per folder it works in. */
const historyFile = (slug: string, directory: string) =>
  join(app.getPath('userData'), 'bots', slug, `memory-history-${createHash('sha256').update(directory).digest('hex').slice(0, 16)}.json`)

export function memoryHistory(slug: unknown, directory: unknown): MemoryRevision[] {
  const held = bot(slug)
  const folder = held ? folderOf(held, directory) : null
  if (!held || !folder) return []
  try {
    const value: unknown = JSON.parse(readFileSync(historyFile(held.slug, folder), 'utf8'))
    return Array.isArray(value) ? value.filter((entry): entry is MemoryRevision => entry && typeof entry.text === 'string' && typeof entry.at === 'number' && ['agent', 'user'].includes(entry.source)).slice(-30) : []
  } catch { return [] }
}

export function snapshotMemory(slug: string, directory: string, source: MemoryRevision['source'] = 'agent'): void {
  const text = memory(slug, directory)
  if (text === null) return
  recordMemory(slug, directory, text, source)
}

function recordMemory(slug: string, directory: string, text: string, source: MemoryRevision['source']): void {
  const history = memoryHistory(slug, directory)
  if (history.at(-1)?.text === text) return
  history.push({ at: Date.now(), text, source })
  mkdirSync(join(app.getPath('userData'), 'bots', slug), { recursive: true })
  writeFileSync(historyFile(slug, directory), JSON.stringify(history.slice(-30)), { mode: 0o600 })
}

/**
 * Delete only app-owned copies; never remove the project's memory file or session store.
 *
 * The whole directory rather than the two files in it by name. Everything under it is this app's:
 * the revision history, the cached briefing, and a briefing half-written by a process that died
 * before it could rename one into place. Naming the files left that last one behind, holding the
 * bot's name and purpose after the bot was deleted.
 */
export function removeMemoryHistory(slug: unknown): void {
  const held = bot(slug)
  if (!held) return
  rmSync(join(app.getPath('userData'), 'bots', held.slug), { recursive: true, force: true })
}

/** Compare before replacing, so a memory edit cannot overwrite a concurrent agent update. */
export function editMemory(slug: unknown, directory: unknown, text: unknown, expected: unknown): string {
  const held = bot(slug)
  const folder = held ? folderOf(held, directory) : null
  if (!held || !folder) throw new Error('That bot has not worked in this folder.')
  if (typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > 64 * 1024 || text.includes('\0')) throw new Error('Memory must be text under 64 KB.')
  if (expected !== null && typeof expected !== 'string') throw new Error('Invalid expected memory')
  const previous = replaceProjectMemory(folder, memoryPath(held), text, expected)
  if (previous !== null) recordMemory(held.slug, folder, previous, 'agent')
  recordMemory(held.slug, folder, text, 'user')
  return text
}
