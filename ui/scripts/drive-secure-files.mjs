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
    // A bot keeps a memory in each folder it works in; a new one has only its home.
    const bot = await window.bravebot.writeBot({name:'Security fixture', purpose:'Disposable test'})
    // A folder the bot never worked in is not one a window may point the memory editor at.
    let foreign = false
    try { await window.bravebot.editBotMemory(bot.slug, directory, 'Foreign memory', null) } catch { foreign = true }
    const first = await window.bravebot.editBotMemory(bot.slug, bot.home, 'First memory', null)
    let conflict = false
    try { await window.bravebot.editBotMemory(bot.slug, bot.home, 'Wrong edit', null) } catch { conflict = true }
    const second = await window.bravebot.editBotMemory(bot.slug, bot.home, 'Second memory', first)
    const memory = await window.bravebot.readBotMemory(bot.slug, bot.home)
    const history = await window.bravebot.readMemoryHistory(bot.slug, bot.home)
    await window.bravebot.removeBot(bot.slug)
    const replacement = await window.bravebot.writeBot({name:'Security fixture', purpose:'New bot'})
    const inherited = await window.bravebot.readMemoryHistory(replacement.slug, replacement.home)
    await window.bravebot.request('session.close', {session:handle})
    return {preview, escaped, traversal, foreign, conflict, second, memory, history, inherited, slug:bot.slug, home:bot.home}
  }, project)
  assert.equal(result.preview.text, 'Project-only fixture')
  assert.equal(result.escaped, null)
  assert.equal(result.traversal, null)
  assert.equal(result.foreign, true, 'a folder the bot never worked in is refused')
  assert.equal(existsSync(join(project, '.bravebot-ui')), false, 'and nothing is written there')
  assert.equal(result.conflict, true)
  assert.equal(result.memory, 'Second memory')
  assert.deepEqual(result.history.map(row => row.text), ['First memory', 'Second memory'])
  assert.deepEqual(result.inherited, [])
  assert.equal(existsSync(join(profile, 'bots', result.slug)), false, 'deleting the bot removes every memory history')
  assert.equal(readFileSync(join(result.home, '.bravebot-ui', 'bots', `${result.slug}.md`), 'utf8'), 'Second memory')
  assert.equal(readFileSync(join(area, 'outside.txt'), 'utf8'), 'Outside sentinel')
  console.log(`PASS: ${executablePath ? 'packaged' : 'development'} secure preview, memory replacement, conflict handling and history deletion`)
} finally { await app.close() }
