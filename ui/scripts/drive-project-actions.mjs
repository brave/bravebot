// A project heading's actions menu against the real agent: deleting every chat of a project, and
// copying its Claude Code sessions in.
//
// Real Electron renderer and real `bravebot-rpc`, on a disposable HOME and profile. Before the app
// starts, the bridge is driven directly to take turns against a stub model service, so the store
// holds real records, and Claude Code transcripts are written where Claude Code keeps them. One
// record is entered in the background roster with its lock held by a child process, as a running
// background session's is, and one has a directory where its trail should be, so deleting it
// fails. Screenshots go to PROJECT_ACTIONS_OUTPUT, or a temp directory the run prints.
//
//   pnpm run drive:project-actions
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { launchWorld, makeWorld, startModelService } from './drive-world.mjs'

const output = process.env.PROJECT_ACTIONS_OUTPUT || mkdtempSync(join(tmpdir(), 'bravebot-project-actions-'))
mkdirSync(output, { recursive: true })
const { service } = await startModelService(() => ({ content: 'done' }))
const world = makeWorld('bravebot-project-actions-world-', { service, project: false, dirs: ['alpha', 'beta'] })
const alpha = join(world.root, 'alpha')
const beta = join(world.root, 'beta')
const MARKUP = '<img src=x onerror="window.injected=1"> **bold**'

/** The bridge on the world's HOME, for seeding the store before the app starts. */
function bridge() {
  const child = spawn(resolve('../target/debug/bravebot-rpc'), [], { env: world.env, stdio: ['pipe', 'pipe', 'ignore'] })
  const pending = new Map()
  const events = []
  const waiters = []
  let asked = 0
  createInterface({ input: child.stdout }).on('line', (line) => {
    const message = JSON.parse(line)
    if (message.id !== undefined) pending.get(message.id)?.(message)
    else {
      events.push(message)
      for (const waiter of [...waiters]) waiter()
    }
  })
  const call = (method, params) => new Promise((done, failed) => {
    const id = ++asked
    const timer = setTimeout(() => failed(new Error(`${method} was never answered`)), 30000)
    pending.set(id, (message) => {
      clearTimeout(timer)
      if (message.error) failed(new Error(`${method}: ${message.error.code} ${message.error.message}`))
      else done(message.ok)
    })
    child.stdin.write(`${JSON.stringify({ id, method, params })}\n`)
  })
  const event = (wanted) => new Promise((done, failed) => {
    const timer = setTimeout(() => failed(new Error('the turn never ended')), 30000)
    const look = () => {
      const at = events.findIndex(wanted)
      if (at < 0) return
      waiters.splice(waiters.indexOf(look), 1)
      clearTimeout(timer)
      done(events.splice(at, 1)[0])
    }
    waiters.push(look)
    look()
  })
  return { call, event, close: () => child.kill() }
}

/** One answered turn in a new session of `directory`; the session's id. */
async function takeTurn(rpc, directory, prompt) {
  const { session } = await rpc.call('session.new', { directory })
  // Asked once a folder; trusting it is remembered for the next session there.
  await rpc.call('trust.reply', { session, trusted: true }).catch((error) => {
    if (!/no_such_request/.test(error.message)) throw error
  })
  await rpc.call('turn.send', { session, prompt })
  const ended = await rpc.event((message) => message.session === session && (message.event === 'turn.done' || message.event === 'turn.error'))
  assert.equal(ended.event, 'turn.done', JSON.stringify(ended))
  await rpc.call('session.close', { session })
  return ended.data.id
}

/** A Claude Code transcript of `directory`, where Claude Code keeps it. */
function claudeSession(directory, sourceId, prompt) {
  const key = directory.replace(/[^A-Za-z0-9]/g, '-')
  const folder = join(world.home, '.claude/projects', key)
  mkdirSync(folder, { recursive: true })
  const lines = [
    { type: 'user', cwd: directory, timestamp: '2026-10-06T12:00:01.000Z', message: { role: 'user', content: prompt } },
    { type: 'assistant', cwd: directory, timestamp: '2026-10-06T12:00:02.000Z', message: { role: 'assistant', content: [{ type: 'text', text: 'Done.' }] } },
  ]
  writeFileSync(join(folder, `${sourceId}.jsonl`), lines.map((line) => `${JSON.stringify(line)}\n`).join(''))
}

/** The record files the store keeps for `id`, wherever its project's directory is. */
function recordOf(id) {
  const stores = join(world.home, '.bravebot/sessions')
  for (const store of readdirSync(stores)) if (existsSync(join(stores, store, `${id}.json`))) return join(stores, store, id)
  return null
}

