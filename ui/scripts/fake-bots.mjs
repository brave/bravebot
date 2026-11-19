import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { statSync } from 'node:fs'
import { join } from 'node:path'

const require = createRequire(import.meta.url)

/** A bot row with every field the GUI holds, over the fields a test names. */
export const botRow = (over = {}) => ({
  slug: 'web-dev',
  name: 'Web dev',
  purpose: 'Build websites',
  avatar: 'v2:preview',
  model: null,
  home: '/tmp/bot-homes/web-dev',
  definition: null,
  session: null,
  conversations: [],
  archived: 0,
  remembered: 0,
  quiet: 0,
  retired: 0,
  created: 1,
  updated: 1,
  ...over,
})

const inByteOrder = (a, b) => (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0)

const CEILING = 100

const slugOf = (name, taken) => {
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+/, '').slice(0, 48).replace(/-+$/, '') || 'bot'
  if (!taken.has(base)) return base
  for (let n = 2; ; n++) if (!taken.has(`${base}-${n}`)) return `${base}-${n}`
}

const coded = (code, message) => Object.assign(new Error(message), { code })

const isAbsoluteDirectory = (value) => {
  if (typeof value !== 'string' || value.includes('\0')) return false
  if (value.startsWith('/')) return true
  return /^[A-Za-z]:[\\/]/.test(value) && !value.slice(2).includes(':')
}

const text = (field, value) => {
  if (typeof value !== 'string') throw coded('bad_request', `${field} must be a string`)
  if (!value.trim()) throw coded('bad_request', `${field} must not be blank`)
  if (value.includes('\0')) throw coded('bad_request', `${field} must not hold a null character`)
  return value
}

const MODEL_MAX = 512
const AVATAR_MAX = 128
const length = (value) => [...value].length

const model = (value) => {
  if (value === null) return value
  if (typeof value !== 'string') throw coded('bad_request', 'model must be a string or null')
  const one = value.trim() === value && !/[\n\r]/.test(value) && length(value) <= MODEL_MAX && value.toLowerCase() !== 'inherit'
  if (!one || !value || value.includes('\0')) {
    throw coded('bad_request', `the model must be one line of at most ${MODEL_MAX} characters with no space around it, and not \`inherit\``)
  }
  return value
}

const avatar = (value) => {
  if (typeof value !== 'string') throw coded('bad_request', 'avatar must be a string')
  if (!value.trim() || value.includes('\0') || length(value) > AVATAR_MAX) {
    throw coded('bad_request', `the avatar is blank, holds a null character or is longer than ${AVATAR_MAX} characters`)
  }
  return value
}

/**
 * The agent's `bot.list`, `bot.replace`, `bot.make`, `bot.edit`, `bot.set_model`, `bot.ensure_home`,
 * `bot.archive`, `bot.restore`, `bot.delete`, `bot.record_conversation`, `bot.record_archived`,
 * `bot.release_session`, `bot.forget_conversation`, `bot.migrate`, `bot.owes_carry_over` and
 * `bot.works_in`, kept in memory.
 *
 * `rows` is what the bots file holds, `calls` every request made, `refuse` makes the next
 * replaces fail with that message (and the bridge code `refuseCode`, none when it is null, as when the agent is gone) and `repair` changes each row a replace writes, as the agent does
 * to a model or an avatar it does not accept. `unreadable` and `dropped` are what a list reports, and
 * `oldRowsWaiting` is the list's answer to whether rows are still in `bravebot-ui.json` unmoved
 * (`leaveOut` names list answer fields to omit, as an older agent would). Like the
 * agent, a replace writes the rows in the byte order of the slug, without the `remembered` and `quiet`
 * figures the row no longer holds, and answers with them.
 */
