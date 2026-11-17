// The window's half of adding and closing folders beside a chat's project (TRUST-9): the Folders
// section on the Files tab, the multi-root tree, and the hook that reads the agent's answers into
// the chat. The agent decides every add and close; these reject the window getting ahead of it:
// listing a folder no answer named, keeping one an answer dropped, staying quiet about a refusal,
// acting on a cancelled picker, and offering Add or Close while a turn runs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { build } from 'esbuild'
import { loadComponents, loadSync } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

const folders = loadSync('src/renderer/folders.ts')

// ---- reading the agent's answers -------------------------------------------------------------

test('an answer is read for its folder list and rewind points, and one with no list is no answer', () => {
  assert.deepEqual(folders.foldersAnswer({ folders: ['/a', 3, '/b'], busy: false }), { folders: ['/a', '/b'] })
  assert.deepEqual(folders.foldersAnswer({ folders: [], rewind: [] }), { folders: [], rewind: [] })
  assert.equal(folders.foldersAnswer({ folder: '/a' }), null)
  assert.equal(folders.foldersAnswer(null), null)
})

test('a cancelled picker is nothing, a refusal keeps the agent’s words, and an add carries its list', () => {
  assert.deepEqual(folders.folderAdded(null), { kind: 'cancelled' })
  assert.deepEqual(
    folders.folderAdded({ error: { code: 'bad_request', message: '/p is the project, which is open already' } }),
    { kind: 'refused', message: '/p is the project, which is open already' },
  )
  assert.deepEqual(
    folders.folderAdded({ ok: { folder: '/docs', folders: ['/docs'], endsCheckouts: false, trust: { rules: [] }, rewind: [] } }),
    { kind: 'added', folder: '/docs', answer: { folders: ['/docs'], rewind: [] } },
  )
  assert.equal(folders.folderAdded({ ok: {} }).kind, 'refused', 'an answer with no list is not taken as an add')
})

test('the tree’s roots are the project and then each folder, named by its last part', () => {
  assert.equal(folders.folderName('/Users/me/docs/'), 'docs')
  assert.deepEqual(folders.treeRoots('/w/app', ['/x/docs', '/y/lib']), [
    { key: '', path: '/w/app', name: 'app' },
    { key: '/x/docs', path: '/x/docs', name: 'docs', folder: '/x/docs' },
    { key: '/y/lib', path: '/y/lib', name: 'lib', folder: '/y/lib' },
  ])
})

// ---- the hook, with React reduced to running what it is handed --------------------------------

/** `use-folders.ts` with a React whose effects run at once, and the window's bridge given here. */
async function loadHook() {
  const reactStub = {
    name: 'react-stub',
    setup(built) {
      built.onResolve({ filter: /^react$/ }, () => ({ path: 'react', namespace: 'react-stub' }))
      built.onLoad({ filter: /.*/, namespace: 'react-stub' }, () => ({
        loader: 'js',
        contents: `export const useCallback = (fn) => fn
          export const useEffect = (fn) => { fn() }
          export const useSyncExternalStore = (_s, get) => get()`,
      }))
    },
  }
  const built = await build({
    entryPoints: ['src/renderer/use-folders.ts'], bundle: true, write: false, platform: 'node', format: 'cjs',
    plugins: [reactStub],
  })
  const module = { exports: {} }
  new Function('require', 'module', 'exports', built.outputFiles[0].text)(require, module, module.exports)
  return module.exports
}

