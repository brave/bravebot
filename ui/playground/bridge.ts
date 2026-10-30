/**
 * A `window.bravebot` that answers from fixtures, so the real renderer runs in a browser tab.
 *
 * The renderer asks for things and listens to a stream of events. This stands where the main
 * process and the agent stand: it keeps the sessions, answers every request the window makes, and
 * plays a scripted turn as events when a prompt is sent. Questions a turn asks wait here for the
 * answer the person gives in the transcript, which is the same path the real agent uses.
 */
import type { BravebotApi, ThemeState } from '../src/preload/index'
import type { Experience } from '../src/shared/experience'
import { EMPTY_CONVERSATION, parseExperience } from '../src/shared/experience'
import type { Appearance } from '../src/shared/theme'
import type { Bot } from '../src/shared/bots'
import type {
  BridgeEvent, EventMap, EventName, ForkedSession, OpenedSession, RewindPoint, RewoundSession, Said, SessionSummary,
} from '../src/shared/protocol'
import {
  BOTS, BOT_HOME, BOT_SAID, DIRECTORY, EVERYTHING_SAID, FORKS, FORK_SAID, IDS, MODELS, PLAN_SAID, REWIND, RULES, SESSIONS,
  SHORT_SAID, SUMMONS, TODOS,
} from './fixtures'
import { PLAN_RUN, QUICK } from './scenarios'
import type { Answered, Kind, Scenario, Script, Want } from './script'

export type Speed = 'instant' | 'fast' | 'normal' | 'slow'
const SPEEDS: Record<Speed, number> = { instant: 0, fast: 0.25, normal: 1, slow: 2.5 }
const CONFIGURED_KEY = 'playground:configured'

/** Whether a backend is configured, as last chosen. Absent means configured. */
export function readConfigured(): boolean {
  return localStorage.getItem(CONFIGURED_KEY) !== 'false'
}

/** What the panel can change and watch. */
export const playground = {
  speed: 'normal' as Speed,
  /** Press the answer buttons for the person, so a whole transcript of decided cards can be drawn. */
  auto: false,
  configured: readConfigured(),
  /** Which scenario a typed prompt gets as its reply. */
  reply: QUICK as Scenario,
  running: null as string | null,
  listeners: new Set<() => void>(),
  changed(): void { for (const listen of this.listeners) listen() },
}

/** Keep the choice and publish it. A reload reads it back through `readConfigured`. */
export function writeConfigured(configured: boolean): void {
  playground.configured = configured
  localStorage.setItem(CONFIGURED_KEY, String(configured))
  playground.changed()
}

class Cancelled extends Error {}

interface Held {
  handle: string
  id: string | null
  directory: string
  title: string
  said: Said[]
  prompts: number
  turn: number
  archived: number
  rewind: RewindPoint[]
  abort: AbortController | null
  waiting: Map<string, (answered: Answered) => void>
}

const held = new Map<string, Held>()
/** Transcripts kept after a session is closed, keyed by conversation id. */
const saved = new Map<string, Said[]>()
const eventListeners = new Set<(event: BridgeEvent) => void>()
const themeListeners = new Set<(state: ThemeState) => void>()
let rows: SessionSummary[] = SESSIONS()
let bots: Bot[] = BOTS()
let experience: Experience = parseExperience(null)
let appearance: Appearance = (localStorage.getItem('playground:theme') as Appearance | null) ?? 'system'
let requests = 100
let created = 0

const sleep = (ms: number, signal?: AbortSignal): Promise<void> => new Promise((resolve, reject) => {
  if (signal?.aborted) return reject(new Cancelled())
  const timer = setTimeout(resolve, ms)
  signal?.addEventListener('abort', () => { clearTimeout(timer); reject(new Cancelled()) }, { once: true })
})

export function emit<N extends EventName>(event: N, data: EventMap[N], session?: string): void {
  const message = { event, data, ...(session ? { session } : {}) } as BridgeEvent
  for (const listen of eventListeners) listen(message)
}

/** The session whose conversation is on screen: the transcript's drop zone carries its handle. */
export function onScreen(): string | null {
  return document.querySelector<HTMLElement>('[data-drop-session]')?.dataset.dropSession ?? null
}

