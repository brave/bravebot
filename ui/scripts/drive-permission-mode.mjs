// That the permission mode chosen in the composer governs the session's turns: accepting edits writes a
// file with no card and still puts a command to the window, planning writes nothing and asks
// nothing about a write, the control stays usable while a turn runs, and the Chat menu's
// shortcut walks the modes.
//
// The real app and the real bridge, against a model service this script serves itself, in a
// directory the person did not trust, so every write would be asked about. Nothing is paid for.
// Whether a write landed is read off the disk.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:permission-mode` does first.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_INPUT, ASSISTANT_BUBBLE, COMPOSER_SEND } from './drive-support.mjs'

// A planner that makes the one call the last prompt names, and ends the turn once a tool answered.
const { service, rounds } = await startModelService(({ messages }) => {
  const answered = messages.at(-1)?.role === 'tool'
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  const file = asked.match(/Write ([a-z]+\.md)/)?.[1]
  return answered ? { content: 'done' }
    : file ? { tool: { name: 'write_file', arguments: { path: file, contents: '# Notes\n' } } }
      : asked.includes('Run') ? { tool: { name: 'run', arguments: { command: '/usr/bin/touch ran.txt' } } }
        : { content: 'done' }
})

const lastToolResult = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').at(-1)?.content

const { root, project, profile, env } = makeWorld('bravebot-mode-', { service })

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  await openProject(page, app, project, { trust: false })

  const composer = page.locator(COMPOSER_INPUT)
  const stop = page.locator('.composer .stop')
  const replies = page.locator(ASSISTANT_BUBBLE)
  const mode = page.locator('[data-test="mode-trigger"]')
  const writeCards = page.locator('.confirm:not(.run):not(.ask)')
  const send = async (prompt) => {
    await composer.fill(prompt)
    await page.locator(COMPOSER_SEND).click()
  }
  // A turn the fixture answers at once can show and hide Stop between two polls, so a turn is
  // waited out by its reply.
  const ask = async (prompt) => {
    const before = await replies.count()
    await send(prompt)
    await replies.nth(before).waitFor()
    await stop.waitFor({ state: 'hidden' })
  }
  // The Leo host has no box of its own, so the mode is read off its attribute, not its visibility.
  const inMode = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="mode-trigger"]')?.getAttribute('data-mode') === wanted, wanted)
  // A refused line carries a long note. The path it names stays whole and the note sits after
  // it, inside the row, rather than over the verb or past the column.
  const refusedWriteLaysOut = async (path) => {
    const row = page.locator('.tool.failed').filter({ has: page.locator('.target', { hasText: path }) }).last()
    await row.waitFor()
    const target = await row.locator('.target').evaluate((element) => {
      const box = element.getBoundingClientRect()
      return { left: box.left, right: box.right, width: box.width, clipped: element.scrollWidth > element.clientWidth }
    })
    const verb = await row.locator('.verb').boundingBox()
    const note = await row.locator('.note').boundingBox()
    const edge = await row.boundingBox()
    assert.ok(target.width > 0 && !target.clipped, `the refused write of ${path} shows its whole path: ${JSON.stringify(target)}`)
    assert.ok(target.left >= verb.x + verb.width, `the path of ${path} follows the verb`)
    assert.ok(note.x >= target.right, `the note on ${path} starts after the path`)
    assert.ok(note.x + note.width <= edge.x + edge.width + 0.5, `the note on ${path} stays inside the row`)
  }
  const changeState = (path) => page.locator('.file-row', { hasText: path }).locator('.tag').innerText()
  const cycle = () => app.evaluate(({ Menu }) => Menu.getApplicationMenu().getMenuItemById('mode.cycle').click())

  assert.equal(await mode.getAttribute('data-mode'), 'ask', 'a session opens asking')
  await page.screenshot({ path: join(output, 'mode-ask.png') })
  await mode.click()
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).waitFor()
  // Leo fades an item's colours in, so the pointer is moved off the menu and the screenshot waits for them.
  await page.mouse.move(5, 5)
  await page.waitForTimeout(1500)
  await page.screenshot({ path: join(output, 'mode-menu.png') })
  // One menu holds the approval and sandbox controls, each in a group of its own, with its copy and the chosen item marked.
  assert.deepEqual((await page.locator('.controls-heading').allInnerTexts()).map((text) => text.trim()), ['Approvals', 'Sandbox'])
  assert.deepEqual(await page.locator('leo-menu-item[data-role="menuitemradio"]').evaluateAll((items) => items.map((item) => [
    item.getAttribute('data-test'), item.querySelector('.controls-option-text > span:first-child')?.textContent, item.querySelector('.menu-detail')?.textContent ?? null, item.getAttribute('aria-checked'),
  ])), [
    ['mode-option-ask', 'Ask', 'Asks before every write and command', 'true'],
    ['mode-option-acceptEdits', 'Accept edits', 'Writes without asking. Commands still ask', 'false'],
    ['mode-option-plan', 'Plan', 'Reads and plans only. Commands still ask', 'false'],
    ['mode-option-auto', 'Auto', 'Writes freely. A model checks commands', 'false'],
    ['sandbox-option-workspace-write', 'Workspace write', 'Can read most files, but can only make changes inside this workspace', 'true'],
    ['sandbox-option-read-only', 'Read only', 'Can read most files, but can’t change anything', 'false'],
    ['sandbox-option-strict', 'Strict', 'Can only access the files named in the command. Anything else fails', 'false'],
  ], 'every item, its copy, and which one is chosen')
  const popupBox = () => page.evaluate(() => {
    const box = document.querySelector('.controls-menu').shadowRoot.querySelector('.leo-menu-popup')
    const { top, bottom } = box.getBoundingClientRect()
    return { top, bottom, height: window.innerHeight, scrolls: box.scrollHeight > box.clientHeight, overflowY: getComputedStyle(box).overflowY }
  })
  const roomy = await popupBox()
  assert.ok(roomy.top >= 0 && roomy.bottom <= roomy.height, `the menu is not cropped where the composer sits mid-window: ${JSON.stringify(roomy)}`)
  const background = (selector) => page.locator(selector).first().evaluate((node) => getComputedStyle(node).backgroundColor)
  assert.notEqual(await background('leo-menu-item[aria-checked="true"]'), await background('leo-menu-item[aria-checked="false"]'), 'the chosen item has a background of its own')
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).click()
  await inMode('acceptEdits')
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).waitFor({ state: 'hidden' })
  await page.waitForFunction(() => document.activeElement?.getAttribute('data-test') === 'mode-trigger')
  await ask('Write notes.md')
  assert.equal(await writeCards.count(), 0, 'accepting edits put a write to the window')
  assert.equal(readFileSync(join(project, 'notes.md'), 'utf8'), '# Notes\n', 'the write landed')
  await page.screenshot({ path: join(output, 'mode-accept-edits.png') })

  await send('Run a command')
  const run = page.locator('.confirm.run')
  await run.waitFor()
  assert.equal(await mode.locator('button').isDisabled(), false, 'the mode can be changed while a turn runs')
  await page.screenshot({ path: join(output, 'mode-command-asked.png') })
  await run.locator('.reject').click()
  await stop.waitFor({ state: 'hidden' })
  assert.equal(existsSync(join(project, 'ran.txt')), false, 'a refused command ran')

  await cycle()
  await inMode('plan')
  const sentBefore = rounds.length
  await ask('Write plan.md')
  assert.equal(await writeCards.count(), 0, 'planning put a write to the window')
  assert.equal(existsSync(join(project, 'plan.md')), false, 'planning wrote a file')
  assert.ok(rounds[sentBefore].includes('Plan mode.'), 'the planner was told it is planning')
  assert.match(lastToolResult(rounds.at(-1)), /refused: the session is in plan mode/, 'the planner was told plan mode refused the write')
  await refusedWriteLaysOut('plan.md')
  assert.equal(await changeState('plan.md'), 'refused', 'a write plan mode refused is listed as refused, not failed')
  assert.equal(await changeState('notes.md'), 'applied', 'the write that landed is listed as applied')
  await page.screenshot({ path: join(output, 'mode-plan.png') })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'mode-plan-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })

  // The shortcut walks on from planning to auto, drawn with the Brave Leo icon, and then back to asking.
  await cycle()
  await inMode('auto')
  assert.equal(await mode.locator('leo-icon[name="product-brave-leo"]').count(), 1, 'auto is drawn with the Brave Leo icon')
  assert.equal((await mode.innerText()).trim(), 'Auto')
  await page.screenshot({ path: join(output, 'mode-auto.png') })
  await cycle()
  await inMode('ask')
  await send('Write later.md')
  await writeCards.first().waitFor()
  await writeCards.first().locator('.reject').click()
  await stop.waitFor({ state: 'hidden' })
  assert.equal(existsSync(join(project, 'later.md')), false, 'a write the person refused landed')
  await refusedWriteLaysOut('later.md')
  assert.equal(await changeState('later.md'), 'refused', 'a write the person refused is listed as refused')
  await page.screenshot({ path: join(output, 'mode-ask-refused.png') })

  // The model picker is the last control before Send, and its selected row is drawn on the selected background.
  await page.keyboard.press('Escape')
  const order = await page.locator('.composer-toolbar [data-test="mode-trigger"], .composer-toolbar [data-test="model-trigger"], .composer-toolbar [data-test="send-message"]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-test')))
  assert.deepEqual(order, ['mode-trigger', 'model-trigger', 'send-message'], 'the model sits between the session controls and Send')
  await page.locator('[data-test="model-trigger"]').click()
  await page.locator('[data-test="model-popover"]').waitFor()
  await page.mouse.move(5, 5)
  await page.waitForTimeout(1500)
  const selectedRow = await page.locator('.model-menu leo-option[data-current="true"]').first().evaluate((node) => getComputedStyle(node).backgroundColor)
  assert.notEqual(selectedRow, 'rgba(0, 0, 0, 0)', 'the selected model row has a background')
  await page.screenshot({ path: join(output, 'mode-model-menu.png') })
  await page.keyboard.press('Escape')

  // A window too short for the whole menu scrolls it, and every item can be brought inside the window.
  await page.setViewportSize({ width: 1000, height: 560 })
  await mode.click()
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).waitFor()
  await page.mouse.move(5, 5)
  await page.waitForTimeout(1500)
  await page.screenshot({ path: join(output, 'mode-menu-short-window.png') })
  // The popup is Leo's own box inside the menu's shadow root. It stays inside the window and scrolls
  // what does not fit, as the model picker's list does.
  const popup = await popupBox()
  assert.ok(popup.top >= 0 && popup.bottom <= popup.height, `the menu stays inside a short window: ${JSON.stringify(popup)}`)
  assert.ok(popup.scrolls && popup.overflowY !== 'visible', `and scrolls what does not fit: ${JSON.stringify(popup)}`)
  const items = page.locator('leo-menu-item[data-role="menuitemradio"]')
  const last = await items.last().evaluate((node) => {
    node.scrollIntoView({ block: 'nearest' })
    const box = node.getBoundingClientRect()
    return { top: box.top, bottom: box.bottom, height: window.innerHeight }
  })
  assert.ok(last.top >= 0 && last.bottom <= last.height, `the last item can be scrolled into view: ${JSON.stringify(last)}`)
  await page.screenshot({ path: join(output, 'mode-menu-short-window-end.png') })
  await page.keyboard.press('Escape')
  await page.setViewportSize({ width: 1400, height: 1000 })

  assert.deepEqual(errors, [])
  console.log(`PASS: accepting edits writes with no card and still asks about a command, the control works while a turn runs, planning writes nothing and asks nothing, a refused write keeps its path in view and is listed as refused, and the menu shortcut walks from planning through auto back to asking. Screenshots in ${output}/mode-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'mode-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
