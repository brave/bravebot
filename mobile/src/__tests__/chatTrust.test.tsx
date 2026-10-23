import { act } from 'react-test-renderer'
import { AccessibilityInfo, FlatList } from 'react-native'
import type { Status } from '@brave/agent-client'
import { ChatScreen } from '../screens/ChatScreen'
import { SessionController } from '../session/controller'
import { allText, byTestId, has, render } from './helpers'
import { FakeSession } from './support/fake'

function screen(status: Status, trust: Record<string, unknown> | null) {
  const session = new FakeSession({ id: 's', trust: trust as never, initial: { status } })
  return render(
    <ChatScreen controller={new SessionController(session, () => undefined)} target="Test" title={null} suggestions={[]} workspaceName="work" onMenu={() => undefined} />,
  )
}

const QUESTION = { directory: '/work', shownDirectory: '/work', keeping: null, shownKeeping: null }

describe('the startup trust card', () => {
  it('is shown while the view says it is waiting', async () => {
    expect(has(await screen('awaiting_trust', QUESTION), 'trust-card')).toBe(true)
  })

  it('goes as soon as the view says the session is ready, even before the answer is recorded', async () => {
    expect(has(await screen('idle', QUESTION), 'trust-card')).toBe(false)
  })

  it('is not shown when no question was asked', async () => {
    expect(has(await screen('awaiting_trust', null), 'trust-card')).toBe(false)
  })
})

describe('announcing a question to a screen reader', () => {
  it('says the project needs a trust answer when the startup question appears', async () => {
    const spy = jest.spyOn(AccessibilityInfo, 'announceForAccessibility').mockImplementation(() => undefined)
    try {
      await screen('awaiting_trust', QUESTION)
      expect(spy).toHaveBeenCalledWith(expect.stringMatching(/trust/i))
    } finally {
      spy.mockRestore()
    }
  })

  it('says nothing when the status has moved on, even if the question lingers', async () => {
    const spy = jest.spyOn(AccessibilityInfo, 'announceForAccessibility').mockImplementation(() => undefined)
    try {
      await screen('idle', QUESTION)
      expect(spy).not.toHaveBeenCalled()
    } finally {
      spy.mockRestore()
    }
  })
})

describe('typing in the composer', () => {
  it('leaves the transcript as it was, so a long conversation does not redraw on every key', async () => {
    const session = new FakeSession({ id: 's', initial: { status: 'idle' } })
    const controller = new SessionController(session, () => undefined)
    const tree = await render(
      <ChatScreen controller={controller} target="Test" title={null} suggestions={[]} workspaceName="work" onMenu={() => undefined} />,
    )
    const before = tree.root.findByType(FlatList).props.renderItem
    await act(async () => {
      byTestId(tree, 'composer-input').props.onChangeText('hello')
    })
    expect(controller.draft).toBe('hello')
    expect(tree.root.findByType(FlatList).props.renderItem).toBe(before)
  })
})

describe('announcing a question of a kind the app does not know', () => {
  it('uses the general wording, even for a kind named like an object property', async () => {
    const spy = jest.spyOn(AccessibilityInfo, 'announceForAccessibility').mockImplementation(() => undefined)
    try {
      const session = new FakeSession({
        id: 's',
        initial: { turn: 1, target: 1, status: 'waiting', pending: { row: 1, request: 1, kind: 'constructor', supported: false, data: { request: 1 } } },
      })
      await render(
        <ChatScreen controller={new SessionController(session, () => undefined)} target="Test" title={null} suggestions={[]} workspaceName="work" onMenu={() => undefined} />,
      )
      expect(spy).toHaveBeenCalledWith('A question is waiting for your answer.')
    } finally {
      spy.mockRestore()
    }
  })
})

describe('what a yes to the trust question does', () => {
  it('names the file a yes is kept in, as the protocol says the client must', async () => {
    const tree = await screen('awaiting_trust', { directory: '/work', shownDirectory: '/work', keeping: '/Users/me/.bravebot/trust.json', shownKeeping: '/Users/me/.bravebot/trust.json' })
    expect(allText(tree)).toContain('/Users/me/.bravebot/trust.json')
    expect(allText(tree)).toContain('applies to later sessions')
  })

  it('says the answer is not kept where the bridge sends no file', async () => {
    const tree = await screen('awaiting_trust', QUESTION)
    expect(allText(tree)).toContain('This answer is not kept')
  })
})

describe('a trust question that cannot be read', () => {
  it('says how to get out, because no turn has started and so there is none to cancel', async () => {
    const tree = await screen('awaiting_trust', { keeping: null })
    expect(has(tree, 'unanswerable-card')).toBe(true)
    expect(allText(tree)).toContain('start a new session')
    expect(allText(tree)).not.toContain('Cancel the turn')
  })
})