const ok = <T,>(value: T): { ok: T } => ({ ok: value })
const failed = (code: string, message: string): { error: { code: string; message: string } } => ({ error: { code, message } })

function hold(directory: string, id: string | null, said: Said[], extra: Partial<Held> = {}): Held {
  const handle = id ? `s-${id}` : `s-new-${++created}`
  const prompts = said.reduce((most, entry) => entry.kind === 'user' && entry.prompt !== undefined ? Math.max(most, entry.prompt + 1) : most, 0)
  const existing = held.get(handle)
  if (existing) return existing
  const entry: Held = {
    handle, id, directory, title: rows.find((row) => row.id === id)?.title ?? 'New chat', said, prompts, turn: prompts,
    archived: 0, rewind: [], abort: null, waiting: new Map(), ...extra,
  }
  held.set(handle, entry)
  return entry
}

/** Keep the transcript under the conversation id, so a later open reads this copy. */
function remember(session: Held): void {
  if (session.id) saved.set(session.id, structuredClone(session.said))
}

// ---------------------------------------------------------------- pressing the buttons

const enabled = (button: HTMLElement | null): HTMLElement | null =>
  button && !button.hasAttribute('disabled') && button.getAttribute('aria-disabled') !== 'true' ? button : null

/** Scroll the card through the transcript, and each nested scroller through its range, so a deciding row taller than the viewport is measured. */
async function reveal(card: HTMLElement, signal: AbortSignal): Promise<void> {
  const entries = card.closest<HTMLElement>('.entries')
  if (!entries) {
    card.scrollIntoView({ block: 'center' })
    await sleep(32, signal)
    return
  }
  const origin = (): number => card.getBoundingClientRect().top - entries.getBoundingClientRect().top + entries.scrollTop
  const last = Math.max(0, entries.scrollHeight - entries.clientHeight)
  const step = Math.max(40, entries.clientHeight * 0.3)
  const start = Math.max(0, origin() - 24)
  const end = Math.min(last, origin() + card.offsetHeight)
  const down: number[] = []
  for (let at = start; at < end; at += step) down.push(at)
  down.push(end)
  for (const at of [...down].reverse().concat(down)) {
    entries.scrollTo({ top: at })
    await sleep(32, signal)
    const boxes = [...card.querySelectorAll<HTMLElement>('*')].filter((element) => {
      const style = getComputedStyle(element)
      return /auto|scroll/.test(style.overflowY) && element.scrollHeight > element.clientHeight + 1
    })
    for (const element of boxes) {
      const innerLast = Math.max(0, element.scrollHeight - element.clientHeight)
      const innerStep = Math.max(16, Math.min(element.clientHeight, 80) * 0.3)
      for (let top = 0; ; top = Math.min(innerLast, top + innerStep)) {
        element.scrollTo({ top })
        await sleep(32, signal)
        if (top >= innerLast) break
      }
    }
  }
}

/** Scroll the newest waiting card through view and press its button, as a person would. */
async function press(kind: Kind, want: Want, signal: AbortSignal): Promise<void> {
  const deadline = Date.now() + 15_000
  await sleep(450, signal)
  let sweeps = 0
  while (Date.now() < deadline) {
    const waiting = [...document.querySelectorAll<HTMLElement>('.confirm')].filter((card) => card.querySelector('.confirm-actions')).at(-1)
    if (waiting) {
      if (sweeps < 2) {
        await reveal(waiting, signal)
        sweeps += 1
        await sleep(500, signal)
      }
      if (kind === 'ask') {
        const choice = waiting.querySelector<HTMLElement>('.choice')
        choice?.click()
        await sleep(250, signal)
        const next = enabled(waiting.querySelector<HTMLElement>('.approve')) ?? waiting.querySelector<HTMLElement>('.reject')
        next?.click()
        await sleep(500, signal)
        if (waiting.querySelector('.confirm-actions')) continue
        return
      }
      const selector = want === 'reject' ? '.reject' : want === 'always' ? '.approve.always' : '.approve:not(.always)'
      const button = enabled(waiting.querySelector<HTMLElement>(selector)) ?? (want === 'always' ? enabled(waiting.querySelector<HTMLElement>('.approve:not(.always)')) : null)
      if (button) { button.click(); return }
    }
    await sleep(300, signal)
  }
  throw new Error(`No enabled answer for ${kind} was on screen within 15 seconds.`)
}

