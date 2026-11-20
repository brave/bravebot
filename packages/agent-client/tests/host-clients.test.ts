import assert from 'node:assert/strict'
import { execFileSync, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { after, test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { ModelStub, type PlannerStep } from './support/model-stub.js'
import { hostBinary, isolatedEnvironment } from './support/rig.js'
import { Typed } from './support/typed-client.js'
import { within } from './support/wait.js'

const here = dirname(fileURLToPath(import.meta.url))
const hostClient = join(here, '../scripts/host-client.js')

interface Options {
  /** What each client may answer. */
  answers: Record<string, string>
  plans: Record<string, PlannerStep[]>
}

class Host {
  readonly waiters: (() => void)[] = []
  readonly stub!: ModelStub
  readonly project!: string
  private text = ''
  private child!: ChildProcessWithoutNullStreams
  private scratch!: string
  private exited!: Promise<void>
  private readonly clients: Typed[] = []

  static async start(options: Options): Promise<Host> {
    const host = new Host()
    host.scratch = realpathSync(mkdtempSync(join(tmpdir(), 'host-clients-')))
    const project = join(host.scratch, 'project')
    const home = join(host.scratch, 'home')
    mkdirSync(project)
    mkdirSync(join(home, '.bravebot'), { recursive: true })
    const stub = await ModelStub.start(options.plans)
    Object.assign(host, { stub, project })
    const fifos = new Map<string, string>()
    const groups: string[] = []
    for (const [name, kinds] of Object.entries(options.answers)) {
      const fifo = join(host.scratch, `${name}.input`)
      execFileSync('mkfifo', [fifo])
      fifos.set(name, fifo)
      groups.push(
        '--client', name, '--answers', kinds, '--program', process.execPath,
        '--arg', hostClient, '--arg', '--name', '--arg', name, '--arg', '--workspace', '--arg', project, '--arg', '--input', '--arg', fifo,
      )
    }
    host.child = spawn(hostBinary(), ['--sessions-root', join(host.scratch, 'sessions'), '--workspace', project, ...groups], {
      env: isolatedEnvironment(home, stub.endpoint),
      cwd: host.scratch,
    })
    host.child.stderr.setEncoding('utf8')
    host.child.stderr.on('data', (chunk: string) => {
      host.text += chunk
      const woken = host.waiters.splice(0)
      for (const wake of woken) wake()
    })
    host.exited = new Promise<void>((resolve) => host.child.once('exit', () => resolve()))
    // A no to the question about the workspace, so each session asks its clients about trust.
    await within(
      new Promise<void>((resolve) => {
        const look = () => {
          if (host.text.includes('trust this directory? [y/N] ')) {
            host.child.stdin.write('n\n')
            return resolve()
          }
          host.waiters.push(look)
        }
        look()
      }),
      'the host to ask about its workspace',
    )
    for (const [name, fifo] of fifos) host.clients.push(new Typed(name, host, fifo))
    hosts.push(host)
    return host
  }

  /** Resolves once the host has said `text` on its standard error. */
  says(text: string): Promise<void> {
    return within(
      new Promise<void>((resolve) => {
        const look = () => (this.text.includes(text) ? resolve() : void this.waiters.push(look))
        look()
      }),
      `the host to say ${text}`,
    )
  }

  client(name: string): Typed {
    const found = this.clients.find((typed) => typed.name === name)
    assert.ok(found, `no client ${name}`)
    return found
  }

  /** What `name` said, one entry per line, without the prefix. */
  lines(name: string): string[] {
    const prefix = `host-client ${name}: `
    // The last segment is a line still being written, or empty.
    return this.text
      .split('\n')
      .slice(0, -1)
      .filter((line) => line.startsWith(prefix))
      .map((line) => line.slice(prefix.length))
  }

  async stop(): Promise<void> {
    for (const typed of this.clients) {
      try {
        typed.leave()
      } catch {
        // Closed already.
      }
    }
    if (this.child.exitCode === null) {
      this.child.stdin.end('quit\n')
      await within(this.exited, 'the host to stop', 15_000).catch(() => this.child.kill('SIGKILL'))
    }
    await this.stub.stop()
    rmSync(this.scratch, { recursive: true, force: true })
  }
}

const hosts: Host[] = []
after(async () => {
  for (const host of hosts) await host.stop()
})

const ALL = 'confirm,run,fetch,ask,trust'
const WITHOUT_TRUST = 'confirm,run,fetch,ask'
const WRITE: Record<string, PlannerStep[]> = {
  'plan:write': [{ tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'written after approval\n' } } }, { say: 'Wrote out.txt.' }],
  'plan:hold': [{ say: 'held and done', hold: 'turn' }],
}

