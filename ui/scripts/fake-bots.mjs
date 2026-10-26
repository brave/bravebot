import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

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

/**
 * The agent's `bot.list` and `bot.replace`, kept in memory.
 *
 * `rows` is what the bots file holds, `calls` every request made, `refuse` makes the next
 * replaces fail with that message and `repair` changes each row a replace writes, as the agent does
 * to a model or an avatar it does not accept. `unreadable` and `dropped` are what a list reports, and
 * `oldRowsWaiting` is the list's answer to whether rows are still in `bravebot-ui.json` unmoved
 * (`leaveOut` names list answer fields to omit, as an older agent would). Like the
 * agent, a replace writes the rows in the byte order of the slug, without the `remembered` and `quiet`
 * figures the row no longer holds, and answers with them.
 */
export function fakeBots(initial = []) {
  const state = {
    rows: initial.map(({ remembered, quiet, ...row }) => structuredClone(row)),
    calls: [],
    refuse: null,
    repair: null,
    unreadable: null,
    dropped: [],
    oldRowsWaiting: false,
    leaveOut: [],
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
