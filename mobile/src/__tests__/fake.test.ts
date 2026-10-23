import { FakeClient, FakeSession } from './support/fake'
import { approval, pendingOf, prompt, reply, resolved } from './support/builders'

// The other tests reach each state through this fake, so these hold the rules they rely on.
describe('the fake session', () => {
  it('replaces a row in its place, appends a new one, and takes the next sequence number each time', () => {
    const session = new FakeSession({ id: 's', initial: { status: 'idle', rows: [prompt(1, 1, 'first')] } })
    const seen: number[] = []
    session.subscribe((view) => seen.push(view.sequence))
    const question = approval(2, 1, 'confirm', { request: 1 })
    session.emit({ status: 'waiting', pending: pendingOf(question), rows: [question] })
    session.emit({ status: 'running', pending: null, rows: [resolved(question), reply(3, 1, 'done')] })
    expect(session.view.rows.map((row) => [row.id, row.resolved])).toEqual([[1, false], [2, true], [3, false]])
    expect(session.view.pending).toBeNull()
    expect(seen).toEqual([1, 2])
  })

  it('replaces the view rather than changing a held one', () => {
    const session = new FakeSession({ id: 's', initial: { status: 'idle' } })
    const before = session.view
    session.emit({ status: 'running' })
    expect(before.status).toBe('idle')
    expect(session.view).not.toBe(before)
  })

  it('stops telling a listener that has unsubscribed, and ends the view when told to', () => {
    const session = new FakeSession({ id: 's' })
    const seen: string[] = []
    const stop = session.subscribe((view) => seen.push(view.ended?.reason ?? view.status))
    session.emit({ status: 'running' })
    session.end('connection_lost')
    stop()
    session.emit({ status: 'completed' })
    expect(seen).toEqual(['running', 'connection_lost'])
    expect(session.view.ended?.reason).toBe('connection_lost')
  })

  it('records each call in order and lets a handler change the view', async () => {
    const session = new FakeSession({ id: 's', trust: { directory: '/work' } })
    expect(session.view.status).toBe('awaiting_trust')
    session.on.answerTrust = () => {
      session.emit({ status: 'idle' })
    }
    await session.answerTrust(true)
    await session.send('hello')
    expect(session.methods).toEqual(['answerTrust', 'send'])
    expect(session.view.status).toBe('idle')
  })
})

describe('the fake client', () => {
  it('lists sessions as summaries of their views, newest first, and opens the one asked for', async () => {
    const waiting = approval(2, 1, 'run', { request: 1 })
    const old = new FakeSession({ id: 'old', workspace: 'notes', firstPrompt: 'Run the tests', initial: { status: 'waiting', rows: [waiting], pending: pendingOf(waiting) } })
    const client = new FakeClient({ sessions: [old] })
    const created = (await client.createSession({ workspace: 'work' })) as FakeSession
    expect((await client.listSessions()).map((entry) => entry.id)).toEqual([created.id, 'old'])
    expect((await client.listSessions())[1]).toEqual({ id: 'old', workspace: 'notes', firstPrompt: 'Run the tests', status: 'waiting', waiting: true, ended: false })
    expect(await client.openSession('old')).toBe(old)
    expect(client.opened).toEqual(['old'])
    await expect(client.openSession('missing')).rejects.toThrow('no session missing')
  })
})
