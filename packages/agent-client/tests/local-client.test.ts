import assert from 'node:assert/strict'
import { execFile, spawn } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { after, test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'
import { startWebsite } from './support/model-stub.js'
import { FIXTURES } from './support/scenario.js'
import { isolatedEnvironment, rpcBinary, startRig, type Rig } from './support/rig.js'

const run = promisify(execFile)
const program = join(dirname(fileURLToPath(import.meta.url)), '../scripts/local-client.js')

const rigs: Rig[] = []
after(async () => {
  for (const made of rigs) await made.stop()
})

async function rig(plans: Parameters<typeof startRig>[0]): Promise<Rig> {
  const made = await startRig(plans)
  rigs.push(made)
  return made
}

async function drive(made: Rig, args: string[]) {
  return run(process.execPath, [program, '--rpc', rpcBinary(), '--directory', made.project, ...args], {
    env: isolatedEnvironment(made.home, made.stub.endpoint),
  }).then(
    (done) => ({ code: 0, out: done.stdout, err: done.stderr }),
    (failed: { code: number; stdout: string; stderr: string }) => ({ code: failed.code, out: failed.stdout, err: failed.stderr }),
  )
}

const write = [{ tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'from the local client' } } }, { say: 'finished' }]

test('the local program approves or rejects a write as told, and closes without claiming more than a detached view', async () => {
  const made = await rig({ 'plan:approve': write, 'plan:reject': write })
  const rejected = await drive(made, ['--trust', 'no', '--decide', 'reject', 'plan:reject'])
  assert.equal(rejected.code, 0, rejected.err)
  assert.match(rejected.out, /reject confirm request \d+/)
  assert.equal(existsSync(join(made.project, 'out.txt')), false)
  const approved = await drive(made, ['--trust', 'no', '--decide', 'approve', 'plan:approve'])
  assert.equal(approved.code, 0, approved.err)
  assert.match(approved.out, /approve confirm request \d+/)
  assert.equal(readFileSync(join(made.project, 'out.txt'), 'utf8'), 'from the local client')
  assert.match(approved.out, /closed: view detached true, worker unknown, saved unknown/)
})

test('the local program refuses to start without an explicit trust answer, before it starts anything', async () => {
  const refused = await run(process.execPath, [program, '--rpc', '/nonexistent', '--directory', '/nonexistent', 'plan:none']).then(
    () => ({ code: 0, err: '' }),
    (failed: { code: number; stderr: string }) => ({ code: failed.code, err: failed.stderr }),
  )
  assert.equal(refused.code, 2)
  assert.match(refused.err, /usage/)
})

test('without a terminal and without --decide, an approval is rejected', async () => {
  const made = await rig({ 'plan:default': write })
  const result = await drive(made, ['--trust', 'no', 'plan:default'])
  assert.equal(result.code, 0, result.err)
  assert.match(result.out, /reject confirm request/)
  assert.equal(existsSync(join(made.project, 'out.txt')), false)
})

test('the program prints released text with control characters escaped, so it cannot redraw the terminal', async () => {
  const site = await startWebsite('\u001b[2J\u001b[Hforged chrome‮ \u009b31m \u2028 \u200b \u{e0041} \u2062 \nreleased line\n')
  after(() => site.stop())
  const made = await rig({
    'plan:escape': [{ tool: { name: 'fetch_url', arguments: { url: `${site.origin}/page` } } }, { say: 'done' }],
  })
  const result = await drive(made, ['--trust', 'no', '--decide', 'approve', 'plan:escape'])
  assert.equal(result.code, 0, result.err)
  assert.deepEqual(site.requests, ['GET /page'])
  assert.equal(result.out.includes('\u001b'), false, 'a raw escape reached the terminal')
  for (const raw of ['‮', '\u009b', '\u2028', '\u200b']) {
    assert.equal(result.out.includes(raw), false, `U+${raw.charCodeAt(0).toString(16)} reached the terminal raw`)
  }
  for (const escaped of ['\\u202e', '\\u009b', '\\u2028', '\\u200b', '\\udb40\\udc41']) assert.ok(result.out.includes(escaped), escaped)
  assert.ok(result.out.includes('\\u001b[2J'), 'the released text is in the dump, escaped')
  // The escapes are JSON: every payload parses, and the page text comes back as it was served.
  const payloads = result.out.split('\n').filter((line) => line.startsWith('row ')).map((line) => JSON.parse(line.slice(line.indexOf(' payload ') + 9)))
  assert.ok(payloads.some((payload) => JSON.stringify(payload).includes(JSON.stringify('\u{e0041}').slice(1, -1))), 'the tag character survives a round trip')
})

test('the program declines a user question and the planner receives no answer', async () => {
  const made = await rig({
    'plan:ask': [
      { tool: { name: 'ask_user', arguments: { questions: [{ header: 'Approach', question: 'Which approach?', options: [{ label: 'Alpha' }, { label: 'Beta' }] }] } } },
      { say: 'done' },
    ],
  })
  const result = await drive(made, ['--trust', 'yes', 'plan:ask'])
  assert.equal(result.code, 0, result.err)
  assert.match(result.out, /declining the question/)
  assert.equal(made.stub.requests.length, 2, 'the planner was asked once more after the question')
})

