import { act } from 'react-test-renderer'
import App from '../App'
import { byTestId, press, render } from './helpers'
import { fakeTarget } from './support/appScenarios'
import { FakeClient, FakeSession } from './support/fake'

function target() {
  const client = new FakeClient({
    workspaces: [
      { id: 'store', name: 'storefront' },
      { id: 'notes', name: 'notes' },
    ],
    make: (workspace, id) => new FakeSession({ id, workspace }),
  })
  client.listSessions = () => Promise.reject(new Error('the list is not available'))
  return fakeTarget(client, { id: 'w', label: 'W', description: 'x', suggestions: [] })
}

describe('the workspace a session is shown in', () => {
  it('is the one it was made in, not the one the drawer has selected for the next session', async () => {
    const tree = await render(<App targets={[target()]} />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(byTestId(tree, 'composer-workspace').props.accessibilityLabel).toBe('Project storefront')
    await press(tree, 'menu')
    await press(tree, 'workspace-notes')
    expect(byTestId(tree, 'composer-workspace').props.accessibilityLabel).toBe('Project storefront')
  })
})