// ---------------------------------------------------------------- running a scenario

function scriptFor(session: Held, signal: AbortSignal): Script {
  return {
    session: session.handle,
    id: () => ++requests,
    emit: (event, data) => emit(event, data, session.handle),
    wait: (ms) => sleep(ms * SPEEDS[playground.speed], signal),
    hold: () => new Promise<never>((_, reject) => signal.addEventListener('abort', () => reject(new Cancelled()), { once: true })),
    ask: (kind, request, want = 'approve') => new Promise<Answered>((resolve, reject) => {
      const key = `${kind}:${request}`
      signal.addEventListener('abort', () => reject(new Cancelled()), { once: true })
      session.waiting.set(key, resolve)
      if (!playground.auto) return
      void press(kind, want, signal).then(() => undefined, (error: unknown) => {
        if (error instanceof Cancelled) return
        if (session.waiting.get(key) !== resolve) return
        session.waiting.delete(key)
        reject(error instanceof Error ? error : new Error(String(error)))
      })
    }),
    start: () => emit('turn.started', { turn: ++session.turn }, session.handle),
    done: (reply, extra = {}) => {
      const id = session.id ?? crypto.randomUUID()
      if (!session.id) {
        session.id = id
        rows = [{ id, directory: session.directory, project: session.directory.split('/').at(-1) ?? '', branch: 'main', title: session.title, updated: Math.floor(Date.now() / 1000), bytes: 2000 }, ...rows]
      }
      if (extra.rewind) session.rewind = [{ steps: 1, turn: session.turn, prompt: session.prompts - 1, text: session.title, paths: ['src/cache.ts', 'tests/cache.test.ts'], gaps: [] }, ...session.rewind.map((point) => ({ ...point, steps: point.steps + 1 }))]
      session.said.push({ kind: 'assistant', text: reply })
      remember(session)
      emit('turn.done', {
        id, turn: session.turn, prompt: session.prompts - 1, reply, model: 'sample/fast', service: 'Sample gateway', sessionModel: true, steps: extra.steps ?? 4, clean: true,
        tokens: 18_342, outputTokens: 1_204, contextTokens: 64_000, notices: extra.notices ?? [], trust: { rules: [] }, archived: session.archived,
        rewind: session.rewind, ...(extra.consolidating ? { consolidating: true } : {}),
      }, session.handle)
    },
    fail: (kind, message, extra = {}) => emit('turn.error', {
      kind, message, turn: session.turn, prompt: session.prompts - 1, id: session.id, contextTokens: 64_000, rewind: session.rewind, ...extra,
    }, session.handle),
  }
}

/** Play a scenario in a session. One turn per session at a time, as the agent has it. */
export function play(handle: string, title: string, scenario: Scenario): boolean {
  const session = held.get(handle)
  if (!session || session.abort) return false
  const controller = new AbortController()
  session.abort = controller
  playground.running = title
  playground.changed()
  const script = scriptFor(session, controller.signal)
  void scenario(script).catch((error: unknown) => {
    if (error instanceof Cancelled) script.fail('cancelled', 'Stopped by you.')
    else script.fail('chat', error instanceof Error ? error.message : String(error))
  }).finally(() => {
    session.abort = null
    session.waiting.clear()
    playground.running = null
    playground.changed()
  })
  return true
}

export const playing = (handle: string): boolean => held.get(handle)?.abort != null

/** Stop whatever turn the session is running. */
export function stop(handle: string): void { held.get(handle)?.abort?.abort() }

// ---------------------------------------------------------------- answering requests

const REPLIES = new Set(['confirm', 'run', 'output', 'vouch', 'vet', 'fetch', 'server', 'manifest', 'exposure', 'mcp-server', 'mcp-tools', 'mcp-call', 'mcp-move', 'host'])

