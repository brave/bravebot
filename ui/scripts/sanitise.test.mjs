// What the main process forwards to the agent from a window's request.
//
// `turn.send` lists are admitted to the planner as trusted input. `attachments` and `images` are
// read as bytes and sent as the person's own, and `attachments` may name any file on the disk, so
// a window able to name either could have any picture on the machine read and shown to the
// model. Only this process composes them.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/main/sanitise.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
const module = { exports: {} }
const electron = { app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false } }
new Function('require', 'module', 'exports', source)(id => id === 'electron' ? electron : require(id), module, module.exports)
const { sanitised } = module.exports

const picture = { media: 'image/png', data: 'iVBORw0KGgo=' }

test('a window’s turn reaches the agent with no picture and no path it named', () => {
  const forwarded = sanitised('turn.send', {
    session: 's1',
    prompt: 'look',
    model: 'm',
    images: [picture],
    attachments: [],
    dropped: ['/etc/hosts'],
    files: ['secret.md'],
    recall: false,
    definition: 'someone-else',
  })

  assert.deepEqual(forwarded, { session: 's1', prompt: 'look', model: 'm', files: [] })
})

// MEMORY-14: a summon names a definition for one turn, which only the main process may do.
test('a window’s turn reaches the agent addressed to no definition it named for that turn', () => {
  const forwarded = sanitised('turn.send', { session: 's1', prompt: 'look', address: 'someone-else' })

  assert.deepEqual(forwarded, { session: 's1', prompt: 'look', files: [] })
})

// ADDRESS-3: a definition picked from the `@` list goes on `bravebot:definitions:address`, so a
// window that names one on `turn.send` itself, beside a bot's standing definition, still sends neither.
test('a window’s turn reaches the agent addressed to no definition it picked, beside no definition standing', () => {
  const forwarded = sanitised('turn.send', { session: 's1', prompt: 'look', address: 'lantern', definition: 'ada', messageId: 'm-1' })

  assert.deepEqual(forwarded, { session: 's1', prompt: 'look', messageId: 'm-1', files: [] })
})

// RPCVIEW-10: the agent refuses a send without the id the window gave it.
test('a window’s turn reaches the agent with the message id it gave', () => {
  const forwarded = sanitised('turn.send', { session: 's1', prompt: 'look', messageId: 'm-1' })

  assert.equal(forwarded.messageId, 'm-1')
})

test('a window naming a path where an attachment id goes sends nothing', () => {
  assert.throws(() => sanitised('turn.send', { session: 's1', prompt: 'look', attachments: ['/Users/me/Pictures/private.png'] }))
})

test('a manifest run forwards a task, a model and text-file markers, and no file or picture the window named', () => {
  const forwarded = sanitised('manifest.run', {
    session: 's1',
    task: 'plan [File #1]',
    model: 'm',
    images: [picture],
    attachments: ['/Users/me/Pictures/private.png'],
    dropped: ['/etc/hosts'],
    files: ['secret.md', { marker: '[File #1]', name: 'notes.md', path: '/etc/hosts' }],
  })

  assert.deepEqual(forwarded, { session: 's1', task: 'plan [File #1]', model: 'm', files: [{ marker: '[File #1]', name: 'notes.md' }] })
})

test("a manifest run forwards the drops' markers as words, and nothing else under that name", () => {
  const forwarded = sanitised('manifest.run', { session: 's1', task: 'plan [Image #1]', markers: ['[Image #1]', { path: '/etc/hosts' }] })

  assert.deepEqual(forwarded.markers, ['[Image #1]'])
})

test('a session list is always the whole one, whatever directory the window put in it', () => {
  const forwarded = sanitised('session.list', { directory: '/Users/me/my app', other: 1 })

  assert.equal('directory' in forwarded, false, 'the window does not choose which store is read')
  assert.deepEqual(forwarded, { other: 1 })
  assert.deepEqual(sanitised('session.list', undefined), {})
})

test('a spend report forwards the folder and the days, and nothing else', () => {
  assert.deepEqual(
    sanitised('session.stats', { directory: '/Users/me/app', days: 7, session: 's1', directories: ['/etc'] }),
    { directory: '/Users/me/app', days: 7 },
  )
  assert.deepEqual(JSON.parse(JSON.stringify(sanitised('session.stats', undefined))), {})
})

test('a new session opens no folder beside its project that the window named', () => {
  const forwarded = sanitised('session.new', { directory: '/Users/me/app', directories: ['/Users/me/.ssh'] })

  assert.deepEqual(forwarded, { directory: '/Users/me/app' }, 'a folder beside the project comes from the main process picker alone')
})

test('a folder list and a folder close forward the session and the folder, and nothing else', () => {
  assert.deepEqual(sanitised('session.folders.list', { session: 's1', path: '/x' }), { session: 's1' })
  assert.deepEqual(
    sanitised('session.folders.close', { session: 's1', path: '/Users/me/docs', trusted: true, directories: ['/etc'] }),
    { session: 's1', path: '/Users/me/docs' },
  )
})

test('a schedule line forwards its argument with the names and markers, and no file or grant under any key', () => {
  const forwarded = sanitised('schedule.command', {
    session: 's1',
    argument: 'add --cron "0 9 * * *" --prompt "look at [Image #1] and [Image #2]"',
    files: [{ marker: '[Image #1]', name: 'shot.png', path: '/Users/me/.ssh/id_rsa', grant: 'g-1' }],
    pictures: ['[Image #2]'],
    drops: [{ marker: '[Image #1]', grant: 'g-1' }],
    pastes: [{ marker: '[Image #2]', grant: 'p-1' }],
    dropped: ['/Users/me/.ssh/id_rsa'],
    images: [picture],
    attachments: ['a-1'],
  })
  assert.deepEqual(forwarded, {
    session: 's1',
    argument: 'add --cron "0 9 * * *" --prompt "look at [Image #1] and [Image #2]"',
    files: [{ marker: '[Image #1]', name: 'shot.png' }],
    pictures: ['[Image #2]'],
  })
})
