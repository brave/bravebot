import assert from 'node:assert/strict'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { after, test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { connectDevRelay, RelayRefusedError, type DevRelay, type RelaySocket } from '../src/dev-relay/index.js'
import { StaleActionError, UnknownSendError, type AgentClient, type AgentSession, type InstanceInfo, type Pending } from '../src/common/index.js'
import { ModelStub, type PlannerStep } from './support/model-stub.js'
import { hostBinary, isolatedEnvironment, until } from './support/rig.js'
import { within } from './support/wait.js'

const here = dirname(fileURLToPath(import.meta.url))
const relay = join(here, '../scripts/dev-relay.js')

interface Launch {
  port: number
  code: string
}

interface Host {
  child: ChildProcessWithoutNullStreams
  stub: ModelStub
  project: string
  stderr(): string
  /** The port and code of the next relay the host starts, in the order they start. */
  launch(): Promise<Launch>
  stop(): Promise<void>
}

const hosts: Host[] = []
after(async () => {
  for (const made of hosts) await made.stop()
})

const LISTENING = /dev-relay: listening on 127\.0\.0\.1 port (\d+); the code for this launch is (\d{6})\n/g

async function startHost(plans: Record<string, PlannerStep[]>): Promise<Host> {
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'dev-relay-')))
  const project = join(scratch, 'project')
  const home = join(scratch, 'home')
  mkdirSync(project)
  mkdirSync(join(home, '.bravebot'), { recursive: true })
  const stub = await ModelStub.start(plans)
  const child = spawn(
    hostBinary(),
    ['--sessions-root', join(scratch, 'sessions'), '--workspace', project, '--', process.execPath, relay, '--workspace', project],
    { env: isolatedEnvironment(home, stub.endpoint), cwd: scratch },
  )
  let text = ''
  let waiters: (() => void)[] = []
  child.stderr.setEncoding('utf8')
  child.stderr.on('data', (chunk: string) => {
    text += chunk
    const woken = waiters
    waiters = []
    for (const wake of woken) wake()
  })
  let taken = 0
  const exited = new Promise<void>((resolve) => child.once('exit', () => resolve()))
  const made: Host = {
    child,
    stub,
    project,
    stderr: () => text,
    launch: () =>
      within(
        new Promise<Launch>((resolve) => {
          const look = () => {
            const found = [...text.matchAll(LISTENING)][taken]
            if (found === undefined) return void waiters.push(look)
            taken++
            resolve({ port: Number(found[1]), code: found[2]! })
          }
          look()
        }),
        'a relay to start listening',
      ),
    async stop() {
      if (child.exitCode === null) {
        child.stdin.end('quit\n')
        await within(exited, 'the host to stop', 15_000).catch(() => child.kill('SIGKILL'))
      }
      await stub.stop()
      rmSync(scratch, { recursive: true, force: true })
    },
  }
  hosts.push(made)
  return made
}

/** Sockets opened by `connect`, so a test can close the app's end itself. */
function sockets(): { open: (url: string) => RelaySocket; opened: RelaySocket[]; urls: string[] } {
  const opened: RelaySocket[] = []
  const urls: string[] = []
  return {
    opened,
    urls,
    open(url) {
      urls.push(url)
      const socket = new WebSocket(url) as unknown as RelaySocket
      opened.push(socket)
      return socket
    },
  }
}

function connect(launch: Launch, open: (url: string) => RelaySocket, code = launch.code): Promise<DevRelay> {
  return connectDevRelay({
    port: launch.port,
    code,
    open,
    deadlines: {
      ms: 30_000,
      schedule(ms, fire) {
        const timer = setTimeout(fire, ms)
        return () => clearTimeout(timer)
      },
    },
  })
}

function otherThan(code: string): string {
  return String((Number(code) + 1) % 1_000_000).padStart(6, '0')
}

function pendingOf(session: AgentSession): Pending {
  assert.ok(session.view.pending)
  return session.view.pending
}