function opened(session: Held, record: SessionSummary | undefined): OpenedSession {
  const everything = session.id === IDS.everything
  return {
    session: session.handle, model: 'sample/fast', said: session.said, context: 'trusted', contextTokens: everything ? 64_000 : 8_000,
    record: {
      id: session.id ?? '', directory: session.directory, branch: record?.branch ?? 'main', title: session.title, started: Math.floor(Date.now() / 1000) - 86400,
      updated: record?.updated ?? 0, turns: session.prompts, tokens: 48_000, build: 'playground', front: 'desktop',
    },
    todos: everything ? { main: TODOS } : {},
    trust: { known: true, rules: [{ path: '', integrity: 'trusted' }] }, shownDirectory: session.directory,
    branchNote: null, buildNote: null,
    frontNote: session.id === IDS.long ? 'The terminal wrote this conversation, so some of its tool lines are drawn without their outcomes.' : null,
    archived: session.archived, rewind: everything ? REWIND : [],
    autoVetting: everything, permissionMode: 'ask', sandboxMode: 'workspace-write', settingsRules: everything ? RULES : null,
    remembered: everything ? { at: Math.floor(Date.now() / 1000) - 7 * 86400, path: DIRECTORY, shownPath: DIRECTORY } : null,
  }
}

function saidFor(id: string): Said[] {
  switch (id) {
    case IDS.everything: return EVERYTHING_SAID
    case IDS.fork: return FORK_SAID
    case IDS.plan: return PLAN_SAID
    case IDS.bot: return BOT_SAID
    default: return SHORT_SAID
  }
}