test('the program escapes what the bridge says about itself and its errors, not only released payloads', async () => {
  const runtime = join(FIXTURES, 'escape-runtime.mjs')
  const result = await run(process.execPath, [program, '--rpc', runtime, '--directory', '/nonexistent', '--trust', 'no', 'plan:none']).then(
    () => ({ code: 0, text: '' }),
    (failed: { code: number; stdout: string; stderr: string }) => ({ code: failed.code, text: failed.stdout + failed.stderr }),
  )
  assert.equal(result.code, 1)
  for (const raw of ['\u001b', '\u009b', '‮', '\u{e0041}', '⁢']) {
    assert.equal(result.text.includes(raw), false, `U+${raw.codePointAt(0)!.toString(16)} reached the terminal raw`)
  }
  for (const escaped of ['\\u001b[31m', '\\u009b31m', '\\u202e', '\\udb40\\udc41', '\\u2062']) assert.ok(result.text.includes(escaped), escaped)
})

test('the program delivers all of its output to a slow reader before it exits', async () => {
  const reply = 'x'.repeat(1_000_000)
  const made = await rig({ 'plan:big': [{ say: reply }] })
  const child = spawn(process.execPath, [program, '--rpc', rpcBinary(), '--directory', made.project, '--trust', 'yes', 'plan:big'], {
    env: isolatedEnvironment(made.home, made.stub.endpoint),
    stdio: ['ignore', 'pipe', 'inherit'],
  })
  const exited = new Promise<number | null>((resolve) => child.once('close', (code) => resolve(code)))
  // The pipe holds 64 KiB; nothing reads it until the program has had every chance to finish writing.
  await new Promise((resolve) => setTimeout(resolve, 1500))
  const chunks: Buffer[] = []
  for await (const chunk of child.stdout) chunks.push(chunk as Buffer)
  assert.equal(await exited, 0)
  const out = Buffer.concat(chunks).toString('utf8')
  assert.ok(out.length > reply.length, `only ${out.length} characters arrived`)
  assert.match(out.trimEnd().split('\n').at(-1)!, /^closed: view detached true/)
})

// A terminal for the program: the python pty module gives it a real one, which node alone cannot.
const underPty = String.raw`
import os, pty, select, sys, time
argv = sys.argv[1:]
pid, fd = pty.fork()
if pid == 0:
    os.execvp(argv[0], argv)
out, sent, deadline = b"", False, time.time() + 40
while time.time() < deadline:
    ready, _, _ = select.select([fd], [], [], 0.2)
    if ready:
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        out += chunk
        if not sent and b"approve? [y/N]" in out:
            os.write(fd, b"\x04")  # Ctrl-D: end of input at the prompt
            sent = True
_, status = os.waitpid(pid, 0)
sys.stdout.buffer.write(out)
sys.exit(os.waitstatus_to_exitcode(status))
`

test('Ctrl-D at the approval prompt rejects the question and the turn finishes', { skip: process.platform === 'win32' }, async () => {
  const made = await rig({ 'plan:eof': write })
  const result = await run('python3', ['-c', underPty, process.execPath, program, '--rpc', rpcBinary(), '--directory', made.project, '--trust', 'no', 'plan:eof'], {
    env: { ...isolatedEnvironment(made.home, made.stub.endpoint), PATH: process.env.PATH ?? '' },
    timeout: 60_000,
  }).then(
    (done) => ({ code: 0, out: done.stdout }),
    (failed: { code: number; stdout: string }) => ({ code: failed.code, out: failed.stdout }),
  )
  assert.equal(result.code, 0, result.out)
  assert.match(result.out, /reject confirm request/)
  assert.equal(existsSync(join(made.project, 'out.txt')), false, 'a write nobody approved must not land')
})

/** Run the program against the refusing bridge stand-in. */
async function againstRefusingBridge(extra: NodeJS.ProcessEnv, args: string[] = []) {
  const started = Date.now()
  const result = await run(
    process.execPath,
    [program, '--rpc', join(FIXTURES, 'refusing-runtime.mjs'), '--directory', '/nonexistent', '--trust', 'no', '--decide', 'approve', ...args, 'go'],
    { env: { ...process.env, ...extra }, timeout: 30_000 },
  ).then(
    (done) => ({ code: 0, out: done.stdout + done.stderr }),
    (failed: { code: number; stdout: string; stderr: string }) => ({ code: failed.code, out: failed.stdout + failed.stderr }),
  )
  return { ...result, seconds: (Date.now() - started) / 1000 }
}

test('when a reply is refused the program cancels the turn and ends it instead of waiting for a question nobody will answer', async () => {
  const result = await againstRefusingBridge({})
  assert.equal(result.code, 1, result.out)
  assert.match(result.out, /could not answer: the bridge could not apply the reply/)
  assert.match(result.out, /turn ended cancelled/)
  assert.doesNotMatch(result.out, /abandoned/)
})

test('when the bridge does not end the turn either, the program gives up after its grace period', async () => {
  const result = await againstRefusingBridge({ IGNORE_CANCEL: '1' }, ['--grace', '1'])
  assert.equal(result.code, 1, result.out)
  assert.match(result.out, /turn abandoned/)
  assert.ok(result.seconds < 20, `took ${result.seconds} seconds`)
})
