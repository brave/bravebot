// The note at the top of a bot's memory file, which earlier versions wrapped at about 90 columns.
//
// Run after `cargo build`: tidying goes through the real `bravebot-ui-files` helper.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const require = createRequire(import.meta.url)
function load(contents, userData) {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => userData, getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)(id => id === 'electron' ? electron : require(id), module, module.exports)
  return module.exports
}

const WRAPPED = [
  'Written by the bot itself, and shown in the transcript each time it changes. Anything here',
  'is carried into every conversation it has; anything not here is forgotten when the',
  'conversation is compacted.',
].join('\n')
const JOINED = WRAPPED.replaceAll('\n', ' ')

const scratch = (name) => realpathSync(mkdtempSync(join(tmpdir(), `bravebot-${name}-`)))

function fixture(name) {
  const userData = scratch(`${name}-app`)
  const home = join(userData, 'bot-homes', 'custodian')
  mkdirSync(home, { recursive: true })
  const api = load("export * from './src/main/bots'; export * from './src/main/memory'; export { parseBots } from './src/shared/bots'", userData)
  const bot = api.parseBots({ bots: [{ slug: 'custodian', name: 'Custodian', purpose: 'Keep the harbour lights lit', home, avatar: 'v2:test', session: null }] }).bots[0]
  api.saveBot(bot)
  const file = join(home, '.bravebot-ui', 'bots', 'custodian.md')
  mkdirSync(join(home, '.bravebot-ui', 'bots'), { recursive: true })
  return { api, bot, home, file, clean: () => rmSync(userData, { recursive: true, force: true }) }
}

test('a wrapped note is joined and the rest of the text is carried as it is', () => {
  const { api, clean } = fixture('note-pure')
  try {
    const rest = '\n\nNothing remembered yet.\n\n- the lamp on the east pier is out\n'
    assert.equal(api.unwrapMemoryNote(`# Custodian memory\n\n${WRAPPED}${rest}`), `# Custodian memory\n\n${JOINED}${rest}`)
  } finally { clean() }
})

test('a note the bot or a person has changed is not touched', () => {
  const { api, clean } = fixture('note-changed')
  try {
    assert.equal(api.unwrapMemoryNote(JOINED), null, 'already joined')
    assert.equal(api.unwrapMemoryNote(WRAPPED.replace('Anything here', 'Everything here')), null, 'reworded')
    assert.equal(api.unwrapMemoryNote(WRAPPED.replace('\nis carried', ' \nis carried')), null, 'spacing changed')
    assert.equal(api.unwrapMemoryNote('# Custodian\n\nNothing yet.\n'), null, 'no note')
  } finally { clean() }
})

test('a new memory file carries the note on one line', () => {
  const { api, bot, home, file, clean } = fixture('note-new')
  try {
    api.ground(bot, home)
    const written = readFileSync(file, 'utf8')
    assert.ok(written.includes(JOINED), 'the note is one line')
    assert.ok(!written.includes(WRAPPED), 'and not wrapped')
  } finally { clean() }
})

test('reading tidies an old file once, keeps the old version in history, and leaves a changed one alone', () => {
  const { api, bot, home, file, clean } = fixture('note-tidy')
  try {
    const old = `# Custodian memory\n\n${WRAPPED}\n\n- the lamp on the east pier is out\n`
    writeFileSync(file, old, 'utf8')
    api.tidyMemory(bot.slug, home)
    assert.equal(readFileSync(file, 'utf8'), `# Custodian memory\n\n${JOINED}\n\n- the lamp on the east pier is out\n`)
    assert.deepEqual(api.memoryHistory(bot.slug, home).map((entry) => entry.text), [old], 'the version it replaced can be restored')
    api.tidyMemory(bot.slug, home)
    assert.equal(api.memoryHistory(bot.slug, home).length, 1, 'a second read changes nothing')

    const changed = old.replace('Anything here', 'Everything here')
    writeFileSync(file, changed, 'utf8')
    api.tidyMemory(bot.slug, home)
    assert.equal(readFileSync(file, 'utf8'), changed)
  } finally { clean() }
})

test('tidying a file does not count as the bot having written', () => {
  const { api, bot, home, file, clean } = fixture('note-mark')
  try {
    writeFileSync(file, `# Custodian memory\n\n${WRAPPED}\n`, 'utf8')
    api.noteBotMemory(bot.slug, home)
    const marked = api.bot(bot.slug)
    assert.ok(marked.remembered > 0, 'the mark follows the file')
    api.saveBot({ ...marked, quiet: 2 })
    api.tidyMemory(bot.slug, home)
    api.noteBotMemory(bot.slug, home)
    assert.equal(api.bot(bot.slug).quiet, 3, 'the turn after a tidy still counts as a quiet one')
  } finally { clean() }
})
