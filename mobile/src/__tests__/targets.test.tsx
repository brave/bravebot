import { act } from 'react-test-renderer'
import App from '../App'
import { Transcript } from '../components/transcript/Transcript'
import type { Target } from '../targets'
import { allText, byTestId, has, press, render, textOf } from './helpers'
import { fakeTarget } from './support/appScenarios'
import { FakeClient } from './support/fake'

function plainTarget(): Target {
  const client = new FakeClient({ workspaces: [{ id: 'w', name: 'work' }] })
  return fakeTarget(client, { id: 'plain', label: 'Plain', description: 'A target that offers one workspace.', suggestions: ['Try this'] })
}

async function launch(targets: readonly Target[]) {
  const tree = await render(<App targets={targets} />)
  await act(async () => {
    await Promise.resolve()
  })
  return tree
}

describe('an app over a target', () => {
  it('uses that target for the header, the empty state and the session list', async () => {
    const tree = await launch([plainTarget()])
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Plain')
    expect(allText(tree)).toContain('work')
    expect(has(tree, 'suggestion-0')).toBe(true)
    expect(has(tree, 'replay')).toBe(false)
    await press(tree, 'menu')
    expect(has(tree, 'session-new-1')).toBe(true)
    expect(has(tree, 'session-update-changelog')).toBe(false)
  })

  it('offers only the first target', async () => {
    const other: Target = { ...plainTarget(), id: 'second', label: 'Second' }
    const tree = await launch([plainTarget(), other])
    expect(textOf(byTestId(tree, 'topbar-target'))).toBe('Plain')
  })
})

describe('a row of an event the app does not draw itself', () => {
  const row = { id: 1, turn: 1, kind: 'activity' as const, event: 'thing.note', data: { text: 'Hello' }, resolved: false }

  it('is drawn as an ordinary activity that could not be shown', async () => {
    const plain = await render(<Transcript rows={[row]} status="idle" decisions={new Map()} empty={null} />)
    expect(allText(plain)).toContain('An action that could not be shown.')
  })
})
