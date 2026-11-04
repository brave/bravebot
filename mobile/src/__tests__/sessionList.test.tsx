import { act } from 'react-test-renderer'
import type { SessionSummary, TargetInfo } from '@brave/agent-client'
import App from '../App'
import { has, press, render, textOf, byTestId } from './helpers'
import { fakeTarget } from './support/appScenarios'
import { FakeClient, FakeSession } from './support/fake'

function client(sessionList = true, sessions: FakeSession[] = []) {
  return new FakeClient({
    workspaces: [{ id: 'w', name: 'work' }],
    sessions,
    make: (workspace, id) => new FakeSession({ id, workspace }),
    sessionList,
  })
}

async function launch(target: FakeClient) {
  const tree = await render(<App targets={[fakeTarget(target, { id: 'list', label: 'List', description: 'x', suggestions: [] })]} />)
  await settle()
  return tree
}

async function settle() {
  await act(async () => {
    await new Promise<void>((resolve) => setTimeout(resolve, 0))
  })
}

const OLD = new FakeSession({ id: 'old', workspace: 'w', firstPrompt: 'An older session' })

describe('the drawer for a target that does not list sessions', () => {
  it('says so instead of an empty list, and never asks for one', async () => {
    const target = client(false)
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(textOf(byTestId(tree, 'session-list-unsupported'))).toBe('This target does not keep a list of sessions.')
    expect(has(tree, 'session-list')).toBe(false)
    expect(target.asked.listSessions).toBe(0)
    expect(target.asked.describe).toBe(1)
  })
})

describe('the drawer when listing fails', () => {
  it('says the sessions could not be listed when none has arrived, not that there are none', async () => {
    const target = client()
    target.listSessions = () => Promise.reject(new Error('the link dropped'))
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(textOf(byTestId(tree, 'session-list-empty'))).toBe('Sessions could not be listed.')
  })

  it('keeps the last list with a note while the newest request fails, and drops the note once one succeeds', async () => {
    const target = client(true, [OLD])
    const list = target.listSessions.bind(target)
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(has(tree, 'session-old')).toBe(true)
    expect(has(tree, 'session-list-stale')).toBe(false)

    target.listSessions = () => Promise.reject(new Error('the link dropped'))
    await press(tree, 'drawer-scrim')
    await press(tree, 'menu')
    await settle()
    expect(has(tree, 'session-old')).toBe(true)
    expect(textOf(byTestId(tree, 'session-list-stale'))).toBe('The list could not be refreshed. It shows the sessions last listed.')

    target.listSessions = list
    await press(tree, 'drawer-scrim')
    await press(tree, 'menu')
    await settle()
    expect(has(tree, 'session-old')).toBe(true)
    expect(has(tree, 'session-list-stale')).toBe(false)
  })

  it('asks the target again whether it lists sessions after that question failed', async () => {
    const target = client(true, [OLD])
    const describe = target.describe.bind(target)
    let failures = 1
    target.describe = (): Promise<TargetInfo> => (failures-- > 0 ? Promise.reject(new Error('no answer')) : describe())
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(has(tree, 'session-old')).toBe(true)
    expect(target.asked.describe).toBe(1)
  })

  it('shows the failure of a question about listing in the same way as a failed list', async () => {
    const target = client(true, [OLD])
    target.describe = () => Promise.reject(new Error('no answer'))
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(textOf(byTestId(tree, 'session-list-empty'))).toBe('Sessions could not be listed.')
    expect(target.asked.listSessions).toBe(0)
  })
})

describe('the drawer for a target that lists no sessions', () => {
  it('says there are none', async () => {
    const target = client()
    target.listSessions = async (): Promise<readonly SessionSummary[]> => []
    const tree = await launch(target)
    await press(tree, 'menu')
    await settle()
    expect(textOf(byTestId(tree, 'session-list-empty'))).toBe('No sessions yet.')
  })
})
