import { app } from 'electron'
import type { Bot } from '../shared/bots'

export type BotsBridge = (method: string, params: Record<string, unknown>) => Promise<unknown>

interface Listed {
  bots: Omit<Bot, 'remembered' | 'quiet'>[]
  dropped: { index: number; slug: string | null; reason: string }[]
  unreadable: { path: string; reason: string } | null
}

let ask: BotsBridge | null = null
let unread: string | null = null
let unwritten: string | null = null

export function useBotsBridge(next: BotsBridge | null): void {
  ask = next
}

export function botsProblem(): string | null {
  return unwritten ?? unread
}

export function noteBotsFailure(error: unknown): void {
  unwritten = message(error)
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

async function request(method: string, params: Record<string, unknown>): Promise<unknown> {
  if (!ask) throw new Error('The agent is not running, so the bots cannot be reached.')
  return ask(method, { dataFolder: app.getPath('userData'), ...params })
}

const withoutFigures = (row: Listed['bots'][number]): Bot => ({ ...row, remembered: 0, quiet: 0 })

function report(listed: Listed): string | null {
  if (listed.unreadable) return `The bots could not be read from ${listed.unreadable.path}: ${listed.unreadable.reason}`
  if (listed.dropped.length === 0) return null
  return listed.dropped
    .map((each) => `${each.slug === null ? `Entry ${each.index + 1}` : `Bot ${each.slug}`} of the bots file was not read, and the next save removes it: ${each.reason}`)
    .join('\n')
}

async function list(): Promise<Listed> {
  const listed = (await request('bot.list', {})) as Listed
  unread = report(listed)
  return listed
}

// An empty list from `readBots` may be a file that did not parse, so a change is computed from this one, which throws then.
export async function usableBots(): Promise<Bot[]> {
  const listed = await list()
  if (listed.unreadable) throw new Error(report(listed) ?? 'The bots could not be read.')
  return listed.bots.map(withoutFigures)
}

export async function readBots(): Promise<Bot[]> {
  try {
    return (await list()).bots.map(withoutFigures)
  } catch (error) {
    unread = message(error)
    return []
  }
}

let queue: Promise<unknown> = Promise.resolve()

// One change at a time: a change reads the list and replaces it, and two interleaved would lose one.
export function updateBots(change: (bots: Bot[]) => Bot[] | null): Promise<Bot[]> {
  const run = queue.then(async () => {
    const current = await usableBots()
    const next = change(current)
    if (next === null) return current
    try {
      const written = (await request('bot.replace', { bots: next })) as { bots: Listed['bots'] }
      unread = null
      unwritten = null
      return written.bots.map(withoutFigures)
    } catch (error) {
      unwritten = message(error)
      throw error
    }
  })
  queue = run.catch(() => undefined)
  return run
}
