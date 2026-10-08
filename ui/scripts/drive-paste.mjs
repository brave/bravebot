// Pasting a picture into the desktop composer (docs/specs/pasting.md, PASTE-2, PASTE-3, PASTE-6,
// PASTE-7 and PASTE-9).
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. The picture is put on the operating system's clipboard by the main process
// (`clipboard.write`), and the paste is the browser's own paste command, sent two ways: through
// `webContents.paste()`, which is what the Edit menu's Paste item (and so Command-V on macOS) calls,
// and through CDP `Input.dispatchKeyEvent` carrying the `paste` editing command, which is how a
// keyboard paste reaches the renderer elsewhere. Both make the browser fire a paste event it marks
// trusted, read from the real clipboard. What neither exercises is the key press itself reaching
// the menu's accelerator. The clipboard is put back as it was found when the script ends.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

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

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-paste-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile'), desktop = join(root, 'desktop')
for (const path of [join(home, '.bravebot'), project, profile, desktop]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# scratch\n')
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const launch = () => electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})

let app = await launch()
// What the clipboard held before, so the person running this gets it back.
await app.evaluate(async ({ clipboard }) => { globalThis.keptClipboard = await clipboard.read().catch(() => []) })
const restoreClipboard = (on) => on.evaluate(async ({ clipboard }, text) => {
  if (globalThis.keptClipboard?.length) await clipboard.write(globalThis.keptClipboard).catch(() => clipboard.writeText(text))
  else await clipboard.writeText(text)
}, '')
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))

  /** A solid picture of `size` pixels, or noise, which no PNG encoder can shrink, as base64 PNG. */
  const picture = (size, colour, noise = false) => app.evaluate(({ nativeImage }, [size, colour, noise]) => {
    const pixels = Buffer.alloc(size * size * 4)
    for (let i = 0; i < size * size; i++) {
      pixels.set(noise ? [Math.random() * 256, Math.random() * 256, Math.random() * 256, 0xff] : colour, i * 4)
    }
    return nativeImage.createFromBitmap(pixels, { width: size, height: size }).toPNG().toString('base64')
  }, [size, colour, noise])
  /** Put a PNG, and optionally text beside it, on the operating system's clipboard. */
  const copy = (png, text) => app.evaluate(async ({ clipboard, ClipboardItem }, [png, text]) => {
    const entry = { 'image/png': new Blob([Buffer.from(png, 'base64')], { type: 'image/png' }) }
    if (text) entry['text/plain'] = text
    await clipboard.write([new ClipboardItem(entry)])
  }, [png, text])
  const copyText = (text) => app.evaluate(({ clipboard }, text) => clipboard.writeText(text), text)
  /** The menu's Paste: `webContents.paste()` on the focused window. */
  const menuPaste = () => app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].webContents.paste())

  const blue = await picture(64, [0x20, 0x60, 0xd0, 0xff])
  const green = await picture(64, [0x20, 0xb0, 0x60, 0xff])
  const dropped = join(desktop, 'shot.png')
  writeFileSync(dropped, Buffer.from(await picture(48, [0xd0, 0x60, 0x20, 0xff]), 'base64'))

  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await trust.getByRole('button', { name: 'Trust this directory' }).click()
  await trust.waitFor({ state: 'hidden' })

  const composer = page.getByRole('textbox', { name: 'Message the agent' })
  const pastedChips = page.locator('[data-test="pasted-chip"]')
  const droppedChips = page.locator('[data-test="dropped-chip"]')
  const stop = page.locator('.composer .stop')
  const cdp = await page.context().newCDPSession(page)
  const keyPaste = async () => {
    const modifiers = process.platform === 'darwin' ? 4 : 2
    const key = { key: 'v', code: 'KeyV', windowsVirtualKeyCode: 86, modifiers, commands: ['paste'] }
    await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...key })
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...key })
  }
  const drag = async (files) => {
    const box = await page.locator('.composer textarea').first().boundingBox()
    const x = Math.round(box.x + box.width / 2), y = Math.round(box.y + box.height / 2)
    const data = { items: [], files, dragOperationsMask: 1 }
    await cdp.send('Input.dispatchDragEvent', { type: 'dragEnter', x, y, data })
    for (let i = 0; i < 3; i++) {
      await cdp.send('Input.dispatchDragEvent', { type: 'dragOver', x, y, data })
      await page.waitForTimeout(100)
    }
    await cdp.send('Input.dispatchDragEvent', { type: 'drop', x, y, data })
  }

  // ---- a drop and then a paste, in one draft, share the counter -------------------------------
  await composer.fill('Compare ')
  await drag([dropped])
  await droppedChips.first().waitFor()
  await copy(blue, 'https://example.com/the-page-the-picture-was-on')
  await composer.focus()
  await menuPaste()
  await pastedChips.first().waitFor()
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] ', 'the paste is numbered after the drop, at the caret, and the text beside the picture did not land')
  assert.equal(await page.locator('[data-test="pasted-chip"] img.attachment-thumb').count(), 1, 'the pasted picture has a thumbnail')
  assert.match(await page.locator('[data-test="pasted-chip"]').innerText(), /Pasted image[\s\S]*\[Image #2\]/)
  assert.match(await page.locator('.attachment-trust').innerText(), /Sent as trusted context/)
  await page.locator('[data-test="composer-mode"]').click()
  assert.equal(await page.locator('[data-test="mode-plan"]').getAttribute('aria-disabled'), 'true', 'Plan is off while a paste is staged')
  await page.locator('[data-test="composer-mode"]').click()
  await page.locator('[data-test="mode-plan"]').waitFor({ state: 'hidden' })
  assert.ok(!(await page.evaluate(() => document.body.innerHTML)).includes(blue.slice(0, 200)), 'the page never holds the pasted bytes')
  await page.screenshot({ path: join(output, 'paste-staged-light.png') })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'paste-staged-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })

  // ---- the keyboard's paste command takes the same path, and both come off two ways ------------
  await copy(green)
  await keyPaste()
  await pastedChips.nth(1).waitFor()
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] [Image #3] ')
  await page.getByRole('button', { name: 'Remove [Image #3] Pasted image' }).click()
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] ', 'removing the chip removes its marker')
  await composer.press('End')
  await keyPaste()
  await pastedChips.nth(1).waitFor()
  await composer.fill((await composer.inputValue()).replace('[Image #4] ', ''))
  assert.equal(await pastedChips.count(), 1, 'deleting the marker takes the picture off')

  // ---- text alone is the browser's paste, as before -------------------------------------------
  await copyText('plain words')
  await composer.press('End')
  await menuPaste()
  await page.waitForTimeout(500)
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] plain words', 'a text paste lands as text and stages nothing')
  assert.equal(await pastedChips.count(), 1)
  await composer.fill('Compare [Image #1] [Image #2] ')

  // ---- a paste the page made up does nothing ---------------------------------------------------
  await copy(green)
  const forged = await page.evaluate(async () => {
    const transfer = new DataTransfer()
    transfer.items.add(new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'forged.png', { type: 'image/png' }))
    const field = document.querySelector('.composer leo-textarea, .composer textarea')
    field.dispatchEvent(new ClipboardEvent('paste', { clipboardData: transfer, bubbles: true, cancelable: true, composed: true }))
    field.focus()
    const command = document.execCommand('paste')
    return { command, api: Object.keys(window.bravebot).filter((key) => /stage|paste/i.test(key)) }
  })
  await page.waitForTimeout(800)
  assert.deepEqual(forged.api, ['onPaste'], 'the page has no way to stage a paste')
  assert.equal(forged.command, false, 'the page cannot run the browser’s paste command')
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] ', 'an untrusted paste event stages nothing')
  assert.equal(await pastedChips.count(), 1)

  // ---- a picture over 10 MiB is refused with its size and the limit ---------------------------
  await copy(await picture(2000, null, true))
  await composer.focus()
  await menuPaste()
  const note = page.locator('[data-test="status-toast"]').filter({ hasText: 'Too large to paste' })
  await note.waitFor()
  assert.match(await note.innerText(), /That picture is 1\d\.\d MB, and a paste carries at most 10\.0 MB\./)
  assert.equal(await pastedChips.count(), 1, 'nothing was staged')
  assert.equal(await composer.inputValue(), 'Compare [Image #1] [Image #2] ')
  await page.screenshot({ path: join(output, 'paste-too-large.png') })

  // ---- sending carries what the draft still names, and a queued message keeps its own ---------
  const sentBefore = requests.length
  await page.locator('.composer .send').click()
  await stop.waitFor({ state: 'visible' })
  assert.equal(await pastedChips.count() + await droppedChips.count(), 0, 'sending clears what was staged')
  await copy(blue)
  await composer.focus()
  await menuPaste()
  await pastedChips.first().waitFor()
  assert.match(await composer.inputValue(), /^\[Image #5\] $/)
  await composer.fill('Queued look at [Image #5] ')
  await composer.press('Enter')
  await page.locator('.queued-messages').waitFor()
  assert.equal(await pastedChips.count(), 0, 'queuing takes the paste with the message')
  await page.waitForFunction(() => document.querySelector('.queued-messages') === null, null, { timeout: 60000 })
  await stop.waitFor({ state: 'visible' })
  await stop.waitFor({ state: 'hidden', timeout: 60000 })

  const sent = requests.slice(sentBefore)
  const userPart = (body, said) => {
    const messages = JSON.parse(body).messages.filter((message) => message.role === 'user')
    return messages.find((message) => JSON.stringify(message.content).includes(said))
  }
  const urls = (message) => (Array.isArray(message.content) ? message.content : []).filter((part) => part.type === 'image_url').map((part) => part.image_url.url)
  const first = sent.map((body) => userPart(body, 'Compare [Image #1] [Image #2]')).find(Boolean)
  assert.ok(first, 'the prompt reached the model with its markers')
  const pictures = urls(first)
  assert.equal(pictures.length, 2, 'the dropped and the pasted picture both went with the prompt')
  assert.ok(pictures.every((url) => url.startsWith('data:image/png;base64,')), 'both as PNG data')
  const sentPaste = await app.evaluate(({ nativeImage }, url) => {
    const image = nativeImage.createFromDataURL(url)
    const bitmap = image.toBitmap()
    return { size: image.getSize(), pixel: [...bitmap.subarray(0, 4)] }
  }, pictures[1])
  assert.deepEqual(sentPaste.size, { width: 64, height: 64 }, 'the second picture is the one pasted')
  const queued = sent.map((body) => userPart(body, 'Queued look at [Image #5]')).find(Boolean)
  assert.ok(queued, 'the queued message was sent when the turn ended')
  assert.equal(urls(queued).length, 1, 'with the picture it was queued with')
  assert.ok(!sent.some((body) => body.includes('the-page-the-picture-was-on')), 'the text beside the picture was never sent')
  const rows = await page.locator('.entries').innerText()
  assert.match(rows, /Compare \[Image #1\] \[Image #2\]/, 'the transcript keeps the markers')
  assert.equal(await page.locator('.entries img[src^="data:"]').count(), 0, 'and draws no pasted bytes')
  assert.deepEqual(errors, [])

  // ---- reopened, the prompt shows its words and the picture still goes to the model -----------
  await restoreClipboard(app)
  await app.close()
  app = await launch()
  await app.evaluate(async ({ clipboard }) => { globalThis.keptClipboard = await clipboard.read().catch(() => []) })
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
  await page.locator('.session-row .session').filter({ hasText: 'Compare [Image #1]' }).first().click()
  const again = page.getByRole('dialog', { name: 'Project trust', exact: true })
  if (await again.isVisible().catch(() => false)) {
    await again.getByRole('button', { name: 'Trust this directory' }).click()
    await again.waitFor({ state: 'hidden' })
  }
  await page.locator('.entries').getByText('Compare [Image #1] [Image #2]').first().waitFor()
  assert.equal(await page.locator('.entries img[src^="data:"]').count(), 0, 'a reopened transcript draws no bytes')
  const resumedBefore = requests.length
  const reopened = page.getByRole('textbox', { name: 'Message the agent' })
  await reopened.fill('What was in them?')
  await page.locator('.composer .send').click()
  await page.locator('.composer .stop').waitFor({ state: 'hidden', timeout: 60000 })
  const resumed = requests.slice(resumedBefore).find((body) => body.includes('What was in them?'))
  assert.ok(resumed, 'the follow-up reached the model')
  assert.equal(urls(userPart(resumed, 'Compare [Image #1] [Image #2]')).length, 2, 'the reopened conversation still carries both pictures')
  await page.screenshot({ path: join(output, 'paste-reopened.png') })

  console.log(`PASS: a trusted paste stages a marker and a chip numbered with drops, a forged paste does nothing, an oversized one says so, and the send carries the picture as PNG data. Screenshots in ${output}/paste-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'paste-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await restoreClipboard(app).catch(() => undefined)
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
