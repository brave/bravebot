// Dropping files on the desktop window (docs/specs/dropping.md, DROP-1 to DROP-6 and DROP-10).
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. The drop is a trusted drag driven through Chromium's input pipeline
// (`Input.dispatchDragEvent`), which reaches the preload as an OS drag would: a `drop` event the
// browser marks trusted, carrying File objects `webUtils.getPathForFile` resolves. What it does not
// exercise is the operating system's own drag session (Finder handing the files to Chromium).
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

// The model service. Every request body is kept; each answer waits a moment, so a message can be
// queued behind a turn that is still running.
const requests = []
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  requests.push(body)
  await new Promise((resolve) => setTimeout(resolve, 2500))
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta: { role: 'assistant', content: 'done' }, finish_reason: 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await new Promise((resolve) => service.listen(0, '127.0.0.1', resolve))

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-drop-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
// Outside the project, which is where a drop usually comes from (DROP-3).
const desktop = join(root, 'desktop')
for (const path of [join(home, '.bravebot'), project, profile, join(desktop, 'a folder')]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# scratch\n')
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? '/tmp/bravebot-ui'
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
  await page.setViewportSize({ width: 1400, height: 1000 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))

  // A real picture, so the thumbnail has something to draw.
  const pictureBytes = await app.evaluate(({ nativeImage }) => {
    const size = 48, pixels = Buffer.alloc(size * size * 4)
    for (let i = 0; i < size * size; i++) pixels.set([0xd0, 0x60, 0x20, 0xff], i * 4)
    return nativeImage.createFromBitmap(pixels, { width: size, height: size }).toPNG().toString('base64')
  })
  const at = (name, bytes) => { const path = join(desktop, name); writeFileSync(path, bytes); return path }
  const png = at('shot.png', Buffer.from(pictureBytes, 'base64'))
  const second = at('second.png', Buffer.from(pictureBytes, 'base64'))
  const third = at('third.png', Buffer.from(pictureBytes, 'base64'))
  const pdf = at('scan.pdf', '%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n')
  const md = at('notes.md', 'The drop marker colour is teal.\n')
  const dmg = at('installer.dmg', 'not text')
  const folder = join(desktop, 'a folder')

  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await trust.getByRole('button', { name: 'Trust this directory' }).click()
  await trust.waitFor({ state: 'hidden' })

  const composer = page.getByRole('textbox', { name: 'Message the agent' })
  const chips = page.locator('[data-test="dropped-chip"]')
  const stop = page.locator('.composer .stop')
  const cdp = await page.context().newCDPSession(page)
  const point = async (selector) => {
    const box = await page.locator(selector).first().boundingBox()
    return { x: Math.round(box.x + box.width / 2), y: Math.round(box.y + box.height / 2) }
  }
  const drag = async (files, selector, { drop = true } = {}) => {
    const { x, y } = await point(selector)
    const data = { items: [], files, dragOperationsMask: 1 }
    await cdp.send('Input.dispatchDragEvent', { type: 'dragEnter', x, y, data })
    // Chromium lets go only onto a target whose answer to the last dragover allowed a drop, and that
    // answer comes back from the renderer asynchronously. A person's drag sends a stream of them, so
    // this sends a few, spaced out, before letting go.
    for (let i = 0; i < 3; i++) {
      await cdp.send('Input.dispatchDragEvent', { type: 'dragOver', x, y, data })
      await page.waitForTimeout(100)
    }
    if (drop) await cdp.send('Input.dispatchDragEvent', { type: 'drop', x, y, data })
  }

  // ---- dragging over the conversation shows where to let go -------------------------------------
  await drag([png], '.entries', { drop: false })
  await page.locator('[data-test="drop-target"]').waitFor()
  await page.screenshot({ path: join(output, 'drop-target.png') })
  await cdp.send('Input.dispatchDragEvent', { type: 'dragCancel', x: 0, y: 0, data: { items: [], files: [png], dragOperationsMask: 1 } }).catch(() => undefined)
  await page.mouse.move(1, 1)

  // ---- one drop of several files, onto the conversation ---------------------------------------
  // The page keeps the File objects a real drop hands it, to try replaying them below.
  await page.evaluate(() => {
    window.addEventListener('drop', (event) => { window.kept = [...event.dataTransfer.files] }, true)
  })
  await composer.fill('Compare ')
  await drag([png, pdf, md, folder, dmg], '.entries')
  await chips.nth(2).waitFor()
  assert.equal(await chips.count(), 3, 'a picture, a PDF and a text file are staged; a folder and a .dmg are not')
  assert.equal(await composer.inputValue(), `Compare [Image #1] [PDF #2] [File #3] ${dmg} `)
  await page.getByText('Folders are not attached', { exact: true }).waitFor()
  assert.equal(await page.locator('[data-test="dropped-chip"] img.attachment-thumb').count(), 1, 'the picture has a thumbnail')
  assert.match(await page.locator('.attachment-trust').innerText(), /Sent as trusted context/)
  await page.locator('[data-test="composer-mode"]').click()
  assert.equal(await page.locator('[data-test="mode-plan"]').getAttribute('aria-disabled'), 'true', 'Plan is off while anything is staged')
  await page.locator('[data-test="composer-mode"]').click()
  await page.locator('[data-test="mode-plan"]').waitFor({ state: 'hidden' })
  assert.ok(!(await page.evaluate(() => document.body.innerHTML)).includes(desktop + '/shot.png'), 'the page never holds a staged file’s path')
  await page.screenshot({ path: join(output, 'drop-staged-light.png') })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'drop-staged-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })

  // ---- a drop onto the composer, then taken off two ways --------------------------------------
  await drag([second, third], '.composer textarea')
  await chips.nth(4).waitFor()
  assert.match(await composer.inputValue(), /\[Image #4\] \[Image #5\]/)
  await page.getByRole('button', { name: 'Remove [Image #4] second.png' }).click()
  assert.doesNotMatch(await composer.inputValue(), /\[Image #4\]/, 'removing the chip removes its marker')
  await composer.fill((await composer.inputValue()).replace('[Image #5] ', ''))
  assert.equal(await chips.count(), 3, 'deleting the marker takes the file off')

  // ---- a drop the page made up does nothing ---------------------------------------------------
  // Both a file the page made and the real files an earlier drop handed it, replayed in an event
  // the page dispatched. The second is the one that matters: those File objects do resolve to
  // paths, so only the event's trusted bit keeps a page from granting itself a file again.
  const before = await composer.inputValue()
  const forged = await page.evaluate(() => {
    const transfer = new DataTransfer()
    transfer.items.add(new File(['x'], 'forged.png', { type: 'image/png' }))
    for (const kept of window.kept ?? []) transfer.items.add(kept)
    const target = document.querySelector('[data-drop-session]')
    for (const type of ['dragenter', 'dragover', 'drop']) target.dispatchEvent(new DragEvent(type, { dataTransfer: transfer, bubbles: true, cancelable: true }))
    return { replayed: (window.kept ?? []).length, api: Object.keys(window.bravebot).filter((key) => /stage|drop/i.test(key)) }
  })
  await page.waitForTimeout(800)
  assert.ok(forged.replayed >= 3, 'the page did hold the real files to replay')
  assert.deepEqual(forged.api, ['onDrop'], 'the page has no way to stage a path')
  assert.equal(await composer.inputValue(), before, 'an untrusted drop event stages nothing')
  assert.equal(await chips.count(), 3)
  assert.equal(await page.locator('[data-test="status-toast"]').filter({ hasText: 'Could not attach' }).count(), 0, 'and is not even looked at')

  // ---- sending carries what the draft still names ---------------------------------------------
  const sentBefore = requests.length
  await page.locator('.composer .send').click()
  await stop.waitFor({ state: 'visible' })
  assert.equal(await chips.count(), 0, 'sending clears what was staged')
  await page.locator('.attached').filter({ hasText: 'Read notes.md' }).waitFor()

  // ---- a message queued behind the running turn keeps its own drop ----------------------------
  await drag([third], '.composer textarea')
  await chips.first().waitFor()
  assert.match(await composer.inputValue(), /^\[Image #6\] $/)
  await composer.fill('Queued look at [Image #6] ')
  await composer.press('Enter')
  await page.locator('.queued-messages').waitFor()
  assert.equal(await chips.count(), 0, 'queuing takes the drop with the message')
  await page.waitForFunction(() => document.querySelector('.queued-messages') === null, null, { timeout: 60000 })
  await stop.waitFor({ state: 'visible' })
  await stop.waitFor({ state: 'hidden', timeout: 60000 })

  const sent = requests.slice(sentBefore)
  const first = sent.find((body) => body.includes('Compare [Image #1]'))
  assert.ok(first, 'the prompt reached the model with its markers')
  assert.ok(first.includes('data:image/png;base64,'), 'the dropped picture reached the model as bytes')
  assert.ok(first.includes('data:application/pdf;base64,'), 'the dropped PDF reached the model as bytes')
  assert.ok(first.includes(`Contents of ${md}:`) || first.includes(`Contents of ${md.replace(/\//g, '\\/')}:`), 'the dropped text file reached the model as context')
  assert.ok(first.includes('The drop marker colour is teal.'))
  assert.ok(!first.includes('forged.png'), 'the forged drop sent nothing')
  assert.ok(!first.includes(second) && !first.includes(third), 'files taken off before sending were not sent')
  const queued = sent.find((body) => body.includes('Queued look at [Image #6]'))
  assert.ok(queued, 'the queued message was sent when the turn ended')
  assert.ok(queued.slice(queued.lastIndexOf('Queued look at')).includes('data:image/png;base64,'), 'with the picture it was queued with')
  await page.screenshot({ path: join(output, 'drop-sent.png') })

  // ---- a queued message refused for its `@` name goes back on the queue with its drop -----------
  await composer.fill('Hold the turn')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await drag([third], '.composer textarea')
  await chips.first().waitFor()
  const marker = (await composer.inputValue()).trim()
  await composer.fill(`Requeued ${marker} with @later.md `)
  await composer.press('Enter')
  const tray = page.locator('.queued-messages')
  await tray.waitFor()
  await stop.waitFor({ state: 'hidden', timeout: 60000 })
  await page.locator('[data-test="send-refused"]').waitFor()
  assert.match(await tray.locator('.tray-title').innerText(), /^Queue paused/, 'the refused message paused the queue')
  writeFileSync(join(project, 'later.md'), 'Written after the refusal.\n')
  await tray.getByRole('button', { name: 'Resume queue' }).click()
  await tray.waitFor({ state: 'hidden', timeout: 60000 })
  await stop.waitFor({ state: 'hidden', timeout: 60000 })
  const requeued = requests.find((body) => body.includes(`Requeued ${marker}`))
  assert.ok(requeued, 'the requeued message was sent once its name could go')
  assert.ok(requeued.slice(requeued.lastIndexOf('Requeued')).includes('data:image/png;base64,'), 'with the picture it was queued with')

  assert.deepEqual(errors, [])
  console.log(`PASS: a trusted drop stages markers and chips, a forged drop does nothing, and the send carries the picture, the PDF and the text file, and a queued message refused for its @ name keeps its picture. Screenshots in ${output}/drop-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'drop-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