const rpc = bridge()
const doomed = await takeTurn(rpc, alpha, 'first alpha question')
const stuck = await takeTurn(rpc, alpha, 'stuck alpha question')
const busy = await takeTurn(rpc, alpha, 'background alpha question')
const theirs = await takeTurn(rpc, beta, 'the beta question')
claudeSession(alpha, '4b0c5a6e-1111-2222-3333-000000000001', 'fix the parser')
claudeSession(alpha, '4b0c5a6e-1111-2222-3333-000000000002', MARKUP)
await rpc.call('session.import', { directory: alpha, ids: ['4b0c5a6e-1111-2222-3333-000000000001'] })
rpc.close()

// Deleting the stuck record fails at its trail, which is a directory with something in it.
const trail = `${recordOf(stuck)}.audit.jsonl`
unlinkSync(trail)
mkdirSync(join(trail, 'inside'), { recursive: true })

// The busy record is running in the background: a roster entry, and its lock held by a child.
const entry = join(world.home, '.bravebot/jobs', busy)
mkdirSync(entry, { recursive: true })
writeFileSync(join(entry, 'state.json'), JSON.stringify({ id: busy, directory: alpha, checkout: null, name: 'background alpha question',
  prompt: 'background alpha question', state: 'idle', held: null, pid: process.pid, started: 1, last_turn: null, mode: 'ask' }))
const holder = spawn('perl', ['-e', 'use Fcntl ":flock"; $| = 1; open(my $f, ">>", $ARGV[0]) or die; flock($f, LOCK_EX | LOCK_NB) or die; print "held\\n"; sleep 600', join(entry, 'live')], { stdio: ['ignore', 'pipe', 'inherit'] })
await new Promise((done, failed) => {
  holder.stdout.once('data', done)
  holder.once('exit', () => failed(new Error('the lock holder ended before it held the lock')))
})

