import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
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
 * `bot.archive`, `bot.restore` and `bot.delete`, kept in memory.
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
