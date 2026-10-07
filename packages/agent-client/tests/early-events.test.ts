import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { RpcAgentClient } from '../src/common/index.js'
import { FIXTURES } from './support/scenario.js'

const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as { capability: unknown }

/** A closed worker can still emit events while a new session is being created. */
test('late events from closed sessions do not displace a fresh startup trust question', async () => {
  let client: RpcAgentClient
  let handle = 0
  const closed: string[] = []
  const emit = (event: string, session: string, data: unknown): void => {
    client.receive(JSON.stringify({ event, session, data }) + '\n')
  }
  client = new RpcAgentClient({
    write(line) {
      const { id, method, params } = JSON.parse(line) as { id: number; method: string; params: { session: string } }
      let ok: unknown = {}
      if (method === 'agent.info') ok = { capabilities: { sessionView: contract.capability } }
      else if (method === 'session.new') {
        // Deliver these while creation is in flight, when early events must still be accepted.
        for (const session of closed) emit('turn.error', session, { message: 'cancelled' })
        const session = `s${++handle}`
        emit('trust.request', session, { directory: '/work', keeping: null })
        ok = { session }
      } else if (method === 'session.view.start') {
        emit('session.view.initial', params.session, {
          sequence: 0, turn: 0, status: 'awaiting_trust', pending: null, rows: [],
        })
      } else if (method === 'session.close') {
        emit('session.view.update', params.session, {
          sequence: 1, turn: 0, status: 'detached', pending: null, rows: [],
        })
        closed.push(params.session)
      } else assert.fail(`unexpected method ${method}`)
      client.receive(JSON.stringify({ id, ok }) + '\n')
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })

  for (let at = 0; at < 65; at++) {
    const session = await client.createSession({ workspace: 'work' })
    assert.equal(session.view.status, 'awaiting_trust')
    assert.deepEqual(session.startupTrust, { directory: '/work', keeping: null }, `session ${at + 1}`)
    assert.equal((await session.close()).viewDetached, true)
    emit('turn.error', session.id, { message: 'cancelled' })
  }
})
