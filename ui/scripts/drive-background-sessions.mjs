// The background sessions section of the sidebar (BG-17): headings in BG-16 order with counts, a
// selected row that follows its session to a new heading, the detail of the selected row, the
// copied attach command, and no section at all while the roster is empty.
//
// Real Electron renderer, isolated profile (a temp --user-data-dir, never the real one) and a
// mocked bridge whose `background.list` answers with a roster the script changes between polls.
// No provider requests, no live agent. Screenshots go to BACKGROUND_OUTPUT, or a temp directory
// the run prints.
//
//   npx electron-vite build && node scripts/drive-background-sessions.mjs
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { oneModelList } from './acceptance-fixture.mjs'
import { SIDEBAR_TAB } from './drive-support.mjs'

const output = process.env.BACKGROUND_OUTPUT || mkdtempSync(join(tmpdir(), 'bravebot-background-'))
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-background-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-background-project-'))
const now = Math.floor(Date.now() / 1000)
// The headings and state words the bridge sends, which are the terminal list's (BG-16, SCREEN-16).
const HEADINGS = { 'needs-input': 'Needs input', interrupted: 'Interrupted', working: 'Working', idle: 'Ready', stopped: 'Stopped' }
const ORDER = Object.keys(HEADINGS)
const words = ({ state, held }) => (state === 'needs-input' && held ? `${HEADINGS[state]} (${held})` : HEADINGS[state])
const session = (n, state, name, ago, extra = {}) => {
  const row = {
    id: `${String(n).repeat(8)}-0000-4000-8000-00000000000${n}`,
    short: String(n).repeat(8),
    name,
    state,
    held: null,
    updated: now - ago,
    directory: `${directory}/checkout-${n}`,
    prompt: `the prompt of session ${n}`,
    ...extra,
  }
  return { ...row, words: words(row) }
}
// The answer of `background.list` for rows in the bridge's order: a group for each state a row is in.
const listed = (rows) => ({
  groups: ORDER.flatMap((state) => {
    const sessions = rows.filter((row) => row.state === state)
    return sessions.length ? [{ state, heading: HEADINGS[state], count: sessions.length, sessions }] : []
  }),
})
// As the bridge sends them: BG-16 order, most recent first within a state.
const roster = [
  session(1, 'needs-input', 'Fix the feed parser', 30, { held: 'run' }),
  session(2, 'interrupted', 'Migrate the tide tables', 900),
  session(3, 'working', 'Write the release notes', 5),
  session(4, 'working', 'Review the lock-free read path', 120),
  session(5, 'idle', 'A deliberately long background session name that is cut', 600),
  session(6, 'stopped', 'Look at the importer', 2 * 86400),
]
const chats = [{
  id: '00000000-0000-4000-8000-000000000001', title: 'A chat in the window', updated: now - 60,
  directory, project: 'project', branch: 'main', bytes: 20,
}]