async function request(method: string, params: Record<string, unknown> = {}): Promise<{ ok?: unknown; error?: { code: string; message: string } }> {
  const session = typeof params.session === 'string' ? held.get(params.session) : undefined
  const prefix = method.replace(/\.reply$/, '')
  if (method.endsWith('.reply') && (REPLIES.has(prefix) || prefix === 'ask')) {
    const kind = prefix as Kind
    const resolve = session?.waiting.get(`${kind}:${params.request}`)
    if (!resolve) return failed('nothing-waiting', 'Nothing is waiting on that answer.')
    session?.waiting.delete(`${kind}:${params.request}`)
    const answered: Answered = { decision: params.decision === 'reject' ? 'reject' : 'approve', remember: params.remember === true, answers: params.answers as Answered['answers'] }
    // The renderer records the decision after this request resolves, so the scenario continues
    // on a later turn and its next event sees the answer.
    setTimeout(() => resolve(answered), 0)
    return ok({ accepted: true })
  }
  switch (method) {
    case 'agent.info': return ok({ configured: playground.configured, build: 'playground (0.19.0)', version: '1' })
    case 'language.get': return ok({ inUse: 'en', saved: null, available: [{ tag: 'en', name: 'English' }, { tag: 'es', name: 'Español' }], overridden: false })
    case 'language.set': return ok({ saved: true, tag: params.tag })
    case 'update.status': return ok({ check: 'on', running: '0.19.0', newer: false, checked: null })
    case 'session.list': return ok({ sessions: rows })
    case 'models.list': return ok(MODELS)
    case 'mentions.named': return ok({ files: [] })
    case 'mentions.offer': {
      const line = String(params.line ?? '')
      const word = line.split(/\s/).at(-1) ?? ''
      if (!word.startsWith('@')) return ok({ typed: null, word: null, entries: [], completes: false })
      const typed = word.slice(1)
      const entries = ['src/cache.ts', 'src/migrate.ts', 'tests/cache.test.ts', 'docs/RELEASING.md'].filter((path) => path.includes(typed)).map((path) => ({ name: path, directory: false }))
      return ok({ typed, word, entries, completes: false })
    }
    case 'session.open': {
      const id = String(params.id)
      const record = rows.find((row) => row.id === id)
      if (!record) return failed('not-found', 'That conversation is not in this playground.')
      const said = saved.get(id) ?? saidFor(id)
      return ok(opened(hold(String(params.directory), id, structuredClone(said)), record))
    }
    case 'session.new': {
      const directory = String(params.directory)
      const made = hold(directory, null, [])
      return ok({
        session: made.handle, directory, branch: 'main', model: 'sample/fast', shownDirectory: directory, keeping: `${directory}/.bravebot/trust.json`,
        shownKeeping: `${directory}/.bravebot/trust.json`, confinedLines: 'It also lets commands you approve run with this folder as their working directory.',
        autoVetting: false, permissionMode: 'ask', sandboxMode: 'workspace-write', settingsRules: null,
      })
    }
    case 'trust.reply': return ok({ trusted: params.trusted === true, kept: params.trusted === true })
    case 'session.close':
      if (session) { remember(session); session.abort?.abort(); held.delete(session.handle) }
      return ok({})
    case 'session.mode': return ok({ permissionMode: params.mode })
    case 'session.sandbox': return ok({ sandboxMode: params.mode })
    case 'turn.send': {
      if (!session) return failed('no-session', 'That session is closed.')
      session.prompts++
      session.said.push({ kind: 'user', text: String(params.prompt), prompt: session.prompts - 1 })
      if (session.title === 'New chat') session.title = String(params.prompt).slice(0, 70)
      requestAnimationFrame(() => play(session.handle, 'Reply', routeFor(String(params.prompt))))
      return ok({ turn: session.turn + 1 })
    }
    case 'turn.interject': {
      const prompt = String(params.prompt ?? '')
      if (session) setTimeout(() => emit('turn.interjected', { prompt }, session.handle), 1400 * SPEEDS[playground.speed])
      return ok({ accepted: true })
    }
    case 'turn.cancel': if (session) stop(session.handle); return ok({})
    case 'session.fork': {
      if (!session) return failed('no-session', 'That session is closed.')
      const at = session.said.findIndex((entry) => entry.kind === 'user' && entry.prompt === params.prompt)
      const said = structuredClone(session.said.slice(0, Math.max(at, 0)))
      const id = crypto.randomUUID()
      const child = hold(session.directory, id, said, { title: session.title })
      rows = [{ id, directory: session.directory, project: session.directory.split('/').at(-1) ?? '', branch: 'main', title: session.title, updated: Math.floor(Date.now() / 1000), bytes: 1000 }, ...rows]
      const forked: ForkedSession = {
        session: child.handle, id, directory: child.directory, branch: 'main', said, prefill: String(params.text ?? ''), context: 'trusted', turns: child.prompts, todos: {},
        trust: { known: true, rules: [] }, shownDirectory: child.directory, parent: { id: session.id ?? '', directory: session.directory, title: session.title, prompt: Number(params.prompt) },
        autoVetting: false, permissionMode: 'ask', sandboxMode: 'workspace-write',
      }
      return ok(forked)
    }
    case 'session.rewind': {
      if (!session) return failed('no-session', 'That session is closed.')
      const point = (session.id === IDS.everything && !session.rewind.length ? REWIND : session.rewind).find((each) => each.steps === params.steps)
      if (!point) return failed('no-point', 'There is nothing to put back to.')
      const at = session.said.findIndex((entry) => entry.kind === 'user' && entry.prompt === point.prompt)
      session.said = session.said.slice(0, Math.max(at, 0))
      const rewound: RewoundSession = {
        session: session.handle, turn: point.turn, text: point.text, refused: [], gaps: point.gaps, said: session.said, context: 'trusted', archived: session.archived, todos: {},
        trust: { rules: [] }, rewind: [],
      }
      return ok(rewound)
    }
    case 'session.delete':
      rows = rows.filter((row) => row.id !== params.id)
      if (typeof params.id === 'string') saved.delete(params.id)
      return ok({})
    case 'session.purge': return ok({ sessions: 0, bytes: 0, ids: [], skipped: [] })
    case 'session.import': return ok({ found: [], copied: [] })
    case 'permissions.list': return ok({
      paths: [{ path: '', shownPath: '""', integrity: 'trusted' }, { path: 'vendor', shownPath: 'vendor', integrity: 'untrusted' }],
      commands: [{ program: '/usr/bin/git', args: ['status'], display: 'git status' }],
      remembered: {
        at: Math.floor(Date.now() / 1000) - 86400,
        path: '/Users/example/Library/Application Support/bravebot-ui-b/trusted',
        shownPath: '/Users/example/Library/Application Support/bravebot-ui-b/trusted',
        root: '/Users/example/Library/Application Support/bravebot-ui-b',
        shownRoot: '/Users/example/Library/Application Support/bravebot-ui-b',
      },
    })
    case 'permissions.revoke': return ok({ paths: [], commands: [] })
    case 'watches.list': return ok({ busy: false, watches: [{ number: 1, path: 'src/cache.ts', state: 'watching', remainingSeconds: 86_400, armedBy: null }] })
    case 'watches.add': case 'watches.stop': return ok({ busy: false, watches: [] })
    case 'connectors.list': return ok({ connectors: [] })
    case 'settings.inspect': return ok({
      build: '0.19.0 (playground)', configured: playground.configured, problem: playground.configured ? null : 'No model service is configured.', model: 'sample/fast', brave: false, bedrock: false,
      providers: [], selected: null, layers: [], overrides: [], managed: { path: null, keys: [] },
      network: { roots: [], problem: null, trustsNothing: false, proxy: null, authenticated: false, unusableProxy: null, noProxy: null },
    })
    case 'hooks.inspect': return ok({ path: '~/.bravebot/hooks.json', text: null, entire: true, timeoutSeconds: { min: 1, max: 600, defaultFor: {} }, hooks: [] })
    case 'doctor': return ok({ found: true, text: playground.configured ? 'Everything the playground needs is here.' : 'Backend not configured. Install a configured build or rebuild with the approved environment.' })
    case 'about': return ok({ version: '0.19.0', build: 'playground', home: '/Users/example/.bravebot' })
    case 'manifest.run': {
      if (!session) return failed('no-session', 'That session is closed.')
      play(session.handle, 'Plan run', PLAN_RUN)
      return ok({ run: 1 })
    }
    default: return ok({})
  }
}

