// Naming a project file with `@` in the composer: the list, the keys that walk it, the file the
// model is sent, the row the transcript draws, a name outside the project refused at send (a
// queued one goes back to the head of a paused queue), and a bot conversation with no project
// naming files in the bot's home folder rather than a project.
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. What the model was sent is read off that service.
//
// Needs `bravebot-rpc` and `bravebot-ui-files` built (`npm run bridge`) and the app built
// (`electron-vite build`), which `npm run drive:at-mentions` does first. Screenshots of the open
// list in light and dark go to AT_MENTIONS_OUTPUT, or /tmp/bravebot-ui.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const listening = (server) => new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))

const rounds = []
// While set, the next reply waits until the test resolves it, so a turn stays running.
let held = null
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  rounds.push(body)
  if (held) await held.promise
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta: { role: 'assistant', content: 'done' }, finish_reason: 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await listening(service)

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-at-mentions-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), join(project, 'src/nested'), join(project, 'node_modules/pkg'), join(project, 'scripts'), profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'src/nested/deep.md'), 'The release colour is blue.\n')
writeFileSync(join(project, 'src/main.rs'), 'fn main() {}\n')
writeFileSync(join(project, 'README.md'), '# Project\n')
writeFileSync(join(root, 'outside.txt'), 'A secret outside the project.\n')
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = process.env.AT_MENTIONS_OUTPUT || '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 900 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.getByRole('button', { name: 'Trust this directory' }).click()
  await trust.waitFor({ state: 'hidden' })

  const field = page.getByRole('textbox', { name: 'Message the agent' })
  const list = page.locator('[data-test="mention-list"]')
  const options = list.getByRole('option')
  const offered = async () => (await options.allInnerTexts()).map((text) => text.trim())
  const selected = (text) => page.waitForFunction((want) => document.querySelector('[data-test="mention-option"][aria-selected="true"]')?.textContent?.trim() === want, text, { timeout: 5000 })
  const value = () => field.inputValue()
  const valueIs = async (want) => {
    for (let tries = 0; tries < 100 && (await value()) !== want; tries++) await page.waitForTimeout(50)
    assert.equal(await value(), want)
  }

  // ---- typing @ offers the project, directories first, noise left out -----------------------
  await field.click()
  await page.keyboard.type('What colour is in @')
  await list.waitFor()
  assert.deepEqual(await offered(), ['scripts/', 'src/', 'README.md'], 'node_modules is not offered')
  await page.screenshot({ path: join(output, 'at-mentions-light.png') })

  // ---- a prefix narrows, the arrows walk, Esc closes and leaves the text -----------------------
  await page.keyboard.type('s')
  await page.waitForFunction(() => document.querySelectorAll('[data-test="mention-option"]').length === 2)
  await selected('scripts/')
  await page.keyboard.press('ArrowDown')
  await selected('src/')
  await page.keyboard.press('ArrowDown')
  await selected('src/')
  await page.keyboard.press('ArrowUp')
  await page.keyboard.press('ArrowUp')
  await selected('scripts/')
  await page.keyboard.press('Escape')
  await list.waitFor({ state: 'hidden' })
  assert.equal(await value(), 'What colour is in @s', 'Escape closed the list and kept the line')
  await page.keyboard.type('r')
  await list.waitFor()

  // ---- Tab completes a directory and the list goes on into it; Enter completes a half name ----
  await page.keyboard.press('Tab')
  await valueIs('What colour is in @src/')
  await page.waitForFunction(() => [...document.querySelectorAll('[data-test="mention-option"]')].map((row) => row.textContent?.trim()).join() === 'src/nested/,src/main.rs')
  await page.keyboard.press('Enter')
  await valueIs('What colour is in @src/nested/')
  assert.equal(rounds.length, 0, 'Enter on a half-typed name completed it rather than sending')
  await page.keyboard.type('de')
  await page.waitForFunction(() => document.querySelectorAll('[data-test="mention-option"]').length === 1)
  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'at-mentions-dark.png') })
  await page.evaluate(() => window.bravebot.writeTheme('system'))
  await page.keyboard.press('Tab')
  await valueIs('What colour is in @src/nested/deep.md ')
  await list.waitFor({ state: 'hidden' })

  // ---- Shift+Enter is still a new line, with or without a name before it -----------------------
  await page.keyboard.press('Shift+Enter')
  await valueIs('What colour is in @src/nested/deep.md \n')
  assert.equal(rounds.length, 0)
  await page.keyboard.press('Backspace')

  // ---- Enter on a finished name sends, and the model is given the file -------------------------
  await page.keyboard.press('Backspace')
  assert.equal(await value(), 'What colour is in @src/nested/deep.md')
  await list.waitFor()
  await page.keyboard.press('Enter')
  await page.locator('.bubble.assistant').first().waitFor()
  await page.locator('.composer .stop').waitFor({ state: 'hidden' })
  assert.equal(rounds.length, 1)
  const sent = JSON.stringify(JSON.parse(rounds[0]).messages)
  assert.ok(sent.includes('Contents of src/nested/deep.md:'), 'the named file went as context')
  assert.ok(sent.includes('The release colour is blue.'))
  await page.locator('.attached', { hasText: 'Read src/nested/deep.md' }).waitFor()
  const rows = await page.locator('.entries .attached').allInnerTexts()
  assert.deepEqual(rows.map((text) => text.trim()), ['Read src/nested/deep.md'], 'one Read row, drawn before the reply came back')

  // ---- a name outside the project is refused at send, and nothing goes -------------------------
  await field.fill('Read @../outside.txt')
  await field.press('Enter')
  const refusal = page.locator('[data-test="send-refused"]')
  await refusal.waitFor()
  assert.match(await refusal.innerText(), /Not sent · @\.\.\/outside\.txt is not a file in this project\. Remove the @ to send it as text\./)
  await page.screenshot({ path: join(output, 'at-mentions-refused.png') })
  assert.equal(rounds.length, 1, 'the model was sent nothing')
  assert.equal(await value(), 'Read @../outside.txt', 'the message is back in the box to fix')
  assert.equal(await page.locator('.bubble.user').count(), 1, 'no bubble for a message that did not go')
  assert.ok(!JSON.stringify(rounds).includes('A secret outside the project'))
  // The bridge refuses the name itself, so a window that skipped the check sends nothing either.
  const bypass = await page.evaluate(async (directory) => {
    const made = await window.bravebot.request('session.new', { directory })
    const session = made.ok.session
    await window.bravebot.request('trust.reply', { session, trusted: true })
    const sent = await window.bravebot.request('turn.send', { session, prompt: 'Read @../outside.txt' })
    await window.bravebot.request('session.close', { session })
    return sent
  }, project)
  assert.equal(bypass.error?.code, 'bad_request', `turn.send refused the name (${JSON.stringify(bypass)})`)
  assert.match(bypass.error.message, /@\.\.\/outside\.txt is not a file in this project/)
  assert.equal(rounds.length, 1, 'the model was sent nothing for a send that skipped the window')
  await field.fill('Read @README.md')
  await field.press('Enter')
  await refusal.waitFor({ state: 'hidden' })
  await page.locator('.bubble.assistant').nth(1).waitFor()
  assert.equal(rounds.length, 2, 'the corrected message went')

  // ---- a refused queued message goes back to the head of the queue, which pauses ---------------
  let release
  held = { promise: new Promise((resolve) => { release = resolve }) }
  await field.fill('Take your time')
  await field.press('Enter')
  for (let tries = 0; tries < 100 && rounds.length < 3; tries++) await page.waitForTimeout(50)
  assert.equal(rounds.length, 3, 'the held turn reached the model')
  await page.locator('.composer .stop').waitFor()
  await field.fill('Read @../outside.txt')
  await field.press('Enter')
  await field.fill('After the refused one')
  await field.press('Enter')
  const tray = page.locator('.queued-messages')
  await tray.waitFor()
  held = null
  release()
  await page.locator('.composer .stop').waitFor({ state: 'hidden' })
  await refusal.waitFor()
  assert.match(await refusal.innerText(), /@\.\.\/outside\.txt is not a file in this project/)
  assert.match(await tray.locator('.tray-title').innerText(), /^Queue paused/, 'the queue paused')
  assert.deepEqual(await tray.locator('.tray-text').allInnerTexts(), ['Read @../outside.txt', 'After the refused one'], 'the refused message is first in the queue')
  await page.waitForTimeout(500)
  assert.equal(rounds.length, 3, 'neither queued message reached the model')
  for (let index = 2; index >= 1; index--) await tray.getByRole('button', { name: `Remove queued message ${index}` }).click()
  await tray.waitFor({ state: 'hidden' })
  await page.locator('[data-test="send-refused"]').getByRole('button', { name: 'Dismiss' }).click()
  await refusal.waitFor({ state: 'hidden' })

  // ---- a bot's conversation with no project names files in the bot's home folder, not a project -
  const bot = await page.evaluate(() => window.bravebot.writeBot({ name: 'Scribe', purpose: 'Answer briefly.' }))
  assert.ok(bot?.home, 'the bot has a home folder')
  mkdirSync(bot.home, { recursive: true })
  writeFileSync(join(bot.home, 'notes.txt'), 'The bot word is lantern.\n')
  await page.reload()
  await page.locator('[data-test="sidebar-tabs"] [role="option"]').nth(1).click()
  await page.locator('.bot').filter({ has: page.locator('.bot-name', { hasText: /^Scribe$/ }) }).locator('.bot-open-button').click()
  await page.locator('[data-test="bot-page"]').waitFor()
  await field.click()
  await page.keyboard.type('Read @notes.txt')
  await page.waitForTimeout(500)
  assert.equal(await list.count(), 0, 'a bot page with no conversation yet has no folder to list')
  await page.keyboard.press('Enter')
  await trust.getByRole('button', { name: 'Trust this directory' }).click()
  await trust.waitFor({ state: 'hidden' })
  await page.locator('.bubble.assistant').first().waitFor()
  await page.locator('.composer .stop').waitFor({ state: 'hidden' })
  assert.equal(rounds.length, 4)
  const botSent = JSON.stringify(JSON.parse(rounds[3]).messages)
  assert.ok(botSent.includes('Contents of notes.txt:') && botSent.includes('The bot word is lantern.'), 'the name typed on the bot page was read from its home folder')
  await field.click()
  await page.keyboard.type('@')
  await list.waitFor()
  const homeOffered = await offered()
  assert.ok(homeOffered.includes('notes.txt') && !homeOffered.includes('README.md'), `the list is the home folder (${homeOffered})`)
  await page.keyboard.press('Escape')
  await field.fill('Read @README.md')
  await field.press('Enter')
  await refusal.waitFor()
  assert.match(await refusal.innerText(), /@README\.md is not a file in this project/)
  assert.equal(rounds.length, 4, 'a project file is not named from the home folder')

  assert.deepEqual(errors, [])
  console.log(`PASS: @ offers the project, narrows, walks and completes; Enter completes a half name and sends a finished one; the model got the file and the transcript drew its Read row; @../outside.txt was refused at send, by turn.send itself too, and a queued one went back to the head of a paused queue; a bot conversation in its home folder read and listed that folder and refused a project file. Screenshots in ${output}/at-mentions-{light,dark}.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'at-mentions-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
