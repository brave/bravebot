import { act } from 'react-test-renderer'
import App from '../App'
import { allText, byTestId, has, press, render, textOf } from './helpers'
import { changelogSession, fakeClient, fakeTarget, finishedSession, longRunningSession } from './support/appScenarios'
import { reply } from './support/builders'
import type { FakeClient } from './support/fake'

async function launch(client: FakeClient = fakeClient([changelogSession(), longRunningSession(), finishedSession()])) {
  const tree = await render(<App targets={[fakeTarget(client)]} />)
  await act(async () => {
    await Promise.resolve()
  })
  return { tree, client }
}

describe('the app', () => {
  it('opens on a new session in the target that waits for the trust answer', async () => {
    const { tree } = await launch()
    expect(has(tree, 'chat-screen')).toBe(true)
    expect(has(tree, 'trust-card')).toBe(true)
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Fake')
    expect(allText(tree)).toContain('New session')
  })

  it('opens the drawer from the menu and opens a waiting session from the list', async () => {
    const { tree } = await launch()
    await press(tree, 'menu')
    expect(has(tree, 'drawer')).toBe(true)
    expect(has(tree, 'session-update-changelog-attention')).toBe(true)
    await press(tree, 'session-update-changelog')
    expect(has(tree, 'confirm-card')).toBe(true)
    expect(has(tree, 'trust-card')).toBe(false)
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Fake')
  })

  it('starts another new session from the drawer without losing the earlier ones', async () => {
    const { tree } = await launch()
    await press(tree, 'menu')
    await press(tree, 'workspace-notes')
    await press(tree, 'new-session')
    expect(has(tree, 'trust-card')).toBe(true)
    await press(tree, 'menu')
    expect(has(tree, 'session-new-1')).toBe(true)
    expect(has(tree, 'session-new-2')).toBe(true)
    expect(has(tree, 'session-update-changelog')).toBe(true)
  })

  it('opens the calls made drawer from the header and lists the session calls', async () => {
    const { tree } = await launch()
    await press(tree, 'menu')
    await press(tree, 'session-long-running')
    expect(has(tree, 'calls-drawer')).toBe(false)

    await press(tree, 'calls')

    expect(has(tree, 'calls-drawer')).toBe(true)
    expect(textOf(byTestId(tree, 'calls-count'))).toBe('6')
    expect(allText(tree)).toContain('Read(package.json)')
    expect(allText(tree)).toContain('Index(src/)')
    await press(tree, 'calls-scrim')
    await act(async () => {
      await Promise.resolve()
    })
  })

  it('shows the calls made of a session with none yet as empty', async () => {
    const { tree } = await launch()
    await press(tree, 'calls')
    expect(has(tree, 'calls-empty')).toBe(true)
    expect(textOf(byTestId(tree, 'calls-count'))).toBe('0')
  })

  it('keeps the session list up to date while the drawer is open', async () => {
    const running = longRunningSession()
    const { tree } = await launch(fakeClient([running]))
    await press(tree, 'menu')
    await press(tree, 'session-long-running')
    await press(tree, 'menu')
    expect(byTestId(tree, 'session-long-running').props.accessibilityLabel).toContain('Working')
    await act(async () => {
      running.emit({ status: 'completed', rows: [reply(12, 1, 'The index covers 1,310 files.')] })
      await Promise.resolve()
    })
    expect(byTestId(tree, 'session-long-running').props.accessibilityLabel).toContain('Done')
  })
})

describe('cancelling a turn that is in the middle of a call', () => {
  it('does not go on showing the call as running', async () => {
    const { tree } = await launch()
    await press(tree, 'menu')
    await press(tree, 'session-long-running')
    expect(has(tree, 'activity-running-7')).toBe(true)
    await press(tree, 'cancel-turn')
    expect(has(tree, 'activity-running-7')).toBe(false)
    expect(has(tree, 'row-error-13')).toBe(true)
  })
})

describe('answering the trust question in a new session', () => {
  it('sends the answer it was given and no longer asks', async () => {
    const declined = await launch()
    await press(declined.tree, 'trust-decline')
    expect(declined.client.created[0]!.calls).toEqual([{ method: 'answerTrust', args: [false] }])
    expect(has(declined.tree, 'trust-card')).toBe(false)

    const trusted = await launch()
    await press(trusted.tree, 'trust-approve')
    expect(trusted.client.created[0]!.calls).toEqual([{ method: 'answerTrust', args: [true] }])
    expect(has(trusted.tree, 'trust-card')).toBe(false)
  })
})
