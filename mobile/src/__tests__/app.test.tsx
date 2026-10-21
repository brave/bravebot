import { act } from 'react-test-renderer'
import App from '../App'
import { Drawer } from '../components/Drawer'
import { ChatScreen } from '../screens/ChatScreen'
import { SessionController } from '../session/controller'
import { allText, byTestId, has, isDisabled, press, render, textOf } from './helpers'
import { SUGGESTIONS, WORKSPACES, branchQuestionSession, changelogSession, fakeTarget, finishedSession, newSession, runTestsSession } from './support/appScenarios'
import { activity, prompt } from './support/builders'
import { FakeClient, FakeSession } from './support/fake'

describe('the app with no target', () => {
  // A build with no target must not look like a working session, so nothing that starts or reviews one is offered.
  it('shows a blank slate that says nothing is connected and offers nothing to start', async () => {
    const tree = await render(<App />)
    expect(has(tree, 'no-target')).toBe(true)
    expect(allText(tree)).toContain('Nothing is connected')
    for (const absent of ['chat-screen', 'menu', 'calls', 'topbar-target', 'session-list', 'trust-card']) {
      expect(has(tree, absent)).toBe(false)
    }
  })

  // A disabled control with no explanation reads as broken, so the composer says why it cannot be used.
  it('has a composer that is disabled and says why', async () => {
    const tree = await render(<App />)
    expect(byTestId(tree, 'composer-input').props.editable).toBe(false)
    expect(isDisabled(tree, 'send')).toBe(true)
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('No target is connected.')
    expect(textOf(byTestId(tree, 'composer-workspace'))).toBe('No workspace')
  })
})

async function screen(session: FakeSession) {
  const controller = new SessionController(session, () => undefined)
  const tree = await render(
    <ChatScreen controller={controller} target="Fake" title={null} suggestions={SUGGESTIONS} workspaceName="storefront" onMenu={() => undefined} />,
  )
  return { tree, controller, session }
}

describe('the chat screen', () => {
  it('shows a waiting write, with the composer offering Cancel', async () => {
    const { tree } = await screen(changelogSession())
    expect(has(tree, 'confirm-card')).toBe(true)
    expect(has(tree, 'cancel-turn')).toBe(true)
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Fake')
  })

  it('approves the write, moves on and remembers the answer', async () => {
    const { tree, session } = await screen(changelogSession())
    await press(tree, 'approve')
    expect(session.calls).toEqual([{ method: 'decide', args: [1, 'approve'] }])
    expect(has(tree, 'confirm-card')).toBe(false)
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('Approved')
    expect(allText(tree)).toContain('Added an entry under 2.4.1')
    expect(has(tree, 'send')).toBe(true)
  })

  it('rejects the write and says so', async () => {
    const { tree, session } = await screen(changelogSession())
    await press(tree, 'reject')
    expect(session.calls).toEqual([{ method: 'decide', args: [1, 'reject'] }])
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('Rejected')
    expect(allText(tree)).toContain('I did not change CHANGELOG.md.')
  })

  it('cancels a waiting turn and refuses its question', async () => {
    const { tree, session } = await screen(runTestsSession())
    await press(tree, 'cancel-turn')
    expect(session.methods).toEqual(['cancel'])
    expect(has(tree, 'run-card')).toBe(false)
    expect(allText(tree)).toContain('Cancelled')
  })

  it('starts at the trust question, then offers the suggested prompts and runs one', async () => {
    const { tree, session } = await screen(newSession('storefront', 'new-1'))
    expect(has(tree, 'trust-card')).toBe(true)
    expect(byTestId(tree, 'composer-input').props.editable).toBe(false)
    await press(tree, 'trust-approve')
    expect(session.calls).toEqual([{ method: 'answerTrust', args: [true] }])
    expect(has(tree, 'trust-card')).toBe(false)
    expect(has(tree, 'suggestion-0')).toBe(true)
    await press(tree, 'suggestion-2')
    expect(session.calls[1]).toEqual({ method: 'send', args: [SUGGESTIONS[2]] })
    expect(has(tree, 'run-card')).toBe(true)
    expect(has(tree, 'suggestion-0')).toBe(false)
  })

  it('shows a refusal from the client instead of acting', async () => {
    const { tree, controller } = await screen(changelogSession())
    await act(async () => {
      await controller.decide(99, 'approve')
    })
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('That question is not waiting any more.')
    expect(has(tree, 'confirm-card')).toBe(true)
  })

  it('shows a closed session as closed', async () => {
    const closing = finishedSession()
    closing.on.close = () => closing.end('detached')
    const { tree, session } = await screen(closing)
    await act(async () => {
      await session.close()
    })
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('This session is closed.')
    expect(byTestId(tree, 'composer-input').props.editable).toBe(false)
  })
})