test('an app that gives the code drives the host through the relay: the workspaces are the host\'s and an approved write lands', async () => {
  const host = await startHost({ 'plan:write': [{ tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'from the phone' } } }, { say: 'written' }] })
  const launch = await host.launch()
  const made = sockets()
  const relayed = await connect(launch, made.open)
  assert.deepEqual(made.urls, [`ws://127.0.0.1:${launch.port}`], 'the code is not part of the address')
  assert.deepEqual(relayed.client.workspaces(), [{ id: host.project, name: 'project' }])
  // The relay writes this line to its stderr, which reaches the test through the host's pipe, so it can land after the connection is up.
  for (let waited = 0; !host.stderr().includes('the app gave the code and is connected') && waited < 5_000; waited += 20) {
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
  assert.equal(host.stderr().includes('the app gave the code and is connected'), true)

  const session = await relayed.client.createSession({ workspace: host.project })
  assert.equal(session.view.status, 'awaiting_trust')
  await session.answerTrust(false)
  await until(session, 'trust to be recorded', (view) => view.status === 'idle')
  await session.send('plan:write')
  await until(session, 'the write question', (view) => view.status === 'waiting')
  const question = pendingOf(session)
  assert.equal(question.kind, 'confirm')
  assert.equal(existsSync(join(host.project, 'out.txt')), false, 'nothing is written before an answer')
  await session.decide(question.request, 'approve')
  const done = await until(session, 'the turn to end', (view) => view.status === 'completed')
  assert.equal(readFileSync(join(host.project, 'out.txt'), 'utf8'), 'from the phone')
  const reply = done.rows.at(-1)
  assert.equal(reply?.kind, 'reply')
  assert.equal((reply.data as { reply: string }).reply, 'written')
})

/** The text of each reply row the view holds, in order. */
function replies(session: AgentSession): string[] {
  return session.view.rows.filter((row) => row.kind === 'reply').map((row) => (row.data as { reply: string }).reply)
}

