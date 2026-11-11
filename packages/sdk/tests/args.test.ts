import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ArgumentError, run, startSession } from '../src/index.js'
import { FLAG_OPTIONS, PROCESS_OPTIONS, RUN_OPTION_NAMES, SESSION_ONLY_OPTIONS, SESSION_OPTION_NAMES, runArguments, sessionArguments } from '../src/args.js'
import { fake } from './support/fake.js'

// The command line the library builds is the whole of what it can ask of bravebot. The lists below
// name every option and every flag it can produce, so a new one has to be added on purpose and
// reviewed against CLI-1 (nothing is approved where nobody is asked).
test('the options the library takes are exactly these, and none lowers a permission, vouches for a path or writes the trust map', () => {
  assert.deepEqual(RUN_OPTION_NAMES, [
    'command', 'cwd', 'effort', 'env', 'incognito', 'maxRounds', 'maxTokens', 'model', 'noShell', 'onEvent', 'onStderr',
    'outputSchema', 'resume', 'signal', 'systemPrompt', 'tools',
  ])
  assert.deepEqual(SESSION_OPTION_NAMES, [...RUN_OPTION_NAMES, ...SESSION_ONLY_OPTIONS].sort())
  assert.deepEqual([...SESSION_ONLY_OPTIONS], ['trustInputLines', 'onQuestion', 'onError'])
  assert.deepEqual([...PROCESS_OPTIONS], ['command', 'cwd', 'env', 'signal', 'onStderr', 'onEvent'])
  assert.deepEqual(Object.values(FLAG_OPTIONS).sort(), [
    '--effort', '--incognito', '--max-rounds', '--max-tokens', '--model', '--no-shell', '--output-schema', '--resume', '--system-prompt', '--tools',
  ])
})

test('a one-shot run builds one --json-stream -p command line and a session builds one --input-format lines command line', () => {
  assert.deepEqual(runArguments('hello', {}), ['--json-stream', '-p', 'hello'])
  assert.deepEqual(sessionArguments({}), ['--json-stream', '--input-format', 'lines'])
})

test('every option becomes its own flag and the prompt is last, whatever order the options were given in', () => {
  const everything = {
    resume: 'rec-1', incognito: true, systemPrompt: 'be brief', noShell: true, tools: ['read_file', 'list_files'],
    maxTokens: 9000, maxRounds: 4, outputSchema: 'verdict.json', effort: 'high', model: 'some/model',
  }
  const expected = [
    '--model', 'some/model', '--effort', 'high', '--output-schema', 'verdict.json', '--max-rounds', '4', '--max-tokens', '9000',
    '--tools', 'read_file,list_files', '--no-shell', '--system-prompt', 'be brief', '--incognito', '--resume', 'rec-1',
  ]
  assert.deepEqual(runArguments('go', everything), ['--json-stream', ...expected, '-p', 'go'])
  assert.deepEqual(sessionArguments(everything), ['--json-stream', '--input-format', 'lines', ...expected])
  assert.deepEqual(sessionArguments({ trustInputLines: true }), ['--json-stream', '--input-format', 'lines', '--trust-input-lines'])
})

test('no command line the library builds holds a flag that lowers a permission, vouches for a path or approves an effect', () => {
  const banned = [
    '--dangerously-skip-permissions', '--accept-edits', '--auto', '--trust-workspace', '--add-dir', '--settings', '--approve-input-lines',
    '--allow', '--yolo', '--locked',
  ]
  const lines = [
    runArguments('go', { model: 'm', tools: ['a'], noShell: true, incognito: true }),
    sessionArguments({ trustInputLines: true, model: 'm', tools: ['a'], noShell: true, incognito: true }),
  ]
  for (const argv of lines) for (const flag of banned) assert.equal(argv.includes(flag), false, flag)
})

test('an option the library does not offer is refused, under any spelling, so it is never ignored or passed on', () => {
  const widening = [
    'acceptEdits', 'accept_edits', 'auto', 'dangerouslySkipPermissions', 'trustWorkspace', 'addDir', 'settings', 'approveInputLines',
    'permissionMode', 'autonomy', 'args', 'argv', 'flags', 'extraArgs', 'onPermission',
  ]
  for (const name of widening) {
    assert.throws(() => runArguments('go', { [name]: true } as never), ArgumentError, name)
    assert.throws(() => sessionArguments({ [name]: true } as never), ArgumentError, name)
  }
})

test('a value that starts with a dash is refused before anything is started, so it can never be read as a flag', async () => {
  const stand = fake('echo')
  try {
    await assert.rejects(run('--dangerously-skip-permissions', { command: stand.command, env: stand.env }), ArgumentError)
    await assert.rejects(run('ok', { command: stand.command, env: stand.env, model: '--accept-edits' }), ArgumentError)
    for (const bad of [
      { effort: '-x' }, { outputSchema: '--settings' }, { systemPrompt: '--auto' }, { resume: '--trust-workspace' }, { tools: ['read_file', '--no-shell'] },
    ]) {
      assert.throws(() => runArguments('ok', bad), ArgumentError, JSON.stringify(bad))
      assert.throws(() => sessionArguments(bad), ArgumentError, JSON.stringify(bad))
    }
    assert.throws(() => startSession({ command: stand.command, env: stand.env, model: '-m' }), ArgumentError)
    assert.throws(() => runArguments('', {}), ArgumentError)
    assert.throws(() => runArguments('ok', { maxRounds: 0 }), ArgumentError)
    assert.throws(() => runArguments('ok', { maxTokens: 1.5 }), ArgumentError)
    assert.throws(() => runArguments('ok', { tools: [] }), ArgumentError)
    assert.throws(() => runArguments('ok', { tools: ['a,--no-shell'] }), ArgumentError)
    assert.throws(() => runArguments('ok', { model: '  ' }), ArgumentError)
    assert.throws(() => stand.argv(), 'nothing was started')
  } finally {
    stand.dispose()
  }
})