describe('the drawer', () => {
  async function drawer(open = true) {
    const client = new FakeClient({
      workspaces: WORKSPACES,
      sessions: [changelogSession(), branchQuestionSession(), finishedSession()],
    })
    const sessions = await client.listSessions()
    const calls: string[] = []
    const tree = await render(
      <Drawer
        open={open}
        onClose={() => calls.push('close')}
        target={{ label: 'Fake', description: 'A target made for a test.' }}
        sessions={sessions}
        workspaces={WORKSPACES}
        workspace="storefront"
        onWorkspace={(id) => calls.push(`workspace:${id}`)}
        activeId="changelog-summary"
        onOpenSession={(id) => calls.push(`open:${id}`)}
        onNewSession={() => calls.push('new')}
      />,
    )
    return { tree, calls }
  }

  it('shows the target, the sessions, and marks the ones waiting for an answer', async () => {
    const { tree } = await drawer()
    expect(textOf(byTestId(tree, 'drawer-target'))).toBe('Fake')
    expect(has(tree, 'session-update-changelog-attention')).toBe(true)
    expect(has(tree, 'session-changelog-summary-attention')).toBe(false)
    expect(allText(tree)).toContain('Add a changelog entry for the checkout fix')
  })

  it('opens a session, starts a new one, picks a workspace and closes', async () => {
    const { tree, calls } = await drawer()
    await press(tree, 'session-pick-branch')
    await press(tree, 'new-session')
    await press(tree, 'workspace-notes')
    await press(tree, 'drawer-scrim')
    expect(calls).toEqual(['open:pick-branch', 'new', 'workspace:notes', 'close'])
  })

  it('draws nothing while closed', async () => {
    const { tree } = await drawer(false)
    expect(has(tree, 'drawer')).toBe(false)
  })
})

describe('a call left unfinished by a turn that ended', () => {
  it('is not drawn as running once the turn was cancelled', async () => {
    const session = new FakeSession({
      id: 'cancelled-mid-call',
      initial: { turn: 1, target: 1, status: 'cancelled', rows: [prompt(1, 1, 'go'), activity(2, 1, { verb: 'read', target: 'a.ts', note: null })] },
    })
    const { tree } = await screen(session)
    expect(has(tree, 'activity-running-2')).toBe(false)
    expect(has(tree, 'activity-unfinished-2')).toBe(true)
    expect(allText(tree)).toContain('read did not finish')
  })
})

describe('a session whose view ended in the middle of a turn', () => {
  it('shows no call as running and no Working line, because nothing is working any more', async () => {
    const session = new FakeSession({
      id: 'lost',
      initial: { turn: 1, target: 1, status: 'running', rows: [prompt(1, 1, 'go'), activity(2, 1, { verb: 'read', target: 'a.ts', note: null })] },
    })
    const { tree } = await screen(session)
    expect(has(tree, 'activity-running-2')).toBe(true)
    await act(async () => {
      session.end('connection_lost')
    })
    expect(has(tree, 'activity-running-2')).toBe(false)
    expect(has(tree, 'activity-unfinished-2')).toBe(true)
    expect(has(tree, 'working')).toBe(false)
  })
})

describe('the first prompt in the drawer', () => {
  it('is one line, so an ordinary line break is not drawn as an escape', async () => {
    const tree = await render(
      <Drawer
        open
        onClose={() => undefined}
        target={{ label: 'Fake', description: 'A target made for a test.' }}
        sessions={[{ id: 'x', workspace: 'storefront', firstPrompt: 'fix this\nand that', status: 'idle', waiting: false, ended: false }]}
        workspaces={WORKSPACES}
        workspace="storefront"
        onWorkspace={() => undefined}
        activeId={null}
        onOpenSession={() => undefined}
        onNewSession={() => undefined}
      />,
    )
    expect(allText(tree)).toContain('fix this and that')
    expect(allText(tree)).not.toContain('U+000A')
  })
})

describe('what is behind an open panel', () => {
  it('is hidden from a screen reader while the sessions drawer is open, and not otherwise', async () => {
    const client = new FakeClient({ workspaces: WORKSPACES, sessions: [finishedSession()] })
    const tree = await render(<App targets={[fakeTarget(client)]} />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(byTestId(tree, 'content').props.importantForAccessibility).toBe('auto')
    await press(tree, 'menu')
    expect(byTestId(tree, 'content').props.importantForAccessibility).toBe('no-hide-descendants')
    expect(byTestId(tree, 'content').props.accessibilityElementsHidden).toBe(true)
  })

  it('is hidden from a screen reader while the calls panel is open', async () => {
    const { tree } = await screen(finishedSession())
    expect(byTestId(tree, 'chat-content').props.importantForAccessibility).toBe('auto')
    await press(tree, 'calls')
    expect(byTestId(tree, 'chat-content').props.importantForAccessibility).toBe('no-hide-descendants')
  })
})

describe('the drawer\'s project names', () => {
  it('are written out where they are drawn, even when the caller did not do it', async () => {
    const tree = await render(
      <Drawer
        open
        onClose={() => undefined}
        target={{ label: 'Fake', description: 'x' }}
        sessions={[]}
        workspaces={[{ id: 'a', name: 'ev\u202Eil' }, { id: 'b', name: 'other' }]}
        workspace="a"
        onWorkspace={() => undefined}
        activeId={null}
        onOpenSession={() => undefined}
        onNewSession={() => undefined}
      />,
    )
    expect(textOf(byTestId(tree, 'workspace-a'))).toContain('U+202E')
  })
})
