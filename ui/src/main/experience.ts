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
  if (key === 'density') state.density = value === 'compact' ? 'compact' : 'comfortable'
  else if (key === 'recentModels') state.recentModels = parseExperience({ recentModels: value }).recentModels
  else if (typeof key === 'string' && key.startsWith('[') && key.length <= 10000) {
    state.conversations[key] = parseConversation(value)
  } else throw new Error('Invalid preference')
  // Drafts are work, not disposable preferences. Report write failures and replace atomically.
  writeFileSync(`${file()}.tmp`, JSON.stringify(state), { encoding: 'utf8', mode: 0o600 })
  renameSync(`${file()}.tmp`, file())
  return state
}
