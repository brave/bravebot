import { StyleSheet } from 'react-native'
import { Composer, composerState } from '../components/Composer'
import { byTestId, has, isDisabled, press, render, textOf } from './helpers'

function props(over: Partial<Parameters<typeof Composer>[0]> = {}) {
  const calls: string[] = []
  return {
    calls,
    value: {
      draft: '',
      onDraft: (text: string) => void calls.push(`draft:${text}`),
      status: 'idle' as const,
      ended: null,
      sending: false,
      workspace: 'storefront',
      onSend: () => void calls.push('send'),
      onCancel: () => void calls.push('cancel'),
      error: null,
      ...over,
    },
  }
}

describe('composer', () => {
  it('sends only when there is text', async () => {
    const empty = props()
    const tree = await render(<Composer {...empty.value} />)
    expect(isDisabled(tree, 'send')).toBe(true)
    await press(tree, 'send')
    expect(empty.calls).toEqual([])
    const filled = props({ draft: 'hello' })
    const tree2 = await render(<Composer {...filled.value} />)
    expect(isDisabled(tree2, 'send')).toBe(false)
    await press(tree2, 'send')
    expect(filled.calls).toEqual(['send'])
  })

  it.each(['running', 'waiting'] as const)('turns Send into Cancel while the turn is %s and keeps the draft editable', async (status) => {
    const { calls, value } = props({ status, draft: 'next thing' })
    const tree = await render(<Composer {...value} />)
    expect(has(tree, 'send')).toBe(false)
    expect(byTestId(tree, 'cancel-turn').props.accessibilityLabel).toBe('Cancel')
    expect(byTestId(tree, 'composer-input').props.value).toBe('next thing')
    expect(byTestId(tree, 'composer-input').props.editable).toBe(true)
    await press(tree, 'cancel-turn')
    expect(calls).toEqual(['cancel'])
  })

  it('draws one round control for both states, so nothing moves when Send becomes Cancel', async () => {
    const send = await render(<Composer {...props({ draft: 'x' }).value} />)
    const cancel = await render(<Composer {...props({ status: 'running' }).value} />)
    const box = (tree: typeof send, id: string) => {
      const { width, height, borderRadius } = StyleSheet.flatten(byTestId(tree, id).props.style)
      return { width, height, borderRadius }
    }
    expect(box(send, 'send')).toEqual({ width: 36, height: 36, borderRadius: 18 })
    expect(box(cancel, 'cancel-turn')).toEqual(box(send, 'send'))
    expect(byTestId(send, 'send').props.accessibilityLabel).toBe('Send')
  })

  it('names the folder the session works in under the message box', async () => {
    const tree = await render(<Composer {...props().value} />)
    expect(textOf(byTestId(tree, 'composer-workspace'))).toBe('storefront')
    expect(byTestId(tree, 'composer-workspace').props.accessibilityLabel).toBe('Project storefront')
  })

  it('says why it cannot take a message', async () => {
    expect(composerState('awaiting_trust', null)).toEqual({ editable: false, reason: 'Answer the trust question to start.' })
    expect(composerState('detached', null).editable).toBe(false)
    expect(composerState('completed', 'The connection was lost. Start a new session to continue.').editable).toBe(false)
    const tree = await render(<Composer {...props({ status: 'awaiting_trust' }).value} />)
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('Answer the trust question to start.')
    expect(byTestId(tree, 'composer-input').props.editable).toBe(false)
  })

  it('shows an error instead of the reason', async () => {
    const tree = await render(<Composer {...props({ error: 'A turn is already running.' }).value} />)
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('A turn is already running.')
  })
})