export function fakeBots(initial = []) {
  const refuseWrite = () => {
    if (state.refuse) throw state.refuseCode ? coded(state.refuseCode, state.refuse) : new Error(state.refuse)
    if (state.unreadable) throw coded('state_unusable', `${state.unreadable.path}: ${state.unreadable.reason}`)
  }
  const state = {
    rows: initial.map(({ remembered, quiet, ...row }) => structuredClone(row)),
    calls: [],
    refuse: null,
    refuseCode: null,
    repair: null,
    unreadable: null,
    dropped: [],
    oldRowsWaiting: false,
    leaveOut: [],
    homes: [],
    homeRefused: false,
    migrateName: undefined,
    owesAnswer: undefined,
    stamp: 10_000,
    async request(method, params) {
      state.calls.push([method, params])
      if (method === 'bot.list') {
        const answer = {
          bots: structuredClone(state.unreadable ? [] : state.rows),
          dropped: state.dropped,
          unreadable: state.unreadable,
          oldRowsWaiting: state.oldRowsWaiting,
        }
        for (const field of state.leaveOut) delete answer[field]
        return answer
      }
      if (method === 'bot.replace') {
        if (state.refuse) throw new Error(state.refuse)
        state.rows = structuredClone(params.bots).map(({ remembered, quiet, ...row }) => (state.repair ? state.repair(row) : row)).sort(inByteOrder)
        return { bots: structuredClone(state.rows) }
      }
      if (method === 'bot.make' || method === 'bot.edit' || method === 'bot.set_model') {
        refuseWrite()
        const stamp = Date.now()
        let row
        if (method === 'bot.make') {
          const name = text('name', params.name)
          const purpose = text('purpose', params.purpose)
          const chosen = params.model === undefined ? null : model(params.model)
          const drawn = params.avatar === undefined ? null : avatar(params.avatar)
          if (state.rows.length >= CEILING) throw coded('bad_request', `there are already ${CEILING} bots`)
          const slug = slugOf(name, new Set(state.rows.map((each) => each.slug)))
          row = {
            slug, name, purpose, avatar: drawn ?? slug, model: chosen,
            home: join(params.dataFolder, 'bot-homes', slug), definition: slug, session: null, conversations: [],
            archived: 0, retired: 0, created: stamp, updated: stamp,
          }
        } else {
          const held = state.rows.find((each) => each.slug === params.slug)
          if (!held) throw coded('bad_request', `no bot has the slug ${params.slug}`)
          row = { ...held, updated: stamp }
          if (method === 'bot.set_model') {
            if (!('model' in params)) throw coded('bad_request', 'model is required')
            row.model = model(params.model)
          } else {
            if (params.name !== undefined) row.name = text('name', params.name)
            if (params.purpose !== undefined) row.purpose = text('purpose', params.purpose)
            if (params.model !== undefined) row.model = model(params.model)
            if (params.avatar !== undefined) row.avatar = avatar(params.avatar)
          }
        }
        state.rows = [...state.rows.filter((each) => each.slug !== row.slug), state.repair ? state.repair(row) : row].sort(inByteOrder)
        const written = structuredClone(state.rows.find((each) => each.slug === row.slug))
        if (method === 'bot.set_model') return { bot: written }
        state.homes.push(row.slug)
        return { bot: written, homeMade: true }
      }
      if (method === 'bot.ensure_home') {
        if (!state.rows.some((each) => each.slug === params.slug)) throw new Error(`no bot has the slug ${params.slug}`)
        if (state.homeRefused) return { homeMade: false }
        state.homes.push(params.slug)
        return { homeMade: true }
      }
      if (method === 'bot.archive' || method === 'bot.restore') {
        refuseWrite()
        const held = state.rows.find((each) => each.slug === params.slug)
        if (!held) throw coded('bad_request', `no bot has the slug ${params.slug}`)
        state.stamp += 1
        const row = { ...held, retired: method === 'bot.archive' ? state.stamp : 0, updated: state.stamp }
        state.rows = [...state.rows.filter((each) => each.slug !== row.slug), state.repair ? state.repair(row) : row].sort(inByteOrder)
        return { bot: structuredClone(state.rows.find((each) => each.slug === row.slug)) }
      }
      if (method === 'bot.delete') {
        refuseWrite()
        const index = state.rows.findIndex((each) => each.slug === params.slug)
        if (index < 0) throw coded('bad_request', `no bot has the slug ${params.slug}`)
        state.rows.splice(index, 1)
        return { slug: params.slug }
      }
      if (method === 'bot.record_conversation' || method === 'bot.record_archived' || method === 'bot.release_session' || method === 'bot.forget_conversation') {
        const required = (name) => {
          if (typeof params[name] !== 'string') throw coded('bad_request', `\`${name}\` must be a string`)
          return params[name]
        }
        if (method === 'bot.record_archived') {
          if (!('archived' in params)) throw coded('bad_request', '`archived` is required')
          if (typeof params.archived !== 'number') return { bot: null, wrote: false }
        }
        refuseWrite()
        const stampRow = (row) => {
          state.stamp += 1
          const next = { ...row, updated: state.stamp }
          state.rows = [...state.rows.filter((each) => each.slug !== next.slug), state.repair ? state.repair(next) : next].sort(inByteOrder)
          return structuredClone(state.rows.find((each) => each.slug === next.slug))
        }
        if (method === 'bot.forget_conversation') {
          const id = required('id')
          const directory = required('directory')
          let changed = 0
          state.stamp += 1
          const stamp = state.stamp
          const nextRows = state.rows.map((held) => {
            const conversations = held.conversations.filter((each) => !(each.id === id && each.directory === directory))
            const pointed = held.session === id
            if (conversations.length === held.conversations.length && !pointed) return held
            changed += 1
            const next = { ...held, conversations, session: pointed ? null : held.session, archived: pointed ? 0 : held.archived, updated: stamp }
            return state.repair ? state.repair(next) : next
          })
          if (changed === 0) state.stamp -= 1
          else state.rows = nextRows.sort(inByteOrder)
          return { changed, wrote: changed > 0 }
        }
        const slug = required('slug')
        const held = state.rows.find((each) => each.slug === slug)
        if (!held) return { bot: null, wrote: false }
        if (method === 'bot.record_conversation') {
          const id = required('id')
          const directory = required('directory')
          if (id.length === 0 || id.includes('/') || id.includes('\0') || !isAbsoluteDirectory(directory)) {
            throw coded('bad_request', 'the conversation was not recorded: the session is not an identifier, or the directory is not an absolute path')
          }
          const known = held.conversations.some((each) => each.id === id && each.directory === directory)
          return { bot: stampRow({ ...held, session: id, conversations: known ? held.conversations : [...held.conversations, { id, directory }] }), wrote: true }
        }
        if (method === 'bot.record_archived') {
          const archived = params.archived
          const whole = Number.isFinite(archived) && archived === Math.trunc(archived) && archived >= 0 && archived <= Number.MAX_SAFE_INTEGER
          if (!whole || archived <= held.archived) return { bot: structuredClone(held), wrote: false }
          return { bot: stampRow({ ...held, archived }), wrote: true }
        }
        if (held.session === null) return { bot: structuredClone(held), wrote: false }
        return { bot: stampRow({ ...held, session: null, archived: 0 }), wrote: true }
      }
      if (method === 'bot.migrate' || method === 'bot.owes_carry_over' || method === 'bot.works_in') {
        const slug = typeof params.slug === 'string' ? params.slug : ''
        const held = state.rows.find((each) => each.slug === slug)
        if (method === 'bot.owes_carry_over') {
          if (state.owesAnswer !== undefined) return { owes: state.owesAnswer }
          if (!held || held.definition !== null) return { owes: false }
          const old = held.conversations[0]?.directory ?? held.home
          return { owes: params.folder === old }
        }
        if (method === 'bot.works_in') {
          if (!held || typeof params.directory !== 'string' || !isAbsoluteDirectory(params.directory)) return { allowed: false }
          if (params.directory === held.home) {
            let directory = false
            try { directory = statSync(params.directory).isDirectory() } catch { directory = false }
            return { allowed: directory }
          }
          const recorded = held.conversations.some((each) => each.directory === params.directory && isAbsoluteDirectory(each.directory))
          const opened = Array.isArray(params.opened) && params.opened.some((each) => each === params.directory && isAbsoluteDirectory(each))
          return { allowed: recorded || opened }
        }
        if (!held) throw coded('bad_request', `there is no bot ${slug}`)
        if (held.definition !== null) return { bot: structuredClone(held) }
        refuseWrite()
        const name = state.migrateName === undefined ? held.slug : state.migrateName
        const slugName = typeof name === 'string' && name.length <= 64 && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name)
        if (!slugName) throw coded('bad_request', 'the agent named a definition that is not a slug')
        state.stamp += 1
        const row = { ...held, definition: name, updated: state.stamp }
        state.rows = [...state.rows.filter((each) => each.slug !== row.slug), state.repair ? state.repair(row) : row].sort(inByteOrder)
        return { bot: structuredClone(state.rows.find((each) => each.slug === row.slug)) }
      }
      throw new Error(`a method the bots store does not call: ${method}`)
    },
  }
  return state
}

/** A main-process module of the GUI, loaded with the Electron app it names and one set of modules below it. */
export function loadMain(entry, electron) {
  const source = buildSync({
    stdin: { contents: entry, resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}
