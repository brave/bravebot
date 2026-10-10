// That no decision card in the window takes an approval before the rows it rests on have been on
// screen (PROMPT-4), drawn in the smallest window with every card too tall for it.
//
// Real Electron renderer, isolated profile, and a mocked bridge that records each reply. For every
// case in shown-cases.mjs the card is put to the window and scrolled through, and a check in the
// page, independent of the card's own count, watches every frame: an approving button open while
// one of its tokens has not been wholly on screen, uncovered, since the card last changed width is
// a failure. Then the card must open once it has been read, close again when its width changes,
// refuse a click while closed, and send its reply once pressed open. A refusal never waits.
//
//   npm run drive:shown
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { CASES } from './shown-cases.mjs'

const output = process.env.SHOWN_OUTPUT || join(tmpdir(), 'bravebot-shown')
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-shown-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-shown-project-'))
const id = '11111111-1111-4111-8111-111111111111'
const SMALL = { width: 900, height: 560 }
const WIDER = { width: 1000, height: 560 }

// A window under another stops drawing frames, and the card measures nothing without them.
const app = await electron.launch({ args: ['.', '--disable-renderer-backgrounding', '--disable-background-timer-throttling', '--disable-backgrounding-occluded-windows', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(10000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))

  await app.evaluate(({ ipcMain, BrowserWindow }, { directory, id }) => {
    const now = Math.floor(Date.now() / 1000)
    const row = { id, title: 'Write the release notes', updated: now - 60, directory, project: 'sample-project', branch: 'main', bytes: 20 }
    const emit = (event, data) => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session: 's-' + id })
    globalThis.shown = { emit, replies: [], thrown: [] }
    // Electron's own handler puts a modal box up, which stops the main process and every call into the page.
    process.on('uncaughtException', (error) => globalThis.shown.thrown.push(String(error?.stack ?? error)))
    const replace = (name, fn) => { ipcMain.removeHandler(name); ipcMain.handle(name, fn) }
    replace('bravebot:request', async (_, method, p = {}) => {
      if (method.endsWith('.reply')) { globalThis.shown.replies.push({ method, ...p }); return { ok: {} } }
      if (method === 'agent.info') return { ok: { configured: true, build: '0.9.0 (abc1234)', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: [row] } }
      if (method === 'models.list') return { ok: { defaultModel: 'sample/fast', warnings: [], models: [{ id: 'sample/fast', name: 'Fast model', provider: 'Sample', premium: false, contextWindow: 200000, capabilities: ['text', 'tools'] }] } }
      if (method === 'session.open') return { ok: { session: 's-' + id, model: 'sample/fast', record: { ...row, started: 1, turns: 1, tokens: 100, build: 'fixture' }, said: [{ kind: 'user', text: 'Write the release notes from the changelog.', prompt: 0 }], todos: { rows: [] }, context: '', trust: { known: true, rules: [] }, archived: 0, branchNote: null, buildNote: null, frontNote: null } }
      return { ok: {} }
    })
  }, { directory, id })
  const emit = (event, data) => app.evaluate((_, v) => globalThis.shown.emit(v.event, v.data), { event, data })
  const replies = () => app.evaluate(() => globalThis.shown.replies)
  // A decided card folds to one row without the words its locator found it by, so the reply is what is waited on.
  const replied = async (count) => {
    for (const started = Date.now(); Date.now() - started < 10000; await new Promise((resolve) => setTimeout(resolve, 50))) {
      const all = await replies()
      if (all.length > count) return all.at(-1)
    }
    throw new Error('no reply was sent')
  }
  const frames = (count = 2) => page.evaluate((count) => new Promise((resolve, reject) => {
    const late = setTimeout(() => reject(new Error(`no frame drawn in 5s, the page is ${document.visibilityState}`)), 5000)
    const next = (left) => (left ? requestAnimationFrame(() => next(left - 1)) : (clearTimeout(late), resolve()))
    next(count)
  }), count)

  await page.setViewportSize(SMALL)
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Write the release notes' }).first().click()
  await page.getByRole('textbox', { name: 'Message the agent' }).waitFor()
  await emit('turn.started', { turn: 2 })

  // The check in the page. It finds each token's text in the card and counts it once the whole of
  // it was inside every box that clips it, its own included, and points across its middle hit it.
  // It knows nothing of `data-deciding` or of the card's own count, so a row the card forgot to
  // mark still fails here.
  const watch = (found) => page.evaluate(({ tokens, standing }) => {
    const all = [...tokens, ...standing]
    const state = { card: null, seen: new Set(), width: null, quiet: 0, violations: [], stopped: false }
    globalThis.oracle?.stop()
    globalThis.oracle = { state, stop: () => { state.stopped = true }, seen: () => [...state.seen], unseen: () => all.filter((token) => !state.seen.has(token)) }
    const closed = (button) => button.hasAttribute('disabled') || button.getAttribute('aria-disabled') === 'true'
    const occurrence = (token) => {
      const walker = document.createTreeWalker(state.card, NodeFilter.SHOW_TEXT)
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const at = (node.nodeValue ?? '').indexOf(token)
        if (at < 0) continue
        const range = document.createRange()
        range.setStart(node, at)
        range.setEnd(node, at + token.length)
        return { range, element: node.parentElement }
      }
      return null
    }
    const inView = (token) => {
      const found = occurrence(token)
      if (!found) return false
      let clip = { top: 0, left: 0, right: innerWidth, bottom: innerHeight }
      for (let at = found.element; at; at = at.parentElement) {
        const style = getComputedStyle(at)
        if (style.overflowX === 'visible' && style.overflowY === 'visible') continue
        const box = at.getBoundingClientRect()
        clip = { top: Math.max(clip.top, box.top), left: Math.max(clip.left, box.left), right: Math.min(clip.right, box.right), bottom: Math.min(clip.bottom, box.bottom) }
      }
      const rects = [...found.range.getClientRects()].filter((rect) => rect.width > 0 && rect.height > 0)
      const hits = (rect) => [rect.left + 1, (rect.left + rect.right) / 2, rect.right - 1]
        .every((x) => found.element.contains(document.elementFromPoint(x, (rect.top + rect.bottom) / 2)))
      return rects.length > 0 && rects.every((rect) =>
        rect.top >= clip.top - 1 && rect.bottom <= clip.bottom + 1 && rect.left >= clip.left - 1 && rect.right <= clip.right + 1 && hits(rect))
    }
    const tick = () => {
      if (state.stopped) return
      state.card ??= [...document.querySelectorAll('.confirm')].find((card) => all.every((token) => card.textContent?.includes(token))) ?? null
      if (state.card?.isConnected && state.card.querySelector('.confirm-actions')) {
        const width = state.card.getBoundingClientRect().width
        // The card's count is a frame behind what is on screen, so after a change of width the
        // buttons are left a few frames to close before an open one is held against it.
        if (state.width !== null && Math.abs(width - state.width) > 0.5) { state.seen.clear(); state.quiet = 6 }
        state.width = width
        for (const token of all) if (!state.seen.has(token) && inView(token)) state.seen.add(token)
        if (state.quiet > 0) state.quiet -= 1
        else {
          for (const button of state.card.querySelectorAll('.confirm-actions .approve')) {
            const needs = button.classList.contains('always') ? all : tokens
            const unseen = needs.filter((token) => !state.seen.has(token))
            if (!closed(button) && unseen.length) state.violations.push(`"${button.textContent?.trim()}" open before ${unseen.join(', ')} was on screen`)
          }
        }
        for (const button of state.card.querySelectorAll('.confirm-actions .reject')) if (closed(button)) state.violations.push(`"${button.textContent?.trim()}" waited`)
      }
      requestAnimationFrame(tick)
    }
    requestAnimationFrame(tick)
  }, found)
  const violations = () => page.evaluate(() => globalThis.oracle.state.violations)

  /** Scrolls the transcript over the card, bottom to top and back, and every box inside it through. */
  const sweep = async (card) => {
    const handle = await card.elementHandle()
    const positions = await page.evaluate((card) => {
      const entries = card.closest('.entries')
      const top = card.getBoundingClientRect().top - entries.getBoundingClientRect().top + entries.scrollTop
      const last = entries.scrollHeight - entries.clientHeight
      const step = Math.max(40, entries.clientHeight * 0.3)
      const down = []
      for (let at = Math.max(0, top - 24); at < Math.min(last, top + card.offsetHeight); at += step) down.push(at)
      down.push(last)
      return [...[...down].reverse(), ...down]
    }, handle)
    for (const at of positions) {
      await page.evaluate(({ card, at }) => { card.closest('.entries').scrollTo({ top: at, behavior: 'instant' }) }, { card: handle, at })
      await frames(2)
      const inner = await page.evaluate((card) => {
        const entries = card.closest('.entries').getBoundingClientRect()
        // A box partly in view is scrolled as a person would scroll it, by less than the slice of it showing.
        return [...card.querySelectorAll('*')].map((element, index) => {
          const box = element.getBoundingClientRect()
          return { element, index, showing: Math.min(box.bottom, entries.bottom) - Math.max(box.top, entries.top) }
        }).filter(({ element, showing }) =>
          /auto|scroll/.test(getComputedStyle(element).overflowY) && element.scrollHeight > element.clientHeight + 1 && showing >= Math.min(element.clientHeight, 80),
        ).map(({ index, element, showing }) => ({ index, last: element.scrollHeight - element.clientHeight, step: Math.max(16, showing * 0.3) }))
      }, handle)
      for (const { index, last, step } of inner) {
        for (let at = 0; ; at = Math.min(last, at + step)) {
          await page.evaluate(({ card, index, at }) => card.querySelectorAll('*')[index].scrollTo({ top: at, behavior: 'instant' }), { card: handle, index, at })
          await frames(2)
          if (at >= last) break
        }
        await page.evaluate(({ card, index }) => card.querySelectorAll('*')[index].scrollTo({ top: 0, behavior: 'instant' }), { card: handle, index })
        await frames(2)
      }
    }
  }
  const toBottom = async (card) => {
    await page.evaluate((card) => { const entries = card.closest('.entries'); entries.scrollTo({ top: entries.scrollHeight, behavior: 'instant' }) }, await card.elementHandle())
    await frames(4)
  }
  const states = (card) => card.locator('.confirm-actions .approve, .confirm-actions .reject').evaluateAll((buttons) => buttons.map((button) => ({
    label: button.textContent?.trim(),
    approve: button.classList.contains('approve'),
    closed: button.hasAttribute('disabled') || button.getAttribute('aria-disabled') === 'true',
  })))

  const answered = []
  for (const [name, found] of Object.entries(CASES)) {
    if (found.none) {
      await emit(found.event, found.request)
      const card = page.locator('.confirm.ask').last()
      await card.waitFor()
      assert.equal(await card.locator('[data-deciding], .shown-left').count(), 0, `${name}: marks a row it says nothing waits on`)
      continue
    }
    await page.setViewportSize(SMALL)
    await watch(found)
    await emit(found.event, found.request)
    const card = page.locator('.confirm', { hasText: found.tokens[0] })
    await card.waitFor()
    await toBottom(card)

    // Too tall for the window, so at the bottom of the transcript some of it is above the fold.
    const fits = await card.evaluate((card) => card.offsetHeight <= card.closest('.entries').clientHeight)
    assert.ok(!fits, `${name}: the case fits the window, so it tests nothing`)
    const before = await states(card)
    assert.ok(before.some((button) => button.approve) && before.filter((button) => button.approve).every((button) => button.closed), `${name}: an approval is open at the bottom of a card nobody scrolled: ${JSON.stringify(before)}`)
    assert.match(await card.locator('.shown-left').innerText(), /^\d+ more lines? to read before approving$/, `${name}: the note`)
    const sent = (await replies()).length
    for (const approve of await card.locator('.confirm-actions .approve').all()) {
      await approve.click({ force: true })
      await approve.dispatchEvent('click')
    }
    await frames(4)
    assert.equal((await replies()).length, sent, `${name}: a press on a closed approval sent a reply`)

    await sweep(card)
    assert.deepEqual(await page.evaluate(() => globalThis.oracle.unseen()), [], `${name}: the sweep did not bring every token on screen`)
    const read = await states(card)
    assert.ok(read.every((button) => !button.closed), `${name}: still closed after every row was on screen: ${JSON.stringify(read)}`)
    assert.equal(await card.locator('.shown-left').count(), 0, `${name}: the note outlived the rows it counts`)

    // What was read at one width is not the rows drawn at another.
    await toBottom(card)
    await page.setViewportSize(WIDER)
    await frames(8)
    const resized = await states(card)
    assert.ok(resized.filter((button) => button.approve).every((button) => button.closed), `${name}: a change of width kept the approval open: ${JSON.stringify(resized)}`)
    await sweep(card)
    assert.ok((await states(card)).every((button) => !button.closed), `${name}: closed after it was read again`)

    const ready = (await replies()).length
    await card.locator('.confirm-actions .approve:not(.always)').click()
    const reply = await replied(ready)
    assert.equal(reply.method, `${found.kind}.reply`, `${name}: the reply`)
    assert.equal(reply.request, found.request.request, `${name}: answered the right question`)
    assert.equal(reply.decision, 'approve', `${name}: the answer`)
    assert.deepEqual(await violations(), [], `${name}: an approval was open early`)
    assert.deepEqual(await app.evaluate(() => globalThis.shown.thrown), [], `${name}: the main process threw`)
    await page.evaluate(() => globalThis.oracle.stop())
    answered.push(name)
    console.log('SHOWN', name)
  }

  // A veil takes no pointer, so no hit test finds it; the dock's fade over the foot of the
  // transcript is one. A card swept through under a veil over the whole window is still unread.
  {
    await page.setViewportSize(SMALL)
    const found = CASES.confirm
    await emit(found.event, { ...found.request, request: 800 })
    const card = page.locator('.confirm', { hasText: found.tokens[0] }).last()
    await card.waitFor()
    await page.evaluate(() => {
      const style = document.createElement('style')
      style.id = 'shown-veil'
      style.textContent = '.shown-veil { position: fixed; inset: 0; pointer-events: none } .shown-veil::before { content: ""; position: absolute; inset: 0; background: rgb(0 0 0 / 0.05) }'
      const veil = document.createElement('div')
      veil.className = 'shown-veil'
      veil.dataset.veil = 'before'
      document.head.append(style)
      document.body.append(veil)
    })
    await sweep(card)
    const veiled = await states(card)
    assert.ok(veiled.filter((button) => button.approve).every((button) => button.closed), `confirm: read under a veil, the approval opened: ${JSON.stringify(veiled)}`)
    await page.evaluate(() => { document.querySelector('.shown-veil').remove(); document.querySelector('#shown-veil').remove() })
    await sweep(card)
    assert.ok((await states(card)).every((button) => !button.closed), 'confirm: closed after the veil lifted and the card was read')

    // The count is a frame behind the window, so a press measures again: one in the same frame as
    // a change of width is refused, though the button has not closed yet.
    const sent = (await replies()).length
    const pressed = await card.evaluate((card) => {
      card.style.width = `${card.getBoundingClientRect().width - 40}px`
      const approve = card.querySelector('.confirm-actions .approve')
      const open = approve.getAttribute('aria-disabled') !== 'true'
      approve.click()
      return open
    })
    await frames(4)
    assert.ok(pressed, 'confirm: the approval had closed before the press, so the press tests nothing')
    assert.equal((await replies()).length, sent, 'confirm: a press in the frame the width changed sent a reply')
    await card.evaluate((card) => { card.style.width = '' })
    await sweep(card)
    const ready = (await replies()).length
    await card.locator('.confirm-actions .reject').click()
    assert.equal((await replied(ready)).decision, 'reject', 'confirm: the refusal after the veil')
  }

  // A first row is the whole row, not the first run of text in it. The move card's opens with a
  // label and the destination after it, and a veil over the destination alone leaves it unread.
  // The card is whole on screen when it arrives, so a change of width clears what was counted.
  {
    const found = CASES['mcp-move']
    await emit(found.event, { ...found.request, request: 801, destination: 'https://zqd02.example/mcp' })
    const card = page.locator('.confirm', { hasText: found.tokens[0] }).last()
    await card.waitFor()
    const sharesRow = await card.evaluate((card) => {
      const row = card.querySelector('[data-deciding="first"]')
      const value = row.querySelector('code')
      const veil = document.createElement('span')
      veil.className = 'shown-veil'
      veil.dataset.veil = 'own'
      Object.assign(veil.style, { position: 'absolute', inset: '0', pointerEvents: 'none' })
      Object.assign(value.style, { position: 'relative' })
      value.append(veil)
      card.style.width = `${card.getBoundingClientRect().width - 40}px`
      const label = row.querySelector('strong').getBoundingClientRect()
      const drawn = value.getClientRects()
      return drawn.length === 1 && drawn[0].top < label.bottom && drawn[0].bottom > label.top
    })
    assert.ok(sharesRow, 'mcp-move: the destination is not on the row of its label, so the veil tests nothing')
    await sweep(card)
    const veiled = await states(card)
    assert.ok(veiled.filter((button) => button.approve).every((button) => button.closed), `mcp-move: the destination was veiled, and the approval opened: ${JSON.stringify(veiled)}`)
    await card.evaluate((card) => { card.querySelector('.shown-veil').remove(); card.style.width = '' })
    await sweep(card)
    assert.ok((await states(card)).every((button) => !button.closed), 'mcp-move: closed after the veil lifted and the card was read')
    const ready = (await replies()).length
    await card.locator('.confirm-actions .reject').click()
    assert.equal((await replied(ready)).decision, 'reject', 'mcp-move: the refusal after the veil')
  }

  // The card as it waits and once read, at the smallest and the default size, in both themes.
  const shots = []
  let next = 900
  for (const size of [SMALL, { width: 1440, height: 900 }]) {
    await page.setViewportSize(size)
    for (const name of ['confirm', 'fetch']) {
      const found = CASES[name]
      const request = { ...found.request, request: next++ }
      await watch(found)
      await emit(found.event, request)
      const card = page.locator('.confirm', { hasText: found.tokens[0] }).last()
      await card.waitFor()
      await toBottom(card)
      for (const state of ['waiting', 'read']) {
        if (state === 'read') { await sweep(card); await toBottom(card) }
        for (const theme of ['light', 'dark']) {
          await page.evaluate((dark) => dark ? document.documentElement.setAttribute('data-theme', 'dark') : document.documentElement.removeAttribute('data-theme'), theme === 'dark')
          await frames(6)
          const file = join(output, `shown-${name}-${state}-${size.width}x${size.height}-${theme}.png`)
          await page.screenshot({ path: file })
          shots.push(file)
        }
      }
      await page.evaluate(() => document.documentElement.removeAttribute('data-theme'))
      const ready = (await replies()).length
      await card.locator('.confirm-actions .reject').click()
      assert.equal((await replied(ready)).decision, 'reject', `${name}: the refusal`)
      await page.evaluate(() => globalThis.oracle.stop())
    }
  }

  assert.deepEqual(errors, [])
  assert.deepEqual(await app.evaluate(() => globalThis.shown.thrown), [])
  console.log(`PASS: ${answered.length} cards took no approval before every row they rest on was on screen, closed again on a change of width, refused a press while closed, and sent their reply once read; a row read under a veil, a first row half veiled and a press in the frame of a change of width counted for nothing. Screenshots in ${output}`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'shown-failure.png') }).catch(() => undefined)
  console.error(error)
  for (const thrown of await app.evaluate(() => globalThis.shown?.thrown ?? []).catch(() => [])) console.error('main process threw:', thrown)
  process.exitCode = 1
} finally {
  await app.close()
}