/** The session id the client opened or joined, from its `session ID status S` line. */
const sessionOf = (line: string): string => /^session (\S+) status/.exec(line)![1]!

test('trust is answered by the client that may, and the client that may not is not asked and is refused', async () => {
  const host = await Host.start({ answers: { desk: ALL, phone: WITHOUT_TRUST }, plans: WRITE })
  const desk = host.client('desk')
  const phone = host.client('phone')

  desk.type('new')
  const opened = await desk.next(/^session \S+ status awaiting_trust/)
  const session = sessionOf(opened)
  await desk.next(/^trust question: /)

  phone.type(`attach ${session}`)
  await phone.next(/^session \S+ status awaiting_trust/)
  phone.type('trust yes')
  await phone.next(/^failed: .*not_allowed/)
  assert.deepEqual(phone.said(/^trust question/), [], 'the client that may not answer trust was asked')
  assert.deepEqual(desk.said(/^trust answered/), [], 'the refusal of the phone answered the question')

  desk.type('no')
  await desk.next(/^trust answered no/)
  await phone.next(/^view \d+ turn 0 status idle actor desk/)
})

test('a question is shown to both clients, the first answer lands, and a later answer from the other client is refused with the file showing the first answer only', async () => {
  const host = await Host.start({ answers: { desk: ALL, phone: ALL }, plans: WRITE })
  const desk = host.client('desk')
  const phone = host.client('phone')

  desk.type('new')
  const session = sessionOf(await desk.next(/^session \S+ status awaiting_trust/))
  await desk.next(/^trust question: /)
  desk.type('no')
  await desk.next(/^trust answered no/)
  phone.type(`attach ${session}`)
  await phone.next(/^session \S+ status idle/)

  desk.type('send plan:write')
  const asked = await desk.next(/^question confirm \d+:/)
  await phone.next(/^question confirm \d+:/)
  const request = /^question confirm (\d+):/.exec(asked)![1]!
  assert.ok(!existsSync(join(host.project, 'out.txt')))
  phone.type('approve')
  await phone.next(/^approve request/)
  // Refused either by the desk's client, whose view may already show the question resolved, or by
  // the host as `no_such_request`. The host's refusal of a second answer is checked in its process
  // tests.
  desk.type(`decide ${request} reject`)
  await desk.next(/^failed: /)
  await desk.next(/status completed actor phone/)
  assert.equal(readFileSync(join(host.project, 'out.txt'), 'utf8'), 'written after approval\n')
  // Each client's view names the client that acted last, the phone for the answer.
  await phone.next(/status completed actor phone/)
})

test('a client leaving keeps the question for the one that stays, and a question the other cannot answer is shown as one to answer elsewhere', async () => {
  const host = await Host.start({ answers: { desk: ALL, phone: 'run' }, plans: WRITE })
  const desk = host.client('desk')
  const phone = host.client('phone')

  desk.type('new')
  const session = sessionOf(await desk.next(/^session \S+ status awaiting_trust/))
  await desk.next(/^trust question: /)
  desk.type('no')
  await desk.next(/^trust answered no/)
  phone.type(`attach ${session}`)
  await phone.next(/^session \S+ status idle/)

  desk.type('send plan:write')
  const asked = await desk.next(/^question confirm \d+:/)
  // The phone may answer runs and not writes, so it is told to answer on another device.
  await phone.next(/^answer on another device: a confirm question/)
  phone.type('quit')
  await phone.next(/^ending/)
  // The host narrows who can answer when it sees the phone's channel end, not when the phone says so.
  await host.says('bravebot-host: client phone ended')
  const request = /^question confirm (\d+):/.exec(asked)![1]!
  assert.ok(request.length > 0)
  desk.type('approve')
  await desk.next(/^approve request/)
  await desk.next(/status completed actor desk/)
  assert.equal(readFileSync(join(host.project, 'out.txt'), 'utf8'), 'written after approval\n')
})

test('a send from the second client while a turn runs is refused as busy, and the first turn ends', async () => {
  const host = await Host.start({ answers: { desk: ALL, phone: ALL }, plans: WRITE })
  const desk = host.client('desk')
  const phone = host.client('phone')

  desk.type('new')
  const session = sessionOf(await desk.next(/^session \S+ status awaiting_trust/))
  await desk.next(/^trust question: /)
  desk.type('no')
  await desk.next(/^trust answered no/)
  phone.type(`attach ${session}`)
  await phone.next(/^session \S+ status idle/)

  desk.type('send plan:hold')
  await host.stub.reached('turn')
  phone.type('send plan:hold again')
  await phone.next(/^failed: .*turn_in_flight/)
  host.stub.release('turn')
  await phone.next(/status completed actor desk/)
  assert.equal(host.stub.requests.length, 1, 'the refused send reached the model')
})
