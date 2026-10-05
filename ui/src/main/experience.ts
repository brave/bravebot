import { app } from 'electron'
import { readFileSync, writeFileSync, renameSync } from 'node:fs'
import { join } from 'node:path'
import { parseConversation, parseExperience, type Experience } from '../shared/experience'

const file = () => join(app.getPath('userData'), 'experience.json')
export function readExperience(): Experience {
  try { return parseExperience(JSON.parse(readFileSync(file(), 'utf8'))) }
  catch { return parseExperience(null) }
}
export function writeExperience(key: unknown, value: unknown): Experience {
  const state = readExperience()
  if (key === 'recentModels') state.recentModels = parseExperience({ recentModels: value }).recentModels
  else if (typeof key === 'string' && key.startsWith('[') && key.length <= 10000) {
    state.conversations[key] = parseConversation(value)
  } else throw new Error('Invalid preference')
  save(state)
  return state
}

/** Drop what was kept for a conversation that no longer exists. */
export function removeConversation(key: string): void {
  const state = readExperience()
  if (!(key in state.conversations)) return
  delete state.conversations[key]
  save(state)
}

function save(state: Experience): void {
  // Drafts are work, not disposable preferences. Report write failures and replace atomically.
  writeFileSync(`${file()}.tmp`, JSON.stringify(state), { encoding: 'utf8', mode: 0o600 })
  renameSync(`${file()}.tmp`, file())
}
