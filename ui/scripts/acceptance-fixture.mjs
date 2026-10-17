// The fixture the acceptance drivers share: an isolated profile holding one bot, and a bridge
// answered from the main process, so no agent and no model service is involved.
//
// `makeProfile` writes into fresh temporary directories, and the drivers launch with
// `--user-data-dir=<profile>`, so the app's own state is not the person's.
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

export const idA = '11111111-1111-4111-8111-111111111111'
export const idB = '22222222-2222-4222-8222-222222222222'

/**
 * A temporary profile and project directory, with a Review Bot in `bravebot-ui.json` and its
 * memory file under the project. `copy` also creates the sibling `<directory>-copy`.
 */
export function makeProfile({ conversations = [idA, idB], copy = false } = {}) {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-ux-profile-'))
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-ux-project-'))
  if (copy) mkdirSync(`${directory}-copy`)
  const bot = {
    slug: 'review-bot',
    name: 'Review Bot',
    purpose: 'Review a disposable project and remember preferences.',
    avatar: 'review-bot',
    model: null,
    directory,
    session: idA,
    conversations,
    archived: 0,
    remembered: 0,
    quiet: 0,
    retired: 0,
    created: 1,
  }
  writeFileSync(join(profile, 'bravebot-ui.json'), JSON.stringify({ bots: [bot], recents: [directory] }))
  mkdirSync(join(directory, '.bravebot-ui/bots'), { recursive: true })
  writeFileSync(join(directory, '.bravebot-ui/bots/review-bot.md'), '# Preferences\n\n- Keep reviews concise.\n')
  return { profile, directory }
}

/**
 * Replace the bridge's handlers in the main process. Runs inside Electron, so it must close over
 * nothing in this module: `idA` and `idB` arrive in `options` and shadow the exports above.
 *
 * `settings` adds the `settings.inspect` answer. `nativePicker` answers the native folder picker
 * and installs no `bravebot:choose-directory` handler; without it that handler is replaced and the
 * picker is left alone. Both are never installed: the real handler records the chosen folder in the
 * main process, so replacing it as well as the dialog would change what the driver exercises.
 */
