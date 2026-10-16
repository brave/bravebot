import App from '../App'
import { allText, byTestId, has, isDisabled, render, textOf } from './helpers'

describe('the app with no target', () => {
  it('shows a blank slate that says nothing is connected and offers nothing to start', async () => {
    const tree = await render(<App />)
    expect(has(tree, 'no-target')).toBe(true)
    expect(allText(tree)).toContain('Nothing is connected')
    for (const absent of ['chat-screen', 'menu', 'replay', 'calls', 'topbar-target', 'session-list', 'trust-card']) {
      expect(has(tree, absent)).toBe(false)
    }
  })

  it('has a composer that is disabled and says why', async () => {
    const tree = await render(<App />)
    expect(byTestId(tree, 'composer-input').props.editable).toBe(false)
    expect(isDisabled(tree, 'send')).toBe(true)
    expect(textOf(byTestId(tree, 'composer-note'))).toBe('No target is connected.')
    expect(textOf(byTestId(tree, 'composer-workspace'))).toBe('No workspace')
  })
})