/** A chat held as App holds it, and the calls the hook made of the window's bridge. */
function harness({ request, addFolder }) {
  const calls = []
  let chat = { handle: 's1', folders: undefined, rewind: [{ steps: 1 }], folderProblem: null }
  const problems = []
  globalThis.window = {
    bravebot: {
      request: async (method, params) => { calls.push([method, params]); return request(method, params) },
      addFolder: async (session) => { calls.push(['addFolder', session]); return addFolder() },
    },
  }
  const updateSession = (handle, action) => {
    assert.equal(handle, 's1')
    chat = typeof action === 'function' ? action(chat) : action
  }
  return { calls, problems, chat: () => chat, args: { handle: 's1', handleRef: { current: 's1' }, updateSession, setProblem: (p) => problems.push(p) } }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

test('a chat coming on screen asks for its folders, and takes the list the agent answers', async () => {
  const { useFolders } = await loadHook()
  const world = harness({ request: async () => ({ ok: { folders: ['/beside'], busy: false } }), addFolder: async () => null })
  useFolders(world.args)
  await settle()
  assert.deepEqual(world.calls, [['session.folders.list', { session: 's1' }]])
  assert.deepEqual(world.chat().folders, ['/beside'])
})

test('a cancelled picker changes nothing and says nothing', async () => {
  const { useFolders } = await loadHook()
  const world = harness({ request: async () => ({ ok: { folders: [] } }), addFolder: async () => null })
  const { addFolder } = useFolders(world.args)
  await settle()
  const before = world.chat()
  await addFolder('panel')
  assert.equal(world.chat(), before)
  assert.deepEqual(world.problems, [])
})

test('an add takes the agent’s list and its rewind points, and clears an earlier refusal', async () => {
  const { useFolders } = await loadHook()
  const world = harness({
    request: async () => ({ ok: { folders: [] } }),
    addFolder: async () => ({ ok: { folder: '/beside', folders: ['/beside'], endsCheckouts: false, trust: { rules: [] }, rewind: [] } }),
  })
  const { addFolder } = useFolders(world.args)
  await settle()
  world.args.updateSession('s1', (old) => ({ ...old, folderProblem: 'earlier' }))
  await addFolder('panel')
  assert.deepEqual(world.chat().folders, ['/beside'])
  assert.deepEqual(world.chat().rewind, [], 'an add shuts the rewind points')
  assert.equal(world.chat().folderProblem, null)
})

test('a refused add is said in the section it came from, or in the error toast from the menu', async () => {
  const { useFolders } = await loadHook()
  const refusal = { error: { code: 'bad_request', message: '/p/src is inside the project' } }
  const world = harness({ request: async () => ({ ok: { folders: [] } }), addFolder: async () => refusal })
  const { addFolder } = useFolders(world.args)
  await settle()
  await addFolder('panel')
  assert.equal(world.chat().folderProblem, '/p/src is inside the project')
  assert.deepEqual(world.problems, [])
  await addFolder('menu')
  assert.deepEqual(world.problems, ['/p/src is inside the project'])
  assert.deepEqual(world.chat().folders, [], 'a refusal leaves the list the agent last gave')
})

test('a close names the folder as listed and takes the list back; a refusal is said in the section', async () => {
  const { useFolders } = await loadHook()
  let busy = false
  const world = harness({
    request: async (method) => {
      if (method === 'session.folders.list') return { ok: { folders: ['/beside'], busy: false } }
      if (busy) return { error: { code: 'turn_in_flight', message: 'Wait for the current turn to finish before closing a folder.' } }
      return { ok: { folder: '/beside', withdrawn: true, folders: [], trust: { rules: [] }, rewind: [] } }
    },
    addFolder: async () => null,
  })
  const { closeFolder } = useFolders(world.args)
  await settle()
  busy = true
  await closeFolder('/beside')
  assert.equal(world.chat().folderProblem, 'Wait for the current turn to finish before closing a folder.')
  assert.deepEqual(world.chat().folders, ['/beside'])
  busy = false
  await closeFolder('/beside')
  assert.deepEqual(world.calls.at(-1), ['session.folders.close', { session: 's1', path: '/beside' }])
  assert.deepEqual(world.chat().folders, [])
  assert.equal(world.chat().folderProblem, null)
})

// ---- what the Files tab draws -----------------------------------------------------------------

// `react-dom/server` draws a Leo button as a bare `<leo-button>`, with none of the props that shut
// it, so here it is drawn as the plain button it stands for. The tree is drawn as the folders it
// was handed, which is all the Folders section owes it.
const inPlace = {
  name: 'in-place',
  setup(built) {
    built.onResolve({ filter: /^@brave\/leo\/react\/button$/ }, () => ({ path: 'button', namespace: 'plain-leo-button' }))
    built.onLoad({ filter: /.*/, namespace: 'plain-leo-button' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export default function Button({ className, isDisabled, onClick, children, ...rest }) {
          return createElement('button', { className, disabled: !!isDisabled, onClick, 'data-test': rest['data-test'], 'data-tooltip': rest['data-tooltip'], 'aria-label': rest['aria-label'] }, children)
        }`,
    }))
  },
}

let bundle
const components = () => (bundle ??= loadComponents({
  contents: "export { Context } from './src/renderer/components/Context.tsx'\nexport { FileTree } from './src/renderer/components/FileTree.tsx'",
}, [inPlace]))

const live = (overrides = {}) => ({
  handle: 's-1', summary: { directory: '/work/app' }, entries: [], todos: [], quarantine: [], phase: null, tokens: 0, running: false,
  ...overrides,
})

/** The Folders section's own markup, cut out of the column. */
function foldersPanel(markup) {
  const start = markup.indexOf('id="panel-folders"')
  assert.ok(start > 0, 'the column has a Folders section')
  return markup.slice(start, markup.indexOf('</section>', start))
}

const tags = (markup, name) => [...markup.matchAll(new RegExp(`<[^>]*data-test="${name}"[^>]*>`, 'g'))].map((found) => found[0])
const disabled = (tag) => /\sdisabled=""/.test(tag)
const draw = async (props) => {
  const { Context } = await components()
  return renderToStaticMarkup(React.createElement(Context, { onAddFolder() {}, onCloseFolder() {}, ...props }))
}

test('the section says when only the project is open, counts none, and offers Add folder', async () => {
  const panel = foldersPanel(await draw({ live: live({ folders: [] }) }))
  assert.ok(panel.includes('Only the project is open.'))
  assert.ok(panel.includes(', 0 items'))
  assert.equal(tags(panel, 'folders-add').length, 1)
  assert.equal(disabled(tags(panel, 'folders-add')[0]), false)
  assert.ok(panel.includes('Add folder…'))
})

test('each open folder is its name, its whole path on hover, and a Close of its own', async () => {
  const panel = foldersPanel(await draw({ live: live({ folders: ['/Users/me/docs', '/srv/shared/lib'] }) }))
  assert.ok(panel.includes('class="folder-name">docs<') && panel.includes('class="folder-name">lib<'))
  assert.ok(panel.includes('data-tooltip="/Users/me/docs"') && panel.includes('data-tooltip="/srv/shared/lib"'))
  const closes = tags(panel, 'folder-close')
  assert.equal(closes.length, 2)
  assert.ok(closes[0].includes('data-tooltip="Close docs'), closes[0])
  assert.ok(closes.every((tag) => !disabled(tag)))
  assert.ok(panel.includes(', 2 items'))
})

test('while a turn runs, Add and Close are greyed and say why', async () => {
  const panel = foldersPanel(await draw({ live: live({ folders: ['/Users/me/docs'], running: true }) }))
  for (const tag of [...tags(panel, 'folders-add'), ...tags(panel, 'folder-close')]) {
    assert.ok(disabled(tag), tag)
    assert.ok(tag.includes('data-tooltip="Wait for the turn to finish"'), tag)
  }
})

test('a refusal is drawn in the section as the agent said it', async () => {
  const panel = foldersPanel(await draw({ live: live({ folders: [], folderProblem: '/work/app is the project itself' }) }))
  assert.equal(tags(panel, 'folders-problem').length, 1)
  assert.ok(panel.includes('/work/app is the project itself'))
})

test('the section is on the Files tab only, and absent where nothing can add a folder', async () => {
  const { Context } = await components()
  const markup = await draw({ live: live({ folders: [] }) })
  // The column opens on Overview, where the Files tab's sections are turned off.
  assert.match(markup, /<section class="panel off" id="panel-folders">/)
  const none = renderToStaticMarkup(React.createElement(Context, { live: live({ folders: [] }) }))
  assert.ok(!none.includes('id="panel-folders"'))
})

test('the tree is the project alone with no folders, and one root row per folder after it', async () => {
  const { FileTree } = await components()
  const single = renderToStaticMarkup(React.createElement(FileTree, { session: 's-1', root: '/work/app', running: false }))
  assert.ok(!single.includes('data-test="tree-roots"'), 'one root draws as it always has')
  const many = renderToStaticMarkup(React.createElement(FileTree, { session: 's-1', root: '/work/app', folders: ['/Users/me/docs'], running: false }))
  const roots = tags(many, 'tree-root-item')
  assert.equal(roots.length, 2)
  assert.ok(roots.every((tag) => tag.includes('aria-level="1"') && tag.includes('aria-expanded="true"')))
  const first = many.indexOf('data-tooltip="/work/app"', many.indexOf('data-test="tree-roots"'))
  const second = many.indexOf('data-tooltip="/Users/me/docs"')
  assert.ok(first > 0 && second > first, 'the project first, then the folder, each with its whole path')
  assert.ok(many.includes('class="tree-name">docs<'))
})