export function installAcceptanceBridge({ ipcMain, BrowserWindow, dialog }, { directory, idA, idB, settings, nativePicker }) {
  // The bridge sends a session's updated time in seconds and the renderer multiplies it by 1000.
  const updated = Math.floor(Date.now() / 1000)
  const rows = [
    { id: idA, title: 'Review the sample project' },
    { id: idB, title: 'Plan the next iteration' },
  ].map((r) => ({ ...r, directory, project: 'sample-project', branch: 'main', updated, bytes: 20 }))
  const emit = (event, data, session = 's-' + idA) =>
    BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session })
  globalThis.ux = {
    emit,
    sent: [],
    configured: true,
    choose: directory,
    grants: {
      paths: [
        { path: '', integrity: 'trusted' },
        { path: 'private', integrity: 'untrusted' },
      ],
      commands: [{ program: '/usr/bin/git', args: ['status'], display: 'git status' }],
    },
  }
  const replace = (name, fn) => {
    ipcMain.removeHandler(name)
    ipcMain.handle(name, fn)
  }
  if (nativePicker) {
    // The native picker is what grants a folder to a new bot, so it is the picker that is answered.
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [globalThis.ux.choose] })
  } else {
    replace('bravebot:choose-directory', () => globalThis.ux.choose)
  }
  replace('bravebot:request', async (_, method, p = {}) => {
    const ux = globalThis.ux
    if (method === 'agent.info') return { ok: { configured: ux.configured, build: 'acceptance fixture', version: '1' } }
    if (settings && method === 'settings.inspect') {
      return {
        ok: {
          build: 'acceptance fixture',
          configured: ux.configured,
          problem: ux.configured ? null : 'No model service configured.',
          model: null,
          brave: false,
          bedrock: false,
          providers: [],
          selected: null,
          layers: [],
          overrides: [],
          managed: { path: null, keys: [] },
          network: {
            roots: [],
            problem: null,
            trustsNothing: false,
            proxy: null,
            authenticated: false,
            unusableProxy: null,
            noProxy: null,
          },
        },
      }
    }
    if (method === 'doctor') {
      return { ok: { found: true, text: 'Backend not configured. Install a configured build or rebuild with the approved environment.' } }
    }
    if (method === 'session.list') return { ok: { sessions: rows } }
    if (method === 'models.list') {
      return {
        ok: {
          defaultModel: 'sample/fast',
          warnings: [],
          models: [
            { id: 'sample/fast', name: 'Fast model', provider: 'Sample', premium: false, contextWindow: 200000, capabilities: ['text', 'tools', 'vision'] },
            { id: 'sample/deep', name: 'Deep model', provider: 'Sample', premium: false, contextWindow: 100000, capabilities: ['text', 'tools'] },
          ],
        },
      }
    }
    if (method === 'session.new') return { ok: { session: 's-new', directory, branch: 'main', model: 'sample/fast' } }
    if (method === 'session.open') {
      return {
        ok: {
          session: 's-' + p.id,
          model: 'sample/fast',
          record: { ...rows.find((r) => r.id === p.id), started: 1, turns: 1, tokens: 100, build: 'fixture' },
          said:
            p.id === idA
              ? [
                  { kind: 'user', text: 'Review the project and show a code example.' },
                  {
                    kind: 'assistant',
                    text: 'The project is ready for review.\n\n```js\nconst message = "A long example line for testing wrapping and copying without losing content.";\nconsole.log(message);\n```\n\nSee [sample.txt](src/nested/sample.txt).',
                  },
                ]
              : [],
          todos: {},
          context: '',
          trust: { known: true, rules: [] },
          archived: 0,
          branchNote: null,
          buildNote: null,
          frontNote: null,
        },
      }
    }
    if (method === 'turn.send') {
      ux.sent.push(p)
      emit('turn.started', { turn: ux.sent.length }, p.session)
      return { ok: { turn: ux.sent.length } }
    }
    if (method === 'turn.cancel') {
      emit(
        'turn.error',
        {
          kind: 'cancelled',
          message: 'Cancelled by user',
          turn: 1,
          id: p.session === 's-new' ? '33333333-3333-4333-8333-333333333333' : p.session.slice(2),
        },
        p.session,
      )
      return { ok: {} }
    }
    if (method === 'permissions.list') return { ok: ux.grants }
    if (method === 'permissions.revoke') {
      if (p.kind === 'path') ux.grants.paths = ux.grants.paths.filter((g) => g.path !== p.path)
      else ux.grants.commands = []
      return { ok: ux.grants }
    }
    return { ok: {} }
  })
  replace('bravebot:bots:send', (_, p) => {
    globalThis.ux.sent.push(p)
    emit('turn.started', { turn: globalThis.ux.sent.length }, p.session)
    return { ok: { turn: globalThis.ux.sent.length } }
  })
  replace('bravebot:files:list', () => ({ path: '', rows: [], truncated: false }))
  replace('bravebot:files:search', () => ({ paths: ['src/nested/sample.txt'], incomplete: false }))
  replace('bravebot:files:preview', (_, session, path) => ({ path, text: 'Alpha\nDelta\nGamma\n', truncated: false }))
  replace('bravebot:files:open', () => {
    globalThis.ux.opened = true
    return { status: 'opened' }
  })
  replace('bravebot:files:choose-attachments', () => [{ id: 'fixture-attachment', path: 'src/nested/sample.txt', bytes: 18 }])
}

/** Install the bridge in a launched app. The options are `installAcceptanceBridge`'s. */
export function installBridge(app, { directory, settings = false, nativePicker = false }) {
  return app.evaluate(installAcceptanceBridge, { directory, idA, idB, settings, nativePicker })
}

/** `emit` sends a bridge event to the window, and `done` finishes a turn. Both default to session A. */
export function eventsFor(app) {
  const emit = async (event, data, session = 's-' + idA) =>
    app.evaluate((_, v) => globalThis.ux.emit(v.event, v.data, v.session), { event, data, session })
  const done = async (session = 's-' + idA, reply = 'Review complete.') =>
    emit(
      'turn.done',
      {
        id: session.slice(2),
        turn: 1,
        reply,
        model: 'sample/fast',
        steps: 1,
        clean: true,
        tokens: 10,
        outputTokens: 10,
        notices: [],
        trust: { rules: [] },
        archived: 0,
      },
      session,
    )
  return { emit, done }
}
