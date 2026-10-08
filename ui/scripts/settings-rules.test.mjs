// How the window shows the permission rules a conversation opened under.
//
// docs/specs/permissions.md has a rule that could not be read, and an allow rule a checkout
// wrote, each reported where a person reads it (PERM-11, PERM-14). A rule that reads as
// protection and is not in force is the failure those clauses exist to prevent.
//
// The components are rendered through `react-dom/server` and the markup is what is asserted on.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Row, RulesBanner } = load('src/renderer/components/Transcript.tsx')
const { RulesInForce, FilesystemRules } = load('src/renderer/components/Permissions.tsx')
const t = load('src/renderer/transcript.ts')

const NONE = { deny: [], ask: [], allow: [], unreadable: [], proposed: [], directories: [] }
const rules = (overrides = {}) => ({ ...NONE, ...overrides })
const render = (component, props) => renderToStaticMarkup(React.createElement(component, props))

test('nothing is said where every setting is in force', () => {
  assert.equal(t.notInForce(null), false)
  assert.equal(t.notInForce(undefined), false)
  assert.equal(t.notInForce(NONE), false)
  // Rules in force are not a problem to report.
  assert.equal(t.notInForce(rules({ deny: ['Read(.env)'], ask: ['Edit'], allow: ['Bash(ls)'] })), false)
})

test('each kind of setting that is not in force is something to say', () => {
  assert.equal(t.notInForce(rules({ unreadable: [{ rule: 'Bsh(ls)', said: 'x' }] })), true)
  assert.equal(t.notInForce(rules({ proposed: [{ rule: 'Bash(ls)', file: '/p/.bravebot/settings.json' }] })), true)
  assert.equal(t.notInForce(rules({ directories: ['../other'] })), true)
})

test('an entry that is not a rule is reported in the agent’s own words', () => {
  const markup = render(RulesBanner, {
    rules: rules({ unreadable: [{ rule: 'Bsh(ls)', said: 'Bsh(ls) names no kind of tool this agent has' }] }),
  })

  assert.ok(markup.includes('1 permission setting is not in force'), markup)
  assert.ok(markup.includes('Bsh(ls) names no kind of tool this agent has'), markup)
  assert.ok(markup.includes('they decide nothing'), markup)
})

test('an allow rule a project wrote is named with its file, and said not to stop a question', () => {
  const markup = render(RulesBanner, {
    rules: rules({ proposed: [{ rule: 'Bash(cargo test)', file: '/home/someone/project/.bravebot/settings.json' }] }),
  })

  assert.ok(markup.includes('<code>Bash(cargo test)</code>'), markup)
  assert.ok(markup.includes('<code>/home/someone/project/.bravebot/settings.json</code>'), markup)
  assert.ok(markup.includes('only your own settings file may write one'), markup)
  assert.ok(markup.includes('You will still be asked'), markup)
})

test('a directory a file named is said not to be open', () => {
  const markup = render(RulesBanner, { rules: rules({ directories: ['../other'] }) })

  assert.ok(markup.includes('<code>../other</code>'), markup)
  assert.ok(markup.includes('This app opens none'), markup)
})

test('the count is of everything not in force', () => {
  const markup = render(RulesBanner, {
    rules: rules({
      unreadable: [{ rule: 'a', said: 'a is wrong' }],
      proposed: [{ rule: 'Bash(ls)', file: '/f' }, { rule: 'Edit', file: '/f' }],
      directories: ['../other'],
    }),
  })

  assert.ok(markup.includes('4 permission settings are not in force'), markup)
})

test('what a settings file wrote is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a>'
  const markup = render(RulesBanner, {
    rules: rules({ unreadable: [{ rule: forged, said: forged }], proposed: [{ rule: forged, file: forged }], directories: [forged] }),
  })

  for (const forbidden of ['<img', '<a ', '<a>']) assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
})

test('the rules in force are listed by what each list does, and cannot be revoked here', () => {
  const markup = render(RulesInForce, {
    rules: rules({ deny: ['Read(.env)'], ask: ['Edit(src/**)'], allow: ['Bash(cargo test)'] }),
  })

  const refused = markup.indexOf('Refused')
  const asked = markup.indexOf('Always asked')
  const allowed = markup.indexOf('Not asked')
  assert.ok(refused > 0 && refused < asked && asked < allowed, 'the lists are in the order they decide in')
  for (const rule of ['Read(.env)', 'Edit(src/**)', 'Bash(cargo test)']) {
    assert.ok(markup.includes(`<code>${rule}</code>`), rule)
  }
  assert.ok(!markup.includes('<button'), 'a rule is changed in its file, so there is nothing to press')
  assert.ok(markup.includes('What a command prints is not trusted because of it'), markup)
  assert.ok(markup.includes('not applied to a plan run'), markup)
})

