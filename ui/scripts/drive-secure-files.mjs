// Real main-process IPC and file helper; isolated profile, no model calls.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, symlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { _electron as electron } from 'playwright-core'

const area = mkdtempSync(join(tmpdir(), 'bravebot-secure-files-'))
const profile = join(area, 'profile'), project = join(area, 'project')
mkdirSync(profile); mkdirSync(project)
writeFileSync(join(project, 'notes.txt'), 'Project-only fixture')
writeFileSync(join(area, 'outside.txt'), 'Outside sentinel')
symlinkSync(join(area, 'outside.txt'), join(project, 'escape.txt'))
const executablePath = process.env.SECURE_FILES_APP
const app = await electron.launch({
  ...(executablePath ? { executablePath: resolve(executablePath) } : {}),
  args: [...(executablePath ? [] : ['.']), `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000,
})
try {
  const page = await app.firstWindow()
  await page.waitForFunction(() => !!window.bravebot)
  const result = await page.evaluate(async directory => {
    const response = await window.bravebot.request('session.new', {directory})
    if (!response.ok) throw new Error(JSON.stringify(response.error))
    const handle = response.ok.session
    const preview = await window.bravebot.previewFile(handle, 'notes.txt')
    const escaped = await window.bravebot.previewFile(handle, 'escape.txt')
    const traversal = await window.bravebot.previewFile(handle, '../outside.txt')
    const bot = await window.bravebot.writeBot({name:'Security fixture', purpose:'Disposable test', directory})
    const first = await window.bravebot.editBotMemory(bot.slug, 'First memory', null)
    let conflict = false
    try { await window.bravebot.editBotMemory(bot.slug, 'Wrong edit', null) } catch { conflict = true }
    const second = await window.bravebot.editBotMemory(bot.slug, 'Second memory', first)
    const memory = await window.bravebot.readBotMemory(bot.slug)
    const history = await window.bravebot.readMemoryHistory(bot.slug)
    await window.bravebot.removeBot(bot.slug)
    const replacement = await window.bravebot.writeBot({name:'Security fixture', purpose:'New bot', directory})
    const inherited = await window.bravebot.readMemoryHistory(replacement.slug)
    await window.bravebot.request('session.close', {session:handle})
    return {preview, escaped, traversal, conflict, second, memory, history, inherited, slug:bot.slug}
  }, project)
  assert.equal(result.preview.text, 'Project-only fixture')
  assert.equal(result.escaped, null)
  assert.equal(result.traversal, null)
  assert.equal(result.conflict, true)
  assert.equal(result.memory, 'Second memory')
  assert.deepEqual(result.history.map(row => row.text), ['First memory', 'Second memory'])
  assert.deepEqual(result.inherited, [])
  assert.equal(existsSync(join(profile, 'bots', result.slug, 'memory-history.json')), false)
  assert.equal(readFileSync(join(project, '.bravebot-ui', 'bots', `${result.slug}.md`), 'utf8'), 'Second memory')
  assert.equal(readFileSync(join(area, 'outside.txt'), 'utf8'), 'Outside sentinel')
  console.log(`PASS: ${executablePath ? 'packaged' : 'development'} secure preview, memory replacement, conflict handling and history deletion`)
} finally { await app.close() }
