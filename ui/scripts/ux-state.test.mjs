import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, readFileSync, rmSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
const require = createRequire(import.meta.url)
function load(path, electron = {}) {
  electron = { ...electron, app: { getAppPath: () => process.cwd(), ...electron.app } }
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => id === 'electron' ? electron : require(id), module, module.exports)
  return module.exports
}

test('turn notices precede early worker activity and markers do not duplicate', () => {
  const t = load('src/renderer/transcript.ts')
  const entries = [t.userSaid('Do the work'), t.narrated('Already thinking')]
  const begun = t.beginTurn(entries, 2)
  assert.deepEqual(begun.map(entry => entry.kind), ['user', 'turn-start', 'narration'])
  assert.equal(t.beginTurn(begun, 2), begun)
  const next = t.beginTurn([...begun, t.replied('Done', 2), t.consolidating()], 3)
  assert.equal(next.at(-1).number, 3)
  assert.deepEqual(t.conversation(next), [{ role: 'user', text: 'Do the work' }, { role: 'assistant', text: 'Done' }], 'metadata does not create exported messages')
})

test('drafts, archives and pins survive process reload and unrelated preference writes', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-state-'))
  const electron = { app: { getPath: () => directory } }
  try {
    const key = JSON.stringify(['/project/a', 'session-a'])
    const state = load('src/main/experience.ts', electron)
    state.writeExperience(key, { draft: 'An unsent prompt\nwith code', scroll: 120, pinned: true, archived: true })
    state.writeExperience('density', 'compact')
    state.writeExperience('recentModels', ['one', 'one', 'two'])
    const fresh = load('src/main/experience.ts', electron).readExperience()
    assert.deepEqual(fresh.conversations[key], { botSlug: null, draft: 'An unsent prompt\nwith code', scroll: 120, pinned: true, archived: true })
    assert.equal(fresh.density, 'compact')
    assert.deepEqual(fresh.recentModels, ['one', 'two'])
    assert.throws(() => state.writeExperience('../../outside', {}))
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('project search finds files in unopened folders; preview rejects traversal and escaping symlinks', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-files-'))
  const root = join(directory, 'project')
  mkdirSync(join(root, 'src', 'deep'), { recursive: true })
  writeFileSync(join(root, 'src', 'deep', 'example.ts'), 'const answer = 42\n')
  writeFileSync(join(directory, 'outside.txt'), 'must not cross')
  symlinkSync(join(directory, 'outside.txt'), join(root, 'escape'))
  symlinkSync(directory, join(root, 'escaped-directory'))
  try {
    const files = load('src/main/files.ts', { shell: {} })
    files.noteRoot('test-session', root)
    assert.deepEqual((await files.search('test-session', 'example', false)).paths, ['src/deep/example.ts'])
    assert.equal(files.preview('test-session', 'src/deep/example.ts').text, 'const answer = 42\n')
    for (const path of ['../outside.txt', '/etc/passwd', 'escape', 'escaped-directory/outside.txt']) assert.equal(files.preview('test-session', path), null)
    assert.equal(files.preview('unknown-session', 'src/deep/example.ts'), null)
    assert.equal((await files.search('test-session', 'outside', false)).paths.length, 0)
    writeFileSync(join(root, 'binary'), Buffer.from([0, 1, 2]))
    assert.equal(files.preview('test-session', 'binary'), null)
    writeFileSync(join(root, 'large'), 'x'.repeat(150000))
    assert.equal(files.preview('test-session', 'large').truncated, true)
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('memory edits use compare-and-replace and preserve recoverable history', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-memory-'))
  const profile = join(directory, 'profile'), project = join(directory, 'project')
  mkdirSync(profile); mkdirSync(project)
  const electron = { app: { getPath: () => profile } }
  try {
    const bots = load('src/main/bots.ts', electron)
    const { parseBots } = load('src/shared/bots.ts')
    bots.saveBot(parseBots({ bots: [{ slug: 'test', name: 'Test', purpose: 'Test', directory: project, avatar: 'test' }] }).bots[0])
    const memory = load('src/main/memory.ts', electron)
    assert.equal(memory.editMemory('test', 'First memory', null), 'First memory')
    assert.throws(() => memory.editMemory('test', 'Lost edit', null), /changed/)
    assert.equal(memory.editMemory('test', 'Second memory', 'First memory'), 'Second memory')
    assert.deepEqual(memory.memoryHistory('test').map((entry) => entry.text), ['First memory', 'Second memory'])
    memory.editMemory('test', '', 'Second memory')
    assert.equal(readFileSync(join(project, '.bravebot-ui', 'bots', 'test.md'), 'utf8'), '')
    assert.equal(memory.memoryHistory('test').at(-2).text, 'Second memory')
    writeFileSync(join(profile, 'bots', 'test', 'ground.md'), 'private briefing')
    memory.removeMemoryHistory('test')
    assert.deepEqual(memory.memoryHistory('test'), [])
    assert.equal(existsSync(join(profile, 'bots', 'test', 'ground.md')), false)
    assert.equal(existsSync(join(profile, 'bots', 'test', 'memory-history.json')), false)
    assert.equal(readFileSync(join(project, '.bravebot-ui', 'bots', 'test.md'), 'utf8'), '')

  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('write status follows execution outcome even when its tool row precedes the approval', () => {
  const { written } = load('src/renderer/components/Context.tsx')
  const approval = { kind: 'confirm', id: 'approval', request: { path: 'a.ts' }, decision: 'approve' }
  const tool = (note, failed = false) => ({ kind: 'tool', id: 'tool', activity: { verb: 'Write', target: 'a.ts', note, failed, changes: [] } })
  assert.equal(written([approval])[0].state, 'approved')
  assert.equal(written([tool(null), approval])[0].state, 'applying')
  assert.equal(written([tool('done'), approval])[0].state, 'applied')
  assert.equal(written([tool('disk full', true), approval])[0].state, 'failed')
  assert.equal(written([tool('done'), approval, tool(null)])[0].state, 'applying')
})


test('attachment grants are per session and revalidate changed files at send', async () => {
  const root = mkdtempSync(join(tmpdir(), 'bravebot-ux-attachment-'))
  const selected = join(root, 'notes.txt')
  writeFileSync(selected, 'Explicit review context')
  const files = load('src/main/files.ts', { dialog: { showOpenDialog: async () => ({ canceled: false, filePaths: [selected] }) } })
  files.noteRoot('a', root); files.noteRoot('b', root)
  try {
    const [file] = await files.chooseAttachments({}, 'a')
    assert.deepEqual(files.attachmentPaths('a', [file.id]), ['notes.txt'])
    assert.throws(() => files.attachmentPaths('b', [file.id]))
    assert.throws(() => files.attachmentPaths('a', ['notes.txt']))
    writeFileSync(selected, Buffer.from([0, 1, 2]))
    assert.throws(() => files.attachmentPaths('a', [file.id]))
    await assert.rejects(files.chooseAttachments({}, 'a'))
    writeFileSync(selected, 'x'.repeat(300000))
    assert.throws(() => files.attachmentPaths('a', [file.id]))
    writeFileSync(selected, 'Text again')
    files.forgetRoot('a')
    assert.throws(() => files.attachmentPaths('a', [file.id]))
  } finally { rmSync(root, { recursive: true, force: true }) }
})


test('diff line numbers account for elided spans, insertions and deletions', () => {
  const { numberedDiffLines, searchableText } = load('src/renderer/transcript.ts')
  const lines = numberedDiffLines([{ kind: 'elided', lines: 40 }, { kind: 'removed', text: 'old' }, { kind: 'added', text: 'new' }, { kind: 'added', text: 'extra' }, { kind: 'kept', text: 'tail' }])
  assert.deepEqual(lines.map(({ before, after }) => [before, after]), [[null, null], [41, null], [null, 41], [null, 42], [42, 43]])
  assert.equal(searchableText({ kind: 'user', id: 'internal-secret-id', text: 'Visible prompt' }), 'Visible prompt')
})


test('ending a turn invalidates pending approvals without changing prior decisions', () => {
  const { interruptPending, outstanding } = load('src/renderer/transcript.ts')
  const entries = [{ kind: 'confirm', id: 'past', request: { request: 1 }, decision: 'approve' }, { kind: 'ask', id: 'pending', request: { request: 2 }, answers: null }]
  assert.equal(outstanding(entries).id, 'pending')
  const stopped = interruptPending(entries)
  assert.equal(outstanding(stopped), null)
  assert.equal(stopped[0], entries[0])
  assert.equal(stopped[1].interrupted, true)
})


test('request IDs reused in later turns never rewrite prior answers or approvals', () => {
  const { answered, decide } = load('src/renderer/transcript.ts')
  const firstAsk = { kind: 'ask', id: 'first', request: { request: 1 }, answers: [{ typed: 'Blue' }] }
  const nextAsk = { kind: 'ask', id: 'next', request: { request: 1 }, answers: null }
  const answers = answered([firstAsk, nextAsk], 1, [{}])
  assert.equal(answers[0], firstAsk)
  assert.deepEqual(answers[1].answers, [{}])
  const firstWrite = { kind: 'confirm', id: 'first-write', request: { request: 1 }, decision: 'approve' }
  const nextWrite = { kind: 'confirm', id: 'next-write', request: { request: 1 }, decision: null }
  const approvals = decide([firstWrite, nextWrite], 'confirm', 1, 'reject')
  assert.equal(approvals[0], firstWrite)
  assert.equal(approvals[1].decision, 'reject')
})


test('built-in accent and message foregrounds meet normal-text contrast', () => {
  const { BUILTINS, roleVariables } = load('src/shared/theme.ts')
  const luminance = (hex) => {
    const linear = hex.slice(1).match(/../g).map((channel) => parseInt(channel, 16) / 255).map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
    return .2126 * linear[0] + .7152 * linear[1] + .0722 * linear[2]
  }
  for (const theme of BUILTINS) for (const dark of [false, true]) {
    const vars = roleVariables(theme, dark)
    for (const role of ['note', 'primary']) {
      const a = luminance(vars[`--role-${role}`]), b = luminance(vars[`--role-${role}-ink`])
      assert.ok((Math.max(a, b) + .05) / (Math.min(a, b) + .05) >= 4.5, `${theme.name} ${role}`)
    }
  }
})


test('reference-backed writes and approval paths share one execution outcome', () => {
  const { written } = load('src/renderer/components/Context.tsx')
  const tool = { kind: 'tool', id: 'tool', activity: { verb: 'Write', target: 'ref:3(U,priv):src/sample.txt', note: 'done', failed: false, changes: [] } }
  const approval = { kind: 'confirm', id: 'approval', request: { path: 'src/sample.txt' }, decision: 'approve' }
  assert.deepEqual(written([tool, approval]), [{ target: 'src/sample.txt', state: 'applied' }])
  assert.equal(written([{ ...approval, decision: null, interrupted: true }])[0].state, 'cancelled')
})

test('bot history includes every saved, associated and draft conversation without mixing bots or projects', () => {
  const { botHistory } = load('src/shared/bot-history.ts')
  const { parseExperience, conversationKey } = load('src/shared/experience.ts')
  const bot = { slug: 'review', directory: '/project', session: 'new', conversations: ['old', 'new', 'missing'] }
  const row = (id, updated = 1, directory = '/project') => ({ id, directory, title: id, updated, project: 'project', branch: null, bytes: 0 })
  const experience = parseExperience({ conversations: {
    [conversationKey('/project', 'old')]: { archived: true },
    [conversationKey('/project', 'associated')]: { botSlug: 'review' },
    [conversationKey('/project', 'draft:pending')]: { botSlug: 'review', draft: 'unsent work' },
    [conversationKey('/elsewhere', 'foreign')]: { botSlug: 'review' },
    [conversationKey('/project', 'other-bot')]: { botSlug: 'another' },
    [conversationKey('/project', 'draft:migrated')]: { botSlug: 'review', draft: '' },
  } })
  const history = botHistory(bot, [row('old'), row('new', 2), row('associated', 3), row('draft:pending', 4), row('other-bot'), row('foreign', 5, '/elsewhere')], experience)
  assert.deepEqual(history.map(row => row.id), ['draft:pending', 'associated', 'new', 'old', 'missing'])
  assert.equal(history.find(row => row.id === 'old').archived, true)
  assert.equal(history.at(-1).session, null)
})

test('starting and revisiting bot conversations preserves all earlier IDs across restart', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-history-'))
  const electron = { app: { getPath: () => directory } }
  const ids = ['11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333']
  try {
    const { parseBots } = load('src/shared/bots.ts')
    const storage = load('src/main/bots.ts', electron)
    storage.saveBot(parseBots({ bots: [{slug:'review', name:'Review', purpose:'Review', directory:'/project', session:ids[0]}] }).bots[0])
    storage.noteBotSession('review', ids[1])
    storage.noteBotSession('review', ids[2])
    storage.noteBotSession('review', ids[0])
    storage.releaseBotSession('review')
    const reopened = load('src/main/bots.ts', electron).bot('review')
    assert.deepEqual(reopened.conversations, ids)
    assert.equal(reopened.session, null)
  } finally { rmSync(directory, {recursive:true, force:true}) }
})
