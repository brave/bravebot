// The world a stub-service driver runs in: a model service this script serves, a disposable HOME
// and profile, and a launch against that profile.
//
// `launchWorld` passes `--user-data-dir` and the env it is given. `makeWorld` builds both inside
// a temporary root, so a driver that takes them from it never reaches the real profile.
// `drive-support.mjs` is the module that launches against the real one, and `ui/docs/testing.md`
// lists which drivers use which.
//
// `playwright-core` is loaded when a launch happens, so the helpers that only take a page or an
// app can be tested without a browser driver installed.
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const INHERITED = ['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG']

export const listening = (server) => new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))

/** The variables an app needs from the environment it was started in, where they are set. */
export const inheritedEnv = () =>
  Object.fromEntries(INHERITED.filter((key) => process.env[key]).map((key) => [key, process.env[key]]))

/**
 * A model service on an ephemeral port. `reply({ body, messages, round })` decides each POST and
 * returns `{ content }` or `{ tool: { name, arguments } }`, where `arguments` is an object and
 * `round` counts the POSTs so far, this one included. A GET lists one model.
 */
export async function startModelService(reply) {
  const rounds = []
  const service = createServer(async (request, response) => {
    let body = ''
    request.setEncoding('utf8')
    for await (const chunk of request) body += chunk
    if (request.method !== 'POST') {
      response.writeHead(200, { 'Content-Type': 'application/json' })
      return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
    }
    rounds.push(body)
    const round = rounds.length
    const { content, tool } = await reply({ body, messages: JSON.parse(body).messages ?? [], round })
    const delta = tool
      ? { role: 'assistant', tool_calls: [{ index: 0, id: `call-${round}`, type: 'function', function: { name: tool.name, arguments: JSON.stringify(tool.arguments) } }] }
      : { role: 'assistant', content }
    const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: tool ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
    response.writeHead(200, { 'Content-Type': 'text/event-stream' })
    response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
  })
  await listening(service)
  return { service, rounds }
}

/**
 * A temporary root holding `home`, `profile` and, unless `project` is false, `project`, with the
 * `local/test` provider written into `home/.bravebot/settings.json` and merged with `settings`.
 * `dirs` are further directories to create, relative to the root. `env` is a plain object for the
 * driver to change before it launches.
 */
export function makeWorld(prefix, { service, baseURL, realpath = true, project = true, dirs = [], settings = {} } = {}) {
  if (!service && !baseURL) throw new Error('makeWorld needs a service or a baseURL for the provider')
  const created = mkdtempSync(join(tmpdir(), prefix))
  const root = realpath ? realpathSync(created) : created
  const home = join(root, 'home')
  const profile = join(root, 'profile')
  const projectDir = project ? join(root, 'project') : undefined
  const directories = [join(home, '.bravebot'), profile, ...(project ? [projectDir] : []), ...dirs.map((dir) => join(root, dir))]
  for (const path of directories) mkdirSync(path, { recursive: true })
  writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
    provider: { local: { options: { baseURL: baseURL ?? `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
    model: 'local/test',
    ...settings,
  }))
  const env = Object.assign(inheritedEnv(), { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })
  return { root, home, project: projectDir, profile, env }
}

export function launchArgs({ profile, linuxArgs = [], ci = process.env.CI, platform = process.platform }) {
  return ['.', ...(ci ? ['--no-sandbox'] : []), ...(platform === 'linux' ? ['--ozone-platform=x11', ...linuxArgs] : []), `--user-data-dir=${profile}`]
}

/**
 * Launch the app on `profile` with `env`, and return it with its first window and the messages of
 * the page errors the window raises. `timeout` is the window's default wait. The app is closed
 * again if the window never comes up.
 */
export async function launchWorld({ profile, env, linuxArgs = [], timeout = 30000, viewport }, start = launchElectron) {
  if (!profile || !env) throw new Error('launchWorld needs a profile and an env, or the app would run on the real ones')
  const app = await start({ args: launchArgs({ profile, linuxArgs }), env, timeout: 40000 })
  try {
    const page = await app.firstWindow()
    page.setDefaultTimeout(timeout)
    if (viewport) await page.setViewportSize(viewport)
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    return { app, page, errors }
  } catch (error) {
    await app.close().catch(() => undefined)
    throw error
  }
}

async function launchElectron(options) {
  const { _electron: electron } = await import('playwright-core')
  return electron.launch(options)
}

/** Answer the native folder picker with `project`, press Open project and answer the trust question. */
export async function openProject(page, app, project, { trust = true } = {}) {
  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const question = page.getByRole('dialog', { name: 'Project trust', exact: true })
  const answer = trust ? question.locator('[data-test="trust-approve"]') : question.getByRole('button', { name: "Don't trust", exact: true })
  await answer.click()
  await question.waitFor({ state: 'hidden' })
}
