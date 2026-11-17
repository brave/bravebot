// The questions a chat asks once its trust question is answered: whether to grant the allow rules
// a checkout's settings propose (PERM-15), and whether to open each directory they name (TRUST-9).
//
// The bridge decides; the window draws the question it says is out, answers it by request id with
// a yes or a no, and draws what the answer says comes next. These tests hold the window to that:
// the grant waits until every rule has been in view, an answer names no rule or path back, each
// answer chains to the next question, a stale answer re-reads the state once and never loops, and
// the folded note says what was granted and what is still not in force.

import test from 'node:test'
import assert from 'node:assert/strict'
import { build, buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { withoutModal, plainLeo } from './shown-markup.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

const options = (path) => ({
  entryPoints: [path],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  external: ['react', 'react-dom', 'react/jsx-runtime'],
})
function evaluate(source) {
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const load = (path) => evaluate(buildSync(options(path)).outputFiles[0].text)

// The dialog frame wants a document, and Leo's elements draw nothing in a server render: both are
// swapped for plain markup, as the trust question's tests do.
const { RulesPrompt, FolderPrompt } = evaluate((await build({ ...options('src/renderer/components/ProjectQuestions.tsx'), plugins: [withoutModal, plainLeo] })).outputFiles[0].text)
const { RulesBanner, SessionNotices } = load('src/renderer/components/Transcript.tsx')
const questions = load('src/renderer/project-questions.ts')
const { Ledger } = load('src/renderer/shown.ts')
const { canSend, askingFirst } = load('src/renderer/can-send.ts')
const { settledBy } = load('src/renderer/live.ts')

const render = (component, props) => renderToStaticMarkup(React.createElement(component, props))
const FILE = '/home/someone/project/.bravebot/settings.json'
const rule = (text) => ({ rule: text, shownRule: text, file: FILE, shownFile: FILE })

const rulesQuestion = (overrides = {}) => ({
  request: 7,
  rules: [rule('Bash(cargo test)'), rule('Edit(src/**)')],
  grantedBefore: [],
  keeping: '/home/someone/.bravebot/granted/abc.json',
  shownKeeping: '~/.bravebot/granted/abc.json',
  ...overrides,
})
const folderQuestion = (overrides = {}) => ({
  request: 8,
  directory: '/home/someone/shared',
  shownDirectory: '/home/someone/shared',
  named: ['../shared'],
  shownNamed: ['../shared'],
  remaining: 1,
  ...overrides,
})
const NONE = { deny: [], ask: [], allow: [], unreadable: [], proposed: [], directories: [] }
const rules = (overrides = {}) => ({ ...NONE, ...overrides })

// ---------------------------------------------------------------- the rules question

test('the rules question lists every rule with the file that proposed it, and where a yes is kept', () => {
  const markup = render(RulesPrompt, { question: rulesQuestion(), onAnswer: () => {} })
  assert.ok(markup.includes('<code>Bash(cargo test)</code>'), markup)
  assert.ok(markup.includes('<code>Edit(src/**)</code>'), markup)
  assert.equal(markup.split(`from ${FILE}`).length - 1, 1, 'a run of rules from one file names it once, above them')
  const other = '/home/someone/project/.bravebot/settings.local.json'
  const two = render(RulesPrompt, { question: rulesQuestion({ rules: [rule('Bash(ls)'), { ...rule('Bash(pwd)'), file: other, shownFile: other }, rule('Bash(date)')] }), onAnswer: () => {} })
  assert.equal(two.split('data-test="rules-file"').length - 1, 3, 'each rule sits under the file that proposed it, in the order listed')
  assert.ok(two.includes(`from ${other}`), two)
  assert.ok(markup.includes('~/.bravebot/granted/abc.json'), 'the record a yes is kept in is named')
  assert.ok(markup.includes('Grant 2 rules'), markup)
  assert.ok(markup.includes('Keep asking me'), markup)
})

test('every rule row is one the grant waits on, and the grant takes no press before it is measured', () => {
  const markup = render(RulesPrompt, { question: rulesQuestion({ rules: Array.from({ length: 30 }, (_, i) => rule(`Bash(step-${i})`)) }), onAnswer: () => {} })
  const rows = markup.match(/<li data-deciding="all" data-test="rules-row">/g) ?? []
  assert.equal(rows.length, 30, 'each rule is a row counted as read only once in view')
  const grant = markup.match(/<[^>]*data-test="rules-grant"[^>]*>/)?.[0] ?? ''
  assert.ok(/\bdisabled\b/.test(grant) && /aria-disabled="true"/.test(grant), grant || markup)
  const decline = markup.match(/<[^>]*data-test="rules-decline"[^>]*>/)?.[0] ?? ''
  assert.ok(decline && !/disabled/.test(decline), `Keep asking me is always open: ${decline}`)
})

test('the grant opens only once every rule row has been in view: a short list at once, a long one after scrolling', () => {
  // The ledger is what the gate counts with: a row not yet in view is left, and a row scrolled past stays read.
  const width = 400
  const row = (seen) => ({ width, seen })
  const short = new Ledger()
  assert.equal(short.take(width, [{ key: 'list', standing: false, hidden: false, rows: [row([0, 400]), row([0, 400])] }]).left, 0, 'a list that fits counts as seen')

  const long = new Ledger()
  const first = Array.from({ length: 20 }, (_, i) => row(i < 8 ? [0, 400] : null))
  assert.equal(long.take(width, [{ key: 'list', standing: false, hidden: false, rows: first }]).left, 12, 'rows below the box are not read yet')
  const scrolled = Array.from({ length: 20 }, (_, i) => row(i >= 8 ? [0, 400] : null))
  assert.equal(long.take(width, [{ key: 'list', standing: false, hidden: false, rows: scrolled }]).left, 0, 'scrolled through, every row has been read')
})

test('a yes that lasts for this chat only says so, and rules granted before are noted', () => {
  const markup = render(RulesPrompt, {
    question: rulesQuestion({ keeping: null, shownKeeping: null, grantedBefore: [rule('Read(docs/**)')] }),
    onAnswer: () => {},
  })
  assert.ok(markup.includes('for this chat only'), markup)
  assert.ok(markup.includes('1 rule from these files was granted before'), markup)
  assert.ok(markup.includes('<code>Read(docs/**)</code>'), markup)
})

// ---------------------------------------------------------------- the folder question

test('the folder question names the directory, how the settings named it, and how many follow', () => {
  const markup = render(FolderPrompt, { question: folderQuestion(), onAnswer: () => {} })
  assert.ok(markup.includes('/home/someone/shared'), markup)
  assert.ok(markup.includes('<code>../shared</code>'), markup)
  assert.ok(markup.includes('1 more folder to ask about'), markup)
  assert.ok(markup.includes('Open folder') && markup.includes('Don&#x27;t open'), markup)
  const last = render(FolderPrompt, { question: folderQuestion({ remaining: 0 }), onAnswer: () => {} })
  assert.ok(!last.includes('data-test="folder-remaining"'), last)
})

// ---------------------------------------------------------------- answering and chaining

/** A window holding one session, and a bridge answering from a script of replies. */
function harness(held, replies) {
  const calls = []
  const problems = []
  const sessions = new Map([['s1', held]])
  const answering = {
    call: async (method, params) => {
      calls.push({ method, params })
      const reply = replies.shift()
      if (!reply) throw new Error(`nothing scripted for ${method}`)
      if (reply.method !== method) throw new Error(`expected ${reply.method}, was asked ${method}`)
      if (reply.error) throw reply.error
      return reply.result
    },
    read: (handle) => sessions.get(handle),
    update: (handle, change) => sessions.set(handle, { ...sessions.get(handle), ...change }),
    setProblem: (problem) => problems.push(problem),
  }
  return { answering, calls, problems, now: () => sessions.get('s1'), replies }
}
const stale = Object.assign(new Error('no such request'), { code: 'no_such_request' })

test('trust, then rules, then each folder: every answer says what is out next, and the composer waits throughout', async () => {
  const opened = settledBy({ autoVetting: false, permissionMode: 'default', sandboxMode: 'confined', rulesQuestion: null, folderQuestion: null, foldersRefused: [] })
  assert.equal(opened.rulesQuestion, null)
  const live = { running: false, askingTrust: '/p', ...questions.reported({ rulesQuestion: rulesQuestion() }) }
  assert.equal(questions.waitingOn(live), 'trust')
  assert.equal(canSend(live, true, 'hi'), false)

  const { answering, calls, now } = harness({ ...live, askingTrust: null }, [
    { method: 'rules.reply', result: { granted: true, kept: true, settingsRules: rules({ granted: [{ ...rule('Bash(cargo test)'), before: false }] }), folderQuestion: folderQuestion() } },
    { method: 'folder.reply', result: { open: true, folder: '/home/someone/shared', shownFolder: '/home/someone/shared', problem: null, folders: ['/home/someone/shared'], folderQuestion: folderQuestion({ request: 9, directory: '/home/someone/other', shownDirectory: '/home/someone/other', remaining: 0 }) } },
    { method: 'folder.reply', result: { open: false, folder: '/home/someone/other', shownFolder: '/home/someone/other', problem: null, folders: ['/home/someone/shared'], folderQuestion: null } },
    { method: 'permissions.list', result: { settingsRules: rules({ directories: ['/home/someone/other'] }), rulesQuestion: null, folderQuestion: null, foldersRefused: [], folders: ['/home/someone/shared'] } },
  ])
  assert.equal(questions.waitingOn(now()), 'rules')
  assert.ok(askingFirst(now()), 'the composer holds a message back while the rules question is out')

  await questions.replyRules(answering, 's1', true)
  assert.deepEqual(calls[0], { method: 'rules.reply', params: { session: 's1', request: 7, granted: true } }, 'a yes names the request and no rule')
  assert.equal(now().rulesQuestion, null)
  assert.equal(now().folderQuestion.request, 8, 'the first folder question is drawn next')
  assert.equal(questions.waitingOn(now()), 'folder')

  await questions.replyFolder(answering, 's1', true)
  assert.deepEqual(calls[1], { method: 'folder.reply', params: { session: 's1', request: 8, open: true } }, 'a yes names the request and no path')
  assert.deepEqual(now().folders, ['/home/someone/shared'], 'the opened folder is in the Folders section')
  assert.equal(now().folderQuestion.request, 9)

  await questions.replyFolder(answering, 's1', false)
  assert.equal(now().folderQuestion, null)
  assert.equal(calls[3].method, 'permissions.list', 'after the last one the note is read again')
  assert.deepEqual(now().rules.directories, ['/home/someone/other'])
  assert.equal(questions.waitingOn({ askingTrust: null, ...now() }), null)
  assert.equal(canSend({ running: false, askingTrust: null, ...now() }, true, 'hi'), true)
})

test('a yes the grant record could not keep is still a yes, and says the next chat will ask', async () => {
  const { answering, problems, now } = harness({ askingTrust: null, ...questions.reported({ rulesQuestion: rulesQuestion() }) }, [
    { method: 'rules.reply', result: { granted: true, kept: false, settingsRules: rules(), folderQuestion: null } },
  ])
  await questions.replyRules(answering, 's1', true)
  assert.equal(now().rulesQuestion, null)
  assert.ok(problems[0]?.includes('for this chat only'), problems[0])
})

test('a yes with nothing to keep, and a no, say nothing about the grant record', async () => {
  for (const [granted, shownKeeping] of [[true, null], [false, '~/.bravebot/granted/abc.json']]) {
    const { answering, problems } = harness({ askingTrust: null, ...questions.reported({ rulesQuestion: rulesQuestion({ keeping: shownKeeping, shownKeeping }) }) }, [
      { method: 'rules.reply', result: { granted, kept: null, settingsRules: rules(), folderQuestion: null } },
    ])
    await questions.replyRules(answering, 's1', granted)
    assert.deepEqual(problems, [], `granted ${granted}: ${problems[0]}`)
  }
})

test('a yes that could not open its folder says why, and the note lists it', async () => {
  const { answering, problems, now } = harness({ askingTrust: null, ...questions.reported({ folderQuestion: folderQuestion({ remaining: 0 }) }) }, [
    { method: 'folder.reply', result: { open: true, folder: '/home/someone/shared', shownFolder: '/home/someone/shared', problem: 'it is no longer a directory', folders: [], folderQuestion: null } },
    { method: 'permissions.list', error: new Error('bridge gone') },
  ])
  await questions.replyFolder(answering, 's1', true)
  assert.ok(problems[0]?.includes('it is no longer a directory'), problems[0])
  assert.equal(now().foldersRefused.length, 1)
  const markup = render(RulesBanner, { rules: rules(), refused: now().foldersRefused })
  assert.ok(markup.includes('cannot be opened') && markup.includes('it is no longer a directory'), markup)
})

test('a stale answer re-reads what is out once and draws that, with no second reply', async () => {
  const { answering, calls, problems, now } = harness({ askingTrust: null, ...questions.reported({ rulesQuestion: rulesQuestion() }) }, [
    { method: 'rules.reply', error: stale },
    { method: 'permissions.list', result: { settingsRules: rules(), rulesQuestion: null, folderQuestion: folderQuestion({ request: 12 }), foldersRefused: [] } },
  ])
  await questions.replyRules(answering, 's1', true)
  assert.deepEqual(calls.map((each) => each.method), ['rules.reply', 'permissions.list'])
  assert.equal(now().rulesQuestion, null)
  assert.equal(now().folderQuestion.request, 12, 'what the bridge says is out is drawn')
  assert.ok(problems[0]?.includes('answered already'), problems[0])
})

test('a stale answer whose re-read fails puts the question away rather than looping', async () => {
  const { answering, calls, now } = harness({ askingTrust: null, ...questions.reported({ folderQuestion: folderQuestion() }) }, [
    { method: 'folder.reply', error: stale },
    { method: 'permissions.list', error: new Error('bridge gone') },
  ])
  await questions.replyFolder(answering, 's1', true)
  assert.equal(calls.length, 2, 'asked once each, and not again')
  assert.equal(now().folderQuestion, null)
  // With nothing out, a second answer sends nothing at all.
  await questions.replyFolder(answering, 's1', true)
  assert.equal(calls.length, 2)
})

// ---------------------------------------------------------------- the folded note

test('after a grant the note lists the granted rules as in force, where the grant is kept, and not as not in force', () => {
  const granted = rules({
    granted: [{ ...rule('Bash(cargo test)'), before: false }, { ...rule('Read(docs/**)'), before: true }],
    grantedRecord: '/home/someone/.bravebot/granted/abc.json',
    shownGrantedRecord: '~/.bravebot/granted/abc.json',
  })
  const markup = render(SessionNotices, { forkedFrom: null, kept: null, vetting: false, rules: granted, refused: [], onOpenParent: () => {}, onManage: () => {} })
  assert.ok(markup.includes('2 project rules granted'), markup)
  assert.ok(!markup.includes('not in force.'), 'nothing is said not to be in force')
  assert.ok(markup.includes('<code>Bash(cargo test)</code>'), markup)
  assert.ok(markup.includes('granted before'), markup)
  assert.ok(markup.includes('~/.bravebot/granted/abc.json'), markup)
  assert.ok(!markup.includes('rules-note-proposed'), markup)

  const thisChat = render(RulesBanner, { rules: rules({ granted: [{ ...rule('Bash(ls)'), before: false }] }) })
  assert.ok(thisChat.includes('for this chat only'), thisChat)
})

test('after declining the note lists the proposed rules and the directories still not open as not in force', () => {
  const declined = rules({ proposed: [rule('Bash(cargo test)')], directories: ['/home/someone/shared'], shownDirectories: ['/home/someone/shared'] })
  const markup = render(SessionNotices, { forkedFrom: null, kept: null, vetting: false, rules: declined, refused: [], onOpenParent: () => {}, onManage: () => {} })
  assert.ok(markup.includes('2 permission settings are not in force'), markup)
  assert.ok(markup.includes('were not granted'), markup)
  assert.ok(markup.includes('<code>/home/someone/shared</code>'), markup)
  assert.ok(markup.includes('they are not open'), markup)
})

test('a refused folder the rules do not list by that name is still said, with its reason', () => {
  const refused = [{ directory: '/elsewhere', shownDirectory: '/elsewhere', problem: 'gone', shownProblem: 'gone' }]
  assert.equal(questions.notInForceCount(rules(), refused), 1)
  const markup = render(SessionNotices, { forkedFrom: null, kept: null, vetting: false, rules: rules(), refused, onOpenParent: () => {}, onManage: () => {} })
  assert.ok(markup.includes('1 permission setting is not in force'), markup)
  assert.ok(markup.includes('<code>/elsewhere</code>: gone'), markup)
})
