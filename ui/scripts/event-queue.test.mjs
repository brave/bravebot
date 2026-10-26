// How the agent's events reach the window while the main process records what a bot's turn did.
import test from 'node:test'
import assert from 'node:assert/strict'
import { loadMain } from './fake-bots.mjs'

const { eventQueue } = loadMain("export * from './src/main/event-queue'\n", {})

const flush = () => new Promise((resolve) => setImmediate(resolve))

function gate() {
  let open
  const opened = new Promise((resolve) => { open = resolve })
  return { opened, open }
}

function rig(prepare) {
  const log = []
  const reports = []
  const push = eventQueue({
    prepare: (message) => prepare(message, log),
    tell: (message) => log.push(`told ${message.event}@${message.session ?? '-'}`),
    report: (error) => reports.push(error),
  })
  return { log, reports, push }
}

const event = (name, session) => ({ event: name, session, data: {} })

test('an event of another session is not held behind a bot turn that is still being recorded', async () => {
  const slow = gate()
  const { log, push } = rig(async (message) => { if (message.session === 'bot') await slow.opened })
  push(event('turn.done', 'bot'))
  push(event('turn.delta', 'other'))
  await flush()
  assert.deepEqual(log, ['told turn.delta@other'], 'the other session was told while the bot turn waited')
  slow.open()
  await flush()
  assert.deepEqual(log, ['told turn.delta@other', 'told turn.done@bot'])
})

test('a turn that ends is still told to the window when recording it throws, and the failure is reported', async () => {
  const { log, reports, push } = rig(async (message) => { if (message.event === 'turn.done') throw new Error('the bots file was locked') })
  push(event('turn.done', 'bot'))
  push(event('turn.delta', 'bot'))
  await flush()
  assert.deepEqual(log, ['told turn.done@bot', 'told turn.delta@bot'])
  assert.equal(reports.length, 1)
  assert.match(String(reports[0]), /bots file was locked/)
})

test('the events of one session are told in the order they arrived, however long each takes to record', async () => {
  const first = gate()
  const { log, push } = rig(async (message) => { if (message.event === 'turn.done') await first.opened })
  push(event('turn.done', 'bot'))
  push(event('turn.delta', 'bot'))
  await flush()
  assert.deepEqual(log, [], 'a later event does not overtake the one before it')
  first.open()
  await flush()
  assert.deepEqual(log, ['told turn.done@bot', 'told turn.delta@bot'])
})

test('events that name no session are told in the order they arrived', async () => {
  const first = gate()
  const { log, push } = rig(async (message) => { if (message.event === 'watch.first') await first.opened })
  push(event('watch.first'))
  push(event('watch.second'))
  await flush()
  assert.deepEqual(log, [])
  first.open()
  await flush()
  assert.deepEqual(log, ['told watch.first@-', 'told watch.second@-'])
})

test('what the app says about a turn that ends is said after the turn is told', async () => {
  const { log, push } = rig(async (message, log) => (message.event === 'turn.done' ? () => log.push('announced consolidation') : undefined))
  push(event('turn.done', 'bot'))
  await flush()
  assert.deepEqual(log, ['told turn.done@bot', 'announced consolidation'])
})

test('an announcement that throws is reported and does not stop the events after it', async () => {
  const { log, reports, push } = rig(async (message) => (message.event === 'turn.done' ? () => { throw new Error('announcement failed') } : undefined))
  push(event('turn.done', 'bot'))
  push(event('turn.delta', 'bot'))
  await flush()
  assert.deepEqual(log, ['told turn.done@bot', 'told turn.delta@bot'])
  assert.equal(reports.length, 1)
})
