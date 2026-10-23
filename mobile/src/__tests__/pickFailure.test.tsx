import { act } from 'react-test-renderer'
import App from '../App'
import { byTestId, has, press, render, textOf } from './helpers'
import { fakeClient, fakeTarget } from './support/appScenarios'
import { FakeSession } from './support/fake'

function target() {
  const client = fakeClient([new FakeSession({ id: 'a', workspace: 'storefront', firstPrompt: 'Prompt A' })])
  client.openSession = () => Promise.reject(new Error('gone'))
  return fakeTarget(client, { id: 'p', label: 'P', description: 'x', suggestions: [] })
}

describe('a session that cannot be opened from the drawer', () => {
  it('says so in the drawer, and the session on screen stays', async () => {
    const tree = await render(<App targets={[target()]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    expect(has(tree, 'drawer-problem')).toBe(false)
    await press(tree, 'session-a')
    expect(has(tree, 'drawer-problem')).toBe(true)
    expect(textOf(byTestId(tree, 'drawer-problem'))).toContain('Something went wrong')
    expect(has(tree, 'chat-screen')).toBe(true)
  })

  it('does not go on saying so once the drawer is opened again', async () => {
    const tree = await render(<App targets={[target()]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await press(tree, 'session-a')
    await press(tree, 'drawer-scrim')
    await press(tree, 'menu')
    expect(has(tree, 'drawer-problem')).toBe(false)
  })
})
