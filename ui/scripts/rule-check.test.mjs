// How the window asks which rule decides a call, and how it draws the answer.
//
// docs/specs/cli.md CLI-30: the Conversation permissions panel asks `permissions.check` about a
// call and shows the decision, the rule as its file spelled it, the file or grant it came from,
// the person's reason, and a checkout allow rule that is not in force.
//
// The components are rendered through `react-dom/server` and the markup is what is asserted on.
// Pressing Check is done by calling the section with React's state hooks stood in for, so the
// request it sends and what it does with the answer can be read.

import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { inPlaceModal, loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

/** `react` for the bundle: the real one, with `useState` taking its first values from `states`, and effects not run. */
const hooks = { states: [], sets: [] }
const reactStub = {
  name: 'react-stub',
  setup(built) {
    built.onResolve({ filter: /^react$/ }, () => ({ path: 'react', namespace: 'react-stub' }))
    built.onLoad({ filter: /.*/, namespace: 'react-stub' }, () => ({
      loader: 'js',
      contents: 'module.exports = globalThis.__ruleCheckReact',
    }))
  },
}
globalThis.__ruleCheckReact = {
  ...React,
  useState: (initial) => {
    const value = hooks.states.length > 0 ? hooks.states.shift() : initial
    return [value, (next) => hooks.sets.push(next)]
  },
  useRef: (current) => ({ current }),
  useEffect: () => {},
}

const plain = await loadComponents('src/renderer/components/Permissions.tsx')
const stubbed = await loadComponents('src/renderer/components/Permissions.tsx', [inPlaceModal, reactStub])

const render = (component, props) => renderToStaticMarkup(React.createElement(component, props))
const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

/** The window's bridge, answering every request with `answer` and keeping what it was asked. */
function bridge(answer) {
  const calls = []
  globalThis.window = { bravebot: { request: async (method, params) => { calls.push([method, params]); return answer } } }
  return calls
}

/** Presses Check on the section with `family` chosen and `text` typed, and returns what it did. */
async function press(family, text, answer) {
  const calls = bridge(answer)
  const problems = []
  hooks.sets = []
  hooks.states = [family, text, null, false]
  const form = stubbed.RuleCheck({ session: 's1', onProblem: (problem) => problems.push(problem) })
  assert.equal(hooks.states.length, 0, 'the section took every state it was given')
  form.props.onCheck()
  await settle()
  return { calls, problems, sets: hooks.sets }
}

const ANSWER = {
  decision: 'deny',
  rule: 'Bash(git push:*)',
  shownRule: 'Bash(git push:*)',
  file: '/home/someone/.bravebot/settings.json',
  shownFile: '/home/someone/.bravebot/settings.json',
  granted: false,
  reason: 'Pushes go through review.',
  notInForce: null,
}
const answer = (overrides = {}) => ({ ...ANSWER, ...overrides })
const NOTHING = { rule: null, shownRule: null, file: null, shownFile: null, granted: false, reason: null, notInForce: null }

test('a command is sent split on runs of whitespace, with the session and the family', async () => {
  const { calls, problems, sets } = await press('Bash', '  git   push\torigin ', { ok: ANSWER })
  assert.deepEqual(calls, [['permissions.check', { session: 's1', family: 'Bash', words: ['git', 'push', 'origin'] }]])
  assert.deepEqual(problems, [''], 'an earlier problem is cleared and no new one is said')
  assert.ok(sets.includes(ANSWER), 'the answer is kept to be drawn')
})

test('a path, a host and a tool are each sent as one word, and a web search with none', async () => {
  for (const [family, text, words] of [
    ['Read', 'notes/my file.md', ['notes/my file.md']],
    ['Edit', 'src/main.rs', ['src/main.rs']],
    ['WebFetch', 'https://example.com/a b', ['https://example.com/a b']],
    ['Mcp', 'brave:open_tab', ['brave:open_tab']],
    ['WebSearch', 'left from before', []],
  ]) {
    const { calls } = await press(family, text, { ok: ANSWER })
    assert.deepEqual(calls, [['permissions.check', { session: 's1', family, words }]], family)
  }
})

test('a refusal is said in the panel’s error, and no answer is drawn', async () => {
  const { problems, sets } = await press('Mcp', 'brave', { error: { code: 'bad_request', message: 'permissions check takes a family and what to ask about' } })
  assert.deepEqual(problems, ['', 'permissions check takes a family and what to ask about'])
  assert.ok(sets.includes(null), 'an earlier answer is taken away')

  hooks.states = [null, problems.at(-1), false]
  const markup = render(stubbed.Permissions, { session: 's1', onClose() {} })
  hooks.states = []
  assert.match(markup, /data-test="permissions-error"[^>]*>[^<]*permissions check takes a family and what to ask about/, markup)

  const thrown = await (async () => {
    globalThis.window = { bravebot: { request: async () => { throw new Error('gone') } } }
    return stubbed.checkRule('s1', 'Read', '.env')
  })()
  assert.deepEqual(thrown, { problem: 'The rule could not be checked. Try again.' })
})

/** The props of the element with `hook` as its data-test, found in what `RuleCheckForm` returns. Leo controls take these as properties, which server markup leaves out. */
function control(props, hook) {
  const walk = (node) => {
    if (!node || typeof node !== 'object') return null
    if (Array.isArray(node)) { for (const child of node) { const found = walk(child); if (found) return found } return null }
    if (node.props?.['data-test'] === hook) return node.props
    return walk(node.props?.children)
  }
  return walk(plain.RuleCheckForm({ busy: false, answer: null, onFamily() {}, onText() {}, onCheck() {}, ...props }))
}

test('the panel offers every family, and a web search takes no words', () => {
  const markup = render(plain.RuleCheckForm, { family: 'Bash', text: '', busy: false, answer: null, onFamily() {}, onText() {}, onCheck() {} })
  for (const family of ['Read', 'Edit', 'Bash', 'WebFetch', 'WebSearch', 'Mcp']) assert.ok(markup.includes(`<leo-option value="${family}">`), family)

  for (const [family, placeholder] of [['Read', 'path'], ['Edit', 'path'], ['Bash', 'program args'], ['WebFetch', 'host or URL'], ['Mcp', 'server:tool']]) {
    const words = control({ family, text: '' }, 'rule-check-words')
    assert.equal(words.placeholder, placeholder, family)
    assert.equal(words.disabled, false, `${family} takes words`)
  }
  assert.equal(control({ family: 'WebSearch', text: 'x' }, 'rule-check-words').disabled, true, 'a web search takes no words')
  assert.equal(control({ family: 'WebSearch', text: '' }, 'rule-check-submit').isDisabled, false, 'a web search is checked with an empty field')
  assert.equal(control({ family: 'Bash', text: '  ' }, 'rule-check-submit').isDisabled, true, 'a command with no words is not sent')
  assert.equal(control({ family: 'Bash', text: 'git push', busy: true }, 'rule-check-submit').isDisabled, true, 'one check at a time')

  let chosen = null
  control({ family: 'Bash', text: '', onFamily: (family) => { chosen = family } }, 'rule-check-family').onChange({ value: 'Mcp' })
  assert.equal(chosen, 'Mcp')
})

test('a rule that decides is drawn with its decision, rule, file and reason', () => {
  for (const [decision, said] of [['deny', 'Refused'], ['ask', 'Always asked'], ['allow', 'Not asked']]) {
    const markup = render(plain.RuleCheckAnswer, { answer: answer({ decision }) })
    assert.match(markup, new RegExp(`data-test="rule-check-decision" data-decision="${decision}">${said}<`), markup)
    assert.ok(markup.includes('<code>Bash(git push:*)</code>'), markup)
    assert.ok(markup.includes('<code>/home/someone/.bravebot/settings.json</code>'), markup)
    assert.ok(markup.includes('Pushes go through review.'), markup)
    assert.ok(!markup.includes('rule-check-not-in-force'), markup)
  }
})

test('no rule and a short name are said, with no rule or file row', () => {
  const none = render(plain.RuleCheckAnswer, { answer: { decision: 'none', ...NOTHING } })
  assert.ok(none.includes('No rule matches, so the usual questions decide'), none)
  const short = render(plain.RuleCheckAnswer, { answer: { decision: 'shortName', ...NOTHING } })
  assert.ok(short.includes('an 8.3 short name cannot be matched against it'), short)
  for (const markup of [none, short]) {
    assert.ok(!markup.includes('>Rule<') && !markup.includes('rule-check-file') && !markup.includes('rule-check-reason'), markup)
  }
})

test('a granted checkout rule names the file it was written in as granted', () => {
  const markup = render(plain.RuleCheckAnswer, { answer: answer({ decision: 'allow', rule: 'Bash(cargo test)', shownRule: 'Bash(cargo test)', file: '/p/.bravebot/settings.json', shownFile: '/p/.bravebot/settings.json', granted: true, reason: null }) })
  assert.ok(markup.includes('Granted for this workspace, written in <code>/p/.bravebot/settings.json</code>'), markup)
})

test('a checkout allow rule that is not in force is noted beside the decision', () => {
  const markup = render(plain.RuleCheckAnswer, {
    answer: { decision: 'none', ...NOTHING, notInForce: { rule: 'Bash(cargo test)', shownRule: 'Bash(cargo test)', file: '/p/.bravebot/settings.json', shownFile: '/p/.bravebot/settings.json' } },
  })
  assert.ok(markup.includes('data-test="rule-check-not-in-force"'), markup)
  assert.ok(markup.includes('<code>Bash(cargo test)</code> in <code>/p/.bravebot/settings.json</code> would allow this once it is granted at the question'), markup)
})

// PROMPT-1: the rule and file are drawn from their spelled fields, and never as markup.
const ZW = '​'
test('the rule, file and note are drawn from their spelled fields as text', () => {
  const forged = '<img src="https://example.com/x.png">'
  const markup = render(plain.RuleCheckAnswer, {
    answer: answer({
      rule: `Bash(g${ZW}it)`, shownRule: '"Bash(g\\u{200b}it)"', file: `/p${ZW}/s.json`, shownFile: '"/p\\u{200b}/s.json"', reason: forged,
      notInForce: { rule: `Edit(${ZW})`, shownRule: '"Edit(\\u{200b})"', file: forged, shownFile: forged },
    }),
  })
  assert.ok(!markup.includes(ZW), 'a held value was drawn')
  assert.ok(!markup.includes('<img'), markup)
  for (const text of ['Bash(g\\u{200b}it)', '/p\\u{200b}/s.json', 'Edit(\\u{200b})']) assert.ok(markup.includes(text), `${text}: ${markup}`)
})
