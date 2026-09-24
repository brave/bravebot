// Which folder a bot may be pinned to.
//
// A saved bot's project folder becomes the directory the confined file helper is pinned to: its
// memory is created under `<folder>/.bravebot-ui/bots/<slug>.md` by a process that checks the
// walk and not the tree it walks in, and no session, no picker and no prompt stands between the
// channel that takes the folder and that write. So the property is about where the folder came
// from rather than about the shape of a path, and the fault it rejects is the well-formedness
// check standing in for it: `isProjectPath` accepts every absolute path on the account.
//
// One bundle for both modules, because the folders the picker handed over are one module's state
// and the composition that reads them is another's — two separate loads would be two sets.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)

/** The two main-process modules, sharing one instance of everything below them. */
function load(userData, showOpenDialog) {
  const source = buildSync({
    stdin: {
      contents: "export * from './src/main/bots'\nexport * from './src/main/opened'\n",
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const electron = {
    app: { getPath: () => userData, getAppPath: () => process.cwd(), isPackaged: false },
    dialog: { showOpenDialog },
  }
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(id => id === 'electron' ? electron : require(id), module, module.exports)
  return module.exports
}

const scratch = (name) => mkdtempSync(join(tmpdir(), `bravebot-${name}-`))
const form = (extra) => ({ name: 'Custodian', purpose: 'Keep the harbour lights lit', ...extra })

test('a new bot is pinned only to a folder the picker handed over', async () => {
  const userData = scratch('bot-directory-app')
  const chosen = scratch('bot-directory-project')
  const elsewhere = scratch('bot-directory-elsewhere')
  try {
    const main = load(userData, async () => ({ canceled: false, filePaths: [chosen] }))
    // Both are absolute and hold no NUL, so `isProjectPath` cannot tell them apart. Nothing has
    // been opened yet, so neither is a folder anybody pointed at.
    assert.equal(main.botFromForm(form({ directory: chosen })), null)
    assert.equal(main.botFromForm(form({ directory: elsewhere })), null)

    assert.equal(await main.chooseDirectory({}), chosen)
    assert.equal(main.botFromForm(form({ directory: chosen }))?.directory, chosen)
    assert.equal(
      main.botFromForm(form({ directory: elsewhere })),
      null,
      'opening one folder says nothing about the folder beside it',
    )
  } finally { for (const at of [userData, chosen, elsewhere]) rmSync(at, { recursive: true, force: true }) }
})

test('a cancelled picker leaves nothing a window may name', async () => {
  const userData = scratch('bot-cancelled-app')
  const offered = scratch('bot-cancelled-project')
  try {
    // The path the dialog was about to return. Cancelling is somebody declining to open it, so
    // the answer afterwards is the answer from before.
    const main = load(userData, async () => ({ canceled: true, filePaths: [offered] }))
    assert.equal(await main.chooseDirectory({}), null)
    assert.equal(main.botFromForm(form({ directory: offered })), null)
  } finally { for (const at of [userData, offered]) rmSync(at, { recursive: true, force: true }) }
})

test('editing a bot cannot move it to another folder', async () => {
  const userData = scratch('bot-move-app')
  const chosen = scratch('bot-move-project')
  const elsewhere = scratch('bot-move-elsewhere')
  try {
    const main = load(userData, async () => ({ canceled: false, filePaths: [chosen] }))
    await main.chooseDirectory({})
    const made = main.botFromForm(form({ directory: chosen }))
    main.saveBot(made)
    // The form fixes the folder of a bot that already exists and sends back the one it holds, so
    // this is a payload only something bypassing the form composes.
    const edited = main.botFromForm(form({ slug: made.slug, name: 'Keeper', directory: elsewhere }))
    assert.equal(edited.slug, made.slug)
    assert.equal(edited.name, 'Keeper')
    assert.equal(edited.directory, chosen, 'a rename is not a move')
  } finally { for (const at of [userData, chosen, elsewhere]) rmSync(at, { recursive: true, force: true }) }
})
