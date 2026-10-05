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

test('a window naming a path where an attachment id goes sends nothing', () => {
  assert.throws(() => sanitised('turn.send', { session: 's1', prompt: 'look', attachments: ['/Users/me/Pictures/private.png'] }))
})

test('a manifest run forwards a task and a model, and no file or picture under any key', () => {
  const forwarded = sanitised('manifest.run', {
    session: 's1',
    task: 'plan',
    model: 'm',
    images: [picture],
    attachments: ['/Users/me/Pictures/private.png'],
    dropped: ['/etc/hosts'],
    files: ['secret.md'],
  })

  assert.deepEqual(forwarded, { session: 's1', task: 'plan', model: 'm' })
})