const app = await electron.launch({
  args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), '--disable-renderer-backgrounding', '--disable-background-timer-throttling', `--user-data-dir=${profile}`],
  cwd: process.cwd(), timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(10000)
  const errors = []
  page.on('pageerror', (e) => { errors.push(e.message); console.error('RENDERER', e.message) })
  await app.evaluate(({ ipcMain }, { roster, chats, models }) => {
    globalThis.backgroundRoster = roster
    globalThis.backgroundReads = 0
    globalThis.requested = []
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, p = {}) => {
      globalThis.requested.push(method)
      if (method === 'agent.info') return { ok: { configured: true, build: 'background fixture', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: chats } }
      // Choosing the Chats tab opens the chat at the top of the list.
      if (method === 'session.open') {
        const record = { ...chats.find((row) => row.id === p.id), started: 1, turns: 0, tokens: 0, build: 'fixture' }
        return { ok: { session: `s-${p.id}`, model: 'sample/fast', record, said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0, branchNote: null, buildNote: null, frontNote: null } }
      }
      if (method === 'models.list') return { ok: models }
      if (method === 'background.list') { globalThis.backgroundReads++; return { ok: globalThis.backgroundRoster } }
      return { ok: {} }
    })
  }, { roster: listed(roster), chats, models: oneModelList })
  const setRoster = (rows) => app.evaluate((_, answer) => { globalThis.backgroundRoster = answer }, listed(rows))
  const reads = () => app.evaluate(() => globalThis.backgroundReads)
  // Tall enough that the six rows and the detail fit in the column without scrolling.
  await page.setViewportSize({ width: 1440, height: 1100 })
  await page.reload()

  const snap = async (name) => { await page.waitForTimeout(300); await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' }); console.log('VISUAL', name) }
  const section = page.locator('[data-test="background-sessions"]')
  const groups = section.locator('.background-group')
  const heads = groups.locator('.background-group-head')
  const row = (n) => section.locator(`.background-row[data-session="${roster[n - 1].id}"]`)
  const under = (state) => section.locator(`.background-group[data-state="${state}"]`)
  const detail = section.locator('[data-test="background-detail"]')

  // 1. Every state has a heading, in BG-16 order, with the number of sessions under it, and the
  // rows keep the bridge's order within a state.
  await section.waitFor()
  await page.locator('.session-list .session-row').first().waitFor()
  assert.equal(await section.locator('.background-title').textContent(), 'Background sessions')
  assert.deepEqual(await heads.locator('.background-group-name').allTextContents(),
    ['Needs input', 'Interrupted', 'Working', 'Ready', 'Stopped'])
  assert.deepEqual(await heads.locator('.count').allTextContents(), ['1', '1', '2', '1', '1'])
  assert.deepEqual(await under('working').locator('.background-short').allTextContents(), ['33333333', '44444444'])
  // The section is above the chats.
  assert.equal(await page.locator('.session-list').first().locator(':scope > :first-child').getAttribute('data-test'), 'background-sessions')

  // 2. A row: the short id, how long ago, the name, and the state with the kind of prompt held.
  assert.equal(await row(1).locator('.background-short').textContent(), '11111111')
  assert.equal(await row(1).locator('.background-name').textContent(), 'Fix the feed parser')
  assert.equal(await row(1).locator('.background-time').textContent(), 'just now')
  assert.equal(await row(1).locator('.background-state').textContent(), 'Needs input (run)')
  assert.equal(await row(2).locator('.background-time').textContent(), '15 minutes ago')
  assert.equal(await row(6).locator('.background-time').textContent(), '2 days ago')
  assert.equal(await row(5).locator('.background-state').textContent(), 'Ready')
  // A name past 28 columns ends in an ellipsis, and the whole of it is in the tooltip.
  const name = row(5).locator('.background-name')
  assert.equal(await name.evaluate((e) => getComputedStyle(e).textOverflow), 'ellipsis')
  assert.equal(await name.evaluate((e) => e.scrollWidth > e.clientWidth), true, 'the long name is cut')
  assert.equal(await name.getAttribute('data-tooltip'), roster[4].name)
  assert.equal(await detail.count(), 0, 'nothing is selected yet, so there is no detail')

  // 3. Selecting a row shows its directory and its last prompt.
  await row(4).click()
  await detail.waitFor()
  assert.equal(await row(4).getAttribute('aria-pressed'), 'true')
  assert.equal(await detail.locator('code').first().getAttribute('data-tooltip'), roster[3].directory)
  assert.equal(await detail.locator('.background-prompt').textContent(), 'the prompt of session 4')
  assert.equal(await detail.locator('.background-command').textContent(), 'bravebot attach 44444444')
  await snap('01-background-sessions-light')

  // 4. The selected session's state changes between polls: it moves under Needs input, stays
  // selected, and the counts follow. Working keeps its other session.
  const moved = { ...roster[3], state: 'needs-input', held: 'write', updated: now }
  await setRoster([
    { ...moved, words: words(moved) },
    roster[0], roster[1], roster[2], roster[4], roster[5],
  ])
  await under('needs-input').locator(`.background-row[data-session="${roster[3].id}"]`).waitFor()
  assert.deepEqual(await heads.locator('.count').allTextContents(), ['2', '1', '1', '1', '1'])
  assert.deepEqual(await under('needs-input').locator('.background-short').allTextContents(), ['44444444', '11111111'],
    'the session written last is first under its heading')
  assert.equal(await row(4).getAttribute('aria-pressed'), 'true', 'the moved row is still selected')
  assert.equal(await row(4).locator('.background-state').textContent(), 'Needs input (write)')
  assert.equal(await section.locator('.background-row[aria-pressed="true"]').count(), 1)
  assert.equal(await detail.locator('.background-prompt').textContent(), 'the prompt of session 4')

  // 5. The one action copies the attach command and nothing else.
  const sentBefore = await app.evaluate(() => globalThis.requested.length)
  await app.evaluate(({ clipboard }) => clipboard.writeText('before'))
  await detail.locator('[data-test="copy-attach"]').click()
  for (let tries = 0; tries < 50 && (await app.evaluate(({ clipboard }) => clipboard.readText())) === 'before'; tries++) await page.waitForTimeout(100)
  assert.equal(await app.evaluate(({ clipboard }) => clipboard.readText()), 'bravebot attach 44444444')
  assert.equal(await section.getByRole('button').count(), roster.length + 1,
    'the section has a button for each row and the copy button, and no other action')
  // Clicking every row and the copy button sends the bridge nothing but reads of the roster:
  // no row control starts, replies to or stops a session.
  for (const n of [1, 2, 3, 4, 5, 6]) await row(n).click()
  await detail.locator('[data-test="copy-attach"]').click()
  await page.waitForTimeout(1500)
  const sent = await app.evaluate((_, from) => globalThis.requested.slice(from), sentBefore)
  assert.deepEqual([...new Set(sent)].filter((method) => method !== 'background.list'), [],
    'a row control sent only background.list')
  await row(4).click()
  await detail.waitFor()

  // 6. Dark, with the moved row selected, for the record.
  await page.emulateMedia({ colorScheme: 'dark' })
  await snap('02-background-sessions-dark')
  await page.emulateMedia({ colorScheme: 'light' })

  // 7. It stops reading while the Bots tab hides the list, and starts again on the Chats tab.
  await page.locator(SIDEBAR_TAB).nth(1).click()
  await page.waitForTimeout(500)
  const hiddenFrom = await reads()
  await page.waitForTimeout(2500)
  assert.equal(await reads(), hiddenFrom, 'nothing is read while the section cannot be seen')
  await page.locator(SIDEBAR_TAB).nth(0).click()
  for (let tries = 0; tries < 30 && (await reads()) === hiddenFrom; tries++) await page.waitForTimeout(100)
  assert.ok((await reads()) > hiddenFrom, 'reading starts again once the list is shown')

  // 8. A session that leaves the roster is no longer selected, and an empty roster hides the
  // section, with the chats still drawn.
  await setRoster([roster[0]])
  await row(4).waitFor({ state: 'detached' })
  assert.equal(await detail.count(), 0, 'the detail goes with the selected session')
  await setRoster([])
  await section.waitFor({ state: 'hidden' })
  assert.equal(await section.locator('.background-row').count(), 0)
  await page.locator('.session-list .session-row').first().waitFor()
  await snap('03-empty-roster-light')
  await page.emulateMedia({ colorScheme: 'dark' })
  await snap('04-empty-roster-dark')
  await page.emulateMedia({ colorScheme: 'light' })

  // 9. A session that appears is shown again without a reload.
  await setRoster([roster[2]])
  await section.waitFor()
  assert.deepEqual(await heads.locator('.background-group-name').allTextContents(), ['Working'])

  assert.deepEqual(errors, [])
  assert.deepEqual(await page.locator('[role="alert"]').allTextContents(), [], 'no error was shown')
  console.log('PASS: background sessions', { output })
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'failure.png') }).catch(() => {})
  throw error
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync(directory, { recursive: true, force: true })
}