const { app, page, errors } = await launchWorld({ profile: world.profile, env: world.env, viewport: { width: 1350, height: 900 } })
try {
  const snap = async (name) => { await page.waitForTimeout(300); await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' }); console.log('VISUAL', name) }
  const list = page.locator('.session-list').first()
  const names = async () => (await list.locator('.session-name').allTextContents()).sort()
  const groupOf = (project) => list.locator(':scope > .session-group-section').filter({ has: page.getByRole('button', { name: `New chat in ${project}`, exact: true }) })
  const menuOf = async (project, item) => {
    await groupOf(project).locator('.session-group-head').hover()
    await page.getByRole('button', { name: `Actions for ${project}`, exact: true }).click()
    await page.getByRole('menuitem', { name: item }).click()
  }

  await list.locator('.session-name').filter({ hasText: 'the beta question' }).waitFor()
  await page.locator('[data-test="view-options"]').click()
  await page.getByRole('menuitemcheckbox', { name: 'Group by project' }).click()
  await groupOf('alpha').waitFor()
  const before = await names()
  assert.deepEqual(before, ['background alpha question', 'first alpha question', 'fix the parser', 'stuck alpha question', 'the beta question'])

  // 1. The heading's menu offers both actions, each saying it opens a dialog.
  await groupOf('alpha').locator('.session-group-head').hover()
  await page.getByRole('button', { name: 'Actions for alpha', exact: true }).click()
  await page.getByRole('menuitem', { name: 'Import from Claude Code…' }).waitFor()
  await page.getByRole('menuitem', { name: 'Delete all chats in alpha…' }).waitFor()
  await snap('01-heading-menu')
  await page.keyboard.press('Escape')

  // 2. Import lists both transcripts: the one copied before ticked, shut and marked, and the one
  // whose title is markup drawn as its characters.
  await menuOf('alpha', 'Import from Claude Code…')
  const importing = page.getByRole('dialog', { name: 'Import from Claude Code', exact: true })
  const rows = importing.locator('.import-session')
  await rows.nth(1).waitFor()
  assert.equal(await rows.count(), 2)
  const copied = rows.filter({ hasText: 'fix the parser' })
  const fresh = rows.filter({ hasText: '**bold**' })
  await copied.getByText('Imported', { exact: true }).waitFor()
  assert.equal(await copied.locator('leo-checkbox').evaluate((e) => [e.checked, e.isDisabled ?? e.disabled]).then(String), 'true,true')
  assert.equal(await fresh.getByText('Imported', { exact: true }).count(), 0)
  assert.equal(await fresh.locator('.import-session-title').textContent(), MARKUP)
  assert.equal(await importing.locator('img').count(), 0, 'a title became an element')
  assert.equal(await importing.getByRole('button', { name: 'Import', exact: true }).evaluate((e) => e.isDisabled ?? e.disabled), true)
  await snap('02-import-list')
  await fresh.locator('leo-checkbox').click()
  await importing.getByRole('button', { name: 'Import 1', exact: true }).click()
  await importing.waitFor({ state: 'hidden' })
  await page.locator('[data-test="status-toast"]').filter({ hasText: 'Imported 1 chat' }).waitFor()
  await list.locator('.session-name').filter({ hasText: MARKUP }).waitFor()
  assert.equal(await page.evaluate(() => window.injected), undefined, 'markup in a title ran')

  // 3. Importing again offers the copy as imported.
  await menuOf('alpha', 'Import from Claude Code…')
  await importing.locator('.import-session').nth(1).waitFor()
  assert.equal(await importing.locator('.import-session').filter({ hasText: 'Imported' }).count(), 2)
  assert.equal(await importing.getByRole('button', { name: 'Import', exact: true }).evaluate((e) => e.isDisabled ?? e.disabled), true)
  await importing.locator('[data-test="import-cancel"]').click()
  await importing.waitFor({ state: 'hidden' })

  // 4. A folder Claude Code has nothing for says so.
  await menuOf('beta', 'Import from Claude Code…')
  await importing.locator('[data-test="import-empty"]').filter({ hasText: 'No Claude Code sessions for this folder.' }).waitFor()
  await snap('03-import-empty')
  await importing.getByRole('button', { name: 'Close', exact: true }).click()
  await importing.waitFor({ state: 'hidden' })

  // 5. Deleting all asks first, counting what goes and what a background session keeps, and
  // Cancel deletes nothing.
  const deleting = page.getByRole('dialog', { name: 'Delete all chats', exact: true })
  const detail = deleting.locator('[data-test="delete-detail"]')
  await menuOf('alpha', 'Delete all chats in alpha…')
  await detail.waitFor()
  assert.equal(await detail.textContent(), '4 conversations will be deleted. 1 conversation in use by a background session will be kept.')
  await snap('04-delete-all')
  await deleting.locator('[data-test="delete-cancel"]').click()
  await deleting.waitFor({ state: 'hidden' })
  const withImport = [...before, MARKUP].sort()
  assert.deepEqual(await names(), withImport)
  assert.ok(recordOf(doomed), 'Cancel deleted a record')

  // 6. Confirming deletes what was listed. The one whose trail could not be removed stays and is
  // reported as an error, the background one stays, and the other project is untouched.
  await menuOf('alpha', 'Delete all chats in alpha…')
  await detail.waitFor()
  await deleting.locator('[data-test="delete-confirm"]').click()
  await deleting.waitFor({ state: 'hidden' })
  await page.locator('.problem-text').filter({ hasText: '3 chats deleted. 1 chat could not be deleted and was kept.' }).waitFor()
  await list.locator('.session-name').filter({ hasText: 'first alpha question' }).waitFor({ state: 'detached' })
  assert.deepEqual(await names(), ['background alpha question', 'stuck alpha question', 'the beta question'])
  assert.equal(recordOf(doomed), null)
  assert.ok(recordOf(stuck) && recordOf(busy) && recordOf(theirs))
  await snap('05-deleted-with-error')

  // 7. A folder that has gone is an error in both dialogs, and deletes nothing.
  rmSync(beta, { recursive: true, force: true })
  await menuOf('beta', 'Delete all chats in beta…')
  await deleting.getByRole('alert').waitFor()
  await snap('06-delete-error')
  await deleting.getByRole('button', { name: 'Close', exact: true }).click()
  await deleting.waitFor({ state: 'hidden' })
  await menuOf('beta', 'Import from Claude Code…')
  await importing.getByRole('alert').waitFor()
  await importing.getByRole('button', { name: 'Close', exact: true }).click()
  await importing.waitFor({ state: 'hidden' })
  assert.ok(recordOf(theirs))

  assert.deepEqual(errors, [])
  console.log('PASS: project actions', { output })
} catch (error) {
  await page.screenshot({ path: join(output, 'failure.png') }).catch(() => {})
  throw error
} finally {
  await app.close()
  holder.kill()
  service.close()
  rmSync(world.root, { recursive: true, force: true })
}