/** The host's listing once no session it holds is working. A turn's worker can still be returning after its view says completed. */
async function settled(client: AgentClient, what: string): Promise<InstanceInfo> {
  const deadline = Date.now() + 10_000
  for (;;) {
    const info = await client.instance()
    if (info.sessions.every((held) => !held.working)) return info
    if (Date.now() > deadline) assert.fail(`timed out waiting for ${what}`)
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
}

test('an app whose connection ended takes its session over through the next relay, holds what it had, and sends again', async () => {
  const host = await startHost({ 'plan:first': [{ say: 'first reply' }], 'plan:second': [{ say: 'second reply' }] })
  const made = sockets()
  const first = await connect(await host.launch(), made.open)
  const session = await first.client.createSession({ workspace: host.project })
  await session.answerTrust(false)
  await until(session, 'trust to be recorded', (view) => view.status === 'idle')
  await session.send('plan:first')
  await until(session, 'the first turn to end', (view) => view.status === 'completed')
  const before = await settled(first.client, 'the first turn\'s worker to return')
  assert.deepEqual(before.sessions, [{ session: session.id, state: 'open', working: false, controlled: true }])

  made.opened[0]!.close(1000, 'the phone lost its network')
  await within(first.ended, 'the client to see its connection end')
  assert.equal(session.view.ended?.reason, 'connection_lost')

  host.child.stdin.write('launch\n')
  const second = await connect(await host.launch(), made.open)
  const after = await second.client.instance()
  assert.equal(after.instance, before.instance, 'the host did not restart')
  assert.deepEqual(after.sessions, [{ session: session.id, state: 'open', working: false, controlled: false }])

  const taken = await second.client.attach(session.id)
  assert.equal(taken.view.status, 'completed')
  assert.equal(taken.view.ended, null)
  assert.deepEqual(replies(taken), ['first reply'], 'the view starts from what the session holds')
  assert.equal((await second.client.instance()).sessions[0]?.controlled, true)

  await taken.send('plan:second')
  await until(taken, 'the second reply', () => replies(taken).length === 2)
  assert.deepEqual(replies(taken), ['first reply', 'second reply'])
  assert.equal(session.view.ended?.reason, 'connection_lost', 'the old session object stays ended')
})

test('a send whose answer was lost with the connection is looked up after taking the session over, and is resent only when the host did not receive it', async () => {
  const host = await startHost({ 'plan:once': [{ say: 'only reply' }], 'plan:again': [{ say: 'resent reply' }] })
  const made = sockets()
  const first = await connect(await host.launch(), (url) => {
    const socket = made.open(url)
    const send = socket.send.bind(socket)
    socket.send = (data) => {
      send(data)
      if (data.includes('"turn.send"')) socket.close(1000, 'the phone lost its network after sending')
    }
    return socket
  })
  const session = await first.client.createSession({ workspace: host.project })
  await session.answerTrust(false)
  await until(session, 'trust to be recorded', (view) => view.status === 'idle')
  const instance = (await first.client.instance()).instance
  const lost = await session.send('plan:once').then(() => assert.fail('the send was answered'), (error: unknown) => error)
  assert.ok(lost instanceof UnknownSendError, `the cut-off send failed with ${String(lost)}`)
  await within(first.ended, 'the client to see its connection end')

  host.child.stdin.write('launch\n')
  const second = await connect(await host.launch(), made.open)
  const listed = await second.client.instance()
  assert.equal(listed.instance, instance, 'the host did not restart, so the session handle is the same session')
  assert.deepEqual(listed.sessions.map((held) => held.session), [session.id])
  const taken = await second.client.attach(session.id)
  const status = await taken.messageStatus(lost.messageId)
  assert.equal(status.state, 'accepted', 'the host received the send whose answer was lost')
  await assert.rejects(taken.resend(lost.messageId, 'plan:once'), StaleActionError)
  await until(taken, 'the reply to the send that reached the host', () => replies(taken).length === 1)
  await settled(second.client, 'the turn\'s worker to return')
  assert.deepEqual(replies(taken), ['only reply'], 'the accepted send ran once')

  assert.deepEqual(await taken.messageStatus('never-sent'), { state: 'not_received' })
  await taken.resend('never-sent', 'plan:again')
  await until(taken, 'the resent reply', () => replies(taken).length === 2)
  assert.deepEqual(replies(taken), ['only reply', 'resent reply'])
  assert.equal((await taken.messageStatus('never-sent')).state, 'accepted')
})

test('a wrong code ends that relay and leaves the host running, and the next launch has a port and code of its own', async () => {
  const host = await startHost({})
  const first = await host.launch()
  const made = sockets()
  await assert.rejects(connect(first, made.open, otherThan(first.code)), (error: unknown) => error instanceof RelayRefusedError && error.wrongCode)
  await within(
    new Promise<void>((resolve) => {
      const look = () => (host.stderr().includes('client ended') ? resolve() : setTimeout(look, 20))
      look()
    }),
    'the host to see the relay end',
  )
  assert.equal(host.child.exitCode, null, 'the host is still running')
  await assert.rejects(connect(first, made.open), (error: unknown) => error instanceof RelayRefusedError && !error.wrongCode, 'the ended relay takes no second try')

  host.child.stdin.write('launch\n')
  const second = await host.launch()
  assert.notEqual(second.port, first.port)
  // One chance in a million that two launches draw the same code.
  assert.notEqual(second.code, first.code)
  const relayed = await connect(second, made.open)
  assert.equal((await relayed.client.describe()).sessionView.version, 1)
})

test('a relay takes one connection: a second app cannot reach it while the first is connected', async () => {
  const host = await startHost({})
  const launch = await host.launch()
  const made = sockets()
  const relayed = await connect(launch, made.open)
  await assert.rejects(connect(launch, made.open), (error: unknown) => error instanceof RelayRefusedError && !error.wrongCode)
  assert.equal((await relayed.client.describe()).sessionView.version, 1, 'the first connection is unaffected')
})

test('the app closing its connection ends the relay and not the host, and the host quitting ends the app\'s connection', async () => {
  const host = await startHost({})
  const made = sockets()
  const first = await connect(await host.launch(), made.open)
  made.opened[0]!.close(1000, 'done')
  assert.match(await within(first.ended, 'the client to see its connection end'), /relay/)
  host.child.stdin.write('launch\n')
  const second = await connect(await host.launch(), made.open)
  assert.equal(host.child.exitCode, null)

  host.child.stdin.write('quit\n')
  // The host stops the relay as soon as the channel ends, often before its close frame is sent,
  // so the reason is the relay's when it arrives and the close code's otherwise.
  assert.match(await within(second.ended, 'the client to see the host end'), /^the relay (ended the connection: the host ended the connection|connection closed \(1006\))$/)
  await assert.rejects(second.client.describe())
})

// A host that quits stops its relay itself, so this host is killed and cannot.
test('the host being killed before any app connects ends the relay, which then takes no connection', async () => {
  const host = await startHost({})
  const launch = await host.launch()
  const exited = new Promise<void>((resolve) => (host.child.exitCode === null ? host.child.once('exit', () => resolve()) : resolve()))
  host.child.kill('SIGKILL')
  await within(exited, 'the host to stop', 10_000)
  await within(
    new Promise<void>((resolve) => {
      const look = () => (host.stderr().includes('the host ended the connection; the relay is ending') ? resolve() : setTimeout(look, 20))
      look()
    }),
    'the relay to end',
    10_000,
  )
  await assert.rejects(connect(launch, sockets().open), (error: unknown) => error instanceof RelayRefusedError && !error.wrongCode)
})

test('a port or code that cannot be right is refused before anything is opened', async () => {
  const made = sockets()
  await assert.rejects(connect({ port: 0, code: '123456' }, made.open), RangeError)
  await assert.rejects(connect({ port: 70000, code: '123456' }, made.open), RangeError)
  await assert.rejects(connect({ port: 4000, code: '12345' }, made.open), RangeError)
  await assert.rejects(connect({ port: 4000, code: '12345a' }, made.open), RangeError)
  assert.deepEqual(made.urls, [])
})