/** A prompt that says `play:<id>` gets that scenario as its reply, so one conversation can show anything. */
function routeFor(prompt: string): Scenario {
  const named = /play:([a-z0-9-]+)/.exec(prompt.toLowerCase())?.[1]
  return (named ? ROUTES.get(named) : undefined) ?? playground.reply
}
const ROUTES = new Map<string, Scenario>()
export const route = (id: string, scenario: Scenario): void => { ROUTES.set(id, scenario) }

// ---------------------------------------------------------------- the api

const bridge: BravebotApi = {
  platform: 'linux',
  readExperience: async () => experience,
  writeExperience: async (key, value) => {
    if (key === 'dismissedUpdate') experience = { ...experience, dismissedUpdate: typeof value === 'string' ? value : null }
    else if (key === 'recentModels') experience = { ...experience, recentModels: Array.isArray(value) ? value as string[] : [] }
    else if (key.startsWith('[')) experience = { ...experience, conversations: { ...experience.conversations, [key]: { ...EMPTY_CONVERSATION, ...experience.conversations[key], ...(value as object) } } }
    return experience
  },
  request: request as BravebotApi['request'],
  readLayout: async () => { try { return JSON.parse(localStorage.getItem('playground:layout') ?? 'null') } catch { return null } },
  writeLayout: (layout) => localStorage.setItem('playground:layout', JSON.stringify(layout)),
  readView: async () => { try { return JSON.parse(localStorage.getItem('playground:view') ?? 'null') ?? { grouped: true, collapsed: [], tab: 'sessions' } } catch { return { grouped: true, collapsed: [], tab: 'sessions' } } },
  writeView: (view) => localStorage.setItem('playground:view', JSON.stringify(view)),
  readTheme: async () => ({ chosen: appearance }),
  writeTheme: (name) => { appearance = name; localStorage.setItem('playground:theme', name); for (const listen of themeListeners) listen({ chosen: name }) },
  onThemeChanged: (listener) => { themeListeners.add(listener); return () => themeListeners.delete(listener) },
  relaunch: async () => false,
  caffeinateConfirmed: async () => false,
  confirmCaffeinate: async () => undefined,
  reportFault: () => undefined,
  reloadWindow: () => location.reload(),
  readRecents: async () => [DIRECTORY, '/Users/example/tide-tables', '/Users/example/a-very-long-project-name-that-needs-truncation'],
  readForks: async () => FORKS(),
  readBots: async () => bots,
  writeBotModel: async (slug, model) => { bots = bots.map((bot) => bot.slug === slug ? { ...bot, model } : bot); return bots.find((bot) => bot.slug === slug) ?? null },
  readBotServers: async () => ({ servers: null, tools: true, inline: false, kind: 'default', editable: true, every: true, everyPossible: true }),
  writeBotServers: async () => ({ servers: null, tools: true, inline: false, kind: 'default', editable: true, every: true, everyPossible: true }),
  writeBot: async (form) => {
    const slug = form.slug ?? (form.name ?? 'bot').toLowerCase().replace(/[^a-z0-9]+/g, '-')
    const bot: Bot = { ...(bots.find((each) => each.slug === slug) ?? { slug, avatar: form.avatar ?? slug, home: `${BOT_HOME}-${slug}`, definition: slug, session: null, conversations: [], lastHome: null, archived: 0, remembered: 0, quiet: 0, retired: 0, created: Date.now(), name: '', purpose: '', model: null, updated: 0 }), ...(form.name ? { name: form.name } : {}), ...(form.purpose ? { purpose: form.purpose } : {}), ...(form.model !== undefined ? { model: form.model } : {}), updated: Date.now() }
    bots = [...bots.filter((each) => each.slug !== slug), bot]
    return bot
  },
  retireBot: async (slug, retired) => { bots = bots.map((bot) => bot.slug === slug ? { ...bot, retired: retired ? Date.now() : 0 } : bot); return bots.find((bot) => bot.slug === slug) ?? null },
  removeBot: async (slug) => { bots = bots.filter((bot) => bot.slug !== slug); return slug },
  readBotMemory: async () => '# Preferences\n\n- Keep reviews concise.\n- Split a lock change from a migration.\n- Ask before touching `config/`.\n',
  readMemoryHistory: async () => [{ at: Date.now() - 86400_000, text: '- Keep reviews concise.\n', source: 'agent' }],
  editBotMemory: async (_slug, _directory, text) => text,
  releaseBotSession: async () => undefined,
  sendBotTurn: async (turn) => {
    const session = held.get(turn.session)
    if (session) { session.prompts++; session.said.push({ kind: 'user', text: turn.prompt, prompt: session.prompts - 1 }); requestAnimationFrame(() => play(session.handle, 'Bot reply', routeFor(turn.prompt))) }
    return ok({ turn: 1 })
  },
  summonBot: async (turn) => {
    const session = held.get(turn.session)
    if (session) { session.prompts++; session.said.push({ kind: 'user', text: turn.prompt, prompt: session.prompts - 1 }); requestAnimationFrame(() => play(session.handle, 'Summoned reply', routeFor(turn.prompt))) }
    return ok({ turn: 1 })
  },
  readSummons: async () => SUMMONS(),
  listFiles: async (_session, path) => ({
    path,
    rows: path === ''
      ? [{ name: 'src', kind: 'directory', hidden: false }, { name: 'tests', kind: 'directory', hidden: false }, { name: '.env', kind: 'file', hidden: true }, { name: 'package.json', kind: 'file', hidden: false }, { name: 'README.md', kind: 'file', hidden: false }]
      : [{ name: 'cache.ts', kind: 'file', hidden: false }, { name: 'migrate.ts', kind: 'file', hidden: false }],
    truncated: false,
  }),
  previewFile: async (_session, path) => ({ path, text: 'export class Cache {\n  private readLock = new RwLock()\n  private writeLock = new Mutex()\n}\n', truncated: false }),
  chooseAttachments: async () => ['src/cache.ts', 'tests/cache.test.ts', 'docs/a-deliberately-long-file-name-that-has-to-truncate.md'].map((path, index) => ({ id: `attachment-${index}`, path })),
  searchFiles: async () => ({ paths: ['src/cache.ts', 'src/migrate.ts'], incomplete: false }),
  openFile: async () => ({ status: 'opened' }),
  selectSettings: async () => null,
  saveHooks: async () => undefined,
  chooseDirectory: async () => DIRECTORY,
  exportSession: async () => ({ status: 'saved', where: '/Users/example/Desktop/conversation.md' }),
  composeBugReport: async () => ({ available: true, text: 'Bravebot playground report\n\nNothing here is real.', log: null }),
  saveBugReport: async () => ({ status: 'saved', where: '/Users/example/Desktop/report.txt' }),
  showBugReportLog: async () => false,
  openPage: async () => true,
  onCommand: () => () => undefined,
  popupContext: () => undefined,
  publishState: () => undefined,
  onBotConsolidation: () => () => undefined,
  onDrop: () => () => undefined,
  onPaste: () => () => undefined,
  onWindowActive: () => () => undefined,
  onEvent: (listener) => { eventListeners.add(listener); return () => eventListeners.delete(listener) },
}

export function install(): void {
  window.bravebot = bridge
  const first = rows.find((row) => row.id === IDS.everything)
  if (first) first.updated = Math.floor(Date.now() / 1000) - 120
}