test('a list with nothing in it is left out, and no rules at all is said', () => {
  const some = render(RulesInForce, { rules: rules({ deny: ['Read(.env)'] }) })
  assert.ok(some.includes('Refused'), some)
  assert.ok(!some.includes('Always asked') && !some.includes('Not asked'), some)

  for (const none of [NONE, null]) {
    const markup = render(RulesInForce, { rules: none })
    assert.ok(markup.includes('No permission rules are in force'), markup)
  }
})

test('a plan names the rules it is not held to', () => {
  const held = t.narrowing(rules({ deny: ['Edit(notes.md)'], ask: ['Edit(src/**)'], allow: ['Bash(ls)'] }))
  // An allow rule narrows nothing, so it is not one a run could break.
  assert.deepEqual(held, ['Edit(notes.md)', 'Edit(src/**)'])

  const entry = t.askedManifest({ request: 1, task: 'write the notes', steps: ['1. [act] write notes.md'] }, held)
  const markup = render(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false })
  assert.ok(markup.includes('Permission rules are not applied to a plan run'), markup)
  assert.ok(markup.includes('<code>Edit(notes.md)</code>'), markup)
  assert.ok(markup.includes('<code>Edit(src/**)</code>'), markup)
})

test('a plan in a conversation with no such rule says nothing about rules', () => {
  for (const held of [[], t.narrowing(null), t.narrowing(rules({ allow: ['Bash(ls)'] }))]) {
    const entry = t.askedManifest({ request: 1, task: 'write the notes', steps: ['1. [act] write notes.md'] }, held)
    const markup = render(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false })
    assert.ok(!markup.includes('Permission rules are not applied'), markup)
  }
})

const FS = [
  { key: 'denyRead', path: '~/.config/gh', file: '/home/someone/.bravebot/settings.json', pinned: false, refused: null },
  { key: 'allowWrite', path: '~/notes', file: null, pinned: false, refused: null },
  { key: 'denyWrite', path: '.env', file: '/p/.bravebot/settings.json', pinned: true, refused: null },
  { key: 'denyRead', path: '../secrets', file: '/home/someone/.bravebot/settings.json', pinned: false, refused: 'it climbs out of the directory it is read from' },
]

// SANDBOX-25: the window shows the four lists and edits nothing.
test('the filesystem lists are shown read only, each entry with who wrote it and why one is not in force', () => {
  const markup = render(RulesInForce, { rules: rules({ filesystem: FS }) })

  assert.ok(!markup.includes('No permission rules are in force'), 'paths in force are rules in force')
  for (const path of ['~/.config/gh', '~/notes', '.env', '../secrets']) assert.ok(markup.includes(`<code>${path}</code>`), path)
  assert.ok(markup.includes('/home/someone/.bravebot/settings.json'), 'the file that wrote an entry is named')
  assert.ok(markup.includes('Command line'), 'an entry with no file came from the command line')
  assert.ok(markup.includes('Set by your administrator'), 'a pinned entry says who pinned it')
  assert.ok(markup.includes('Not in force: it climbs out of the directory it is read from'), markup)
  assert.ok(!markup.includes('<button'), 'a path is changed in its file, so there is nothing to press')
})

test('a project file’s allowance is said to be ignored, and an older bridge says nothing', () => {
  const markup = render(FilesystemRules, {
    rules: rules({ filesystemIgnored: [{ key: 'allowWrite', file: '/p/.bravebot/settings.json' }] }),
  })
  assert.ok(markup.includes('allowWrite in /p/.bravebot/settings.json is not obeyed'), markup)

  for (const absent of [NONE, null, rules({ filesystem: [] })]) {
    assert.equal(render(FilesystemRules, { rules: absent }), '', 'nothing to show is no section')
  }
})

test('a refused path or an ignored allowance is something the banner says is not in force', () => {
  assert.equal(t.notInForce(rules({ filesystem: [FS[0], FS[1], FS[2]] })), false)
  assert.equal(t.notInForce(rules({ filesystem: FS })), true)
  assert.equal(t.notInForce(rules({ filesystemIgnored: [{ key: 'allowRead', file: '/f' }] })), true)

  const markup = render(RulesBanner, {
    rules: rules({ filesystem: FS, filesystemIgnored: [{ key: 'allowRead', file: '/p/.bravebot/settings.json' }] }),
  })
  assert.ok(markup.includes('2 permission settings are not in force'), markup)
  assert.ok(markup.includes('<code>../secrets</code>'), markup)
  assert.ok(markup.includes('is not started'), markup)
})

test('a path a settings file wrote is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png">'
  const markup = render(FilesystemRules, {
    rules: rules({ filesystem: [{ key: 'denyRead', path: forged, file: forged, pinned: false, refused: forged }] }),
  })
  assert.ok(!markup.includes('<img'), markup)
})
