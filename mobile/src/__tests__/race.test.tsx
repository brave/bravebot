import { StrictMode } from 'react'
import { act } from 'react-test-renderer'
import type { SessionSummary } from '@brave/agent-client'
import App from '../App'
import { byTestId, has, press, render, textOf } from './helpers'
import { fakeTarget } from './support/appScenarios'
import { narration, prompt, reply } from './support/builders'
import { FakeClient, FakeSession } from './support/fake'

function clientWithSessionA() {
  return new FakeClient({
    workspaces: [{ id: 'w', name: 'work' }],
    sessions: [new FakeSession({ id: 'a', workspace: 'w', firstPrompt: 'Prompt A' })],
    make: (workspace, id) => new FakeSession({ id, workspace }),
  })
}

describe('picking a session while another is still being made', () => {
  it('keeps the session that was picked last on screen, and closes the late one instead of leaving it behind', async () => {
    const client = clientWithSessionA()
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const create = client.createSession.bind(client)
    let made = 0
    // The first session, made at launch, comes at once. Later ones wait for the gate.
    client.createSession = async (options) => {
      made++
      if (made > 1) await gate
      return create(options)
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'slow', label: 'Slow', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    // The session made at launch has something in it, so New session makes another.
    await act(async () => {
      client.created[0]!.emit({ rows: [prompt(1, 1, 'already started')] })
    })
    await press(tree, 'menu')
    await press(tree, 'new-session')
    await press(tree, 'session-a')
    expect(textOf(byTestId(tree, 'chat-screen'))).toContain('Prompt A')

    await act(async () => {
      release()
      await Promise.resolve()
    })

    expect(textOf(byTestId(tree, 'chat-screen'))).toContain('Prompt A')
    expect(client.created.map((session) => session.id)).toEqual(['new-1', 'new-2'])
    expect(client.created[0]!.methods).not.toContain('close')
    expect(client.created[1]!.methods).toEqual(['close'])
  })
})

describe('refreshing the session list while an earlier request is still out', () => {
  it('keeps the newer list when the older answer arrives last', async () => {
    const client = clientWithSessionA()
    let calls = 0
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const list = client.listSessions.bind(client)
    // The first request, made at launch, is answered late and with a list from before the session was made.
    client.listSessions = async (): Promise<readonly SessionSummary[]> => {
      calls++
      if (calls === 1) {
        await gate
        return []
      }
      return list()
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'late', label: 'Late', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    expect(has(tree, 'session-a')).toBe(true)
    await act(async () => {
      release()
      await Promise.resolve()
    })
    expect(has(tree, 'session-a')).toBe(true)
  })
})

describe('a session made after the app is gone', () => {
  it('is closed, not left open with nothing to show it', async () => {
    const client = clientWithSessionA()
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const create = client.createSession.bind(client)
    client.createSession = async (options) => {
      await gate
      return create(options)
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'gone', label: 'Gone', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      tree.unmount()
    })
    await act(async () => {
      release()
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
    })
    expect(client.created[0]?.methods).toEqual(['close'])
  })
})

describe('the session list while a turn streams in', () => {
  it('is fetched again when a row of it would change, not for every row the turn adds', async () => {
    const session = new FakeSession({ id: 's', workspace: 'w', initial: { turn: 1, target: 1, status: 'running', rows: [prompt(1, 1, 'go')] } })
    const client = new FakeClient({ workspaces: [{ id: 'w', name: 'work' }], make: () => session })
    let lists = 0
    const list = client.listSessions.bind(client)
    client.listSessions = async () => {
      lists++
      return list()
    }
    await render(<App targets={[fakeTarget(client, { id: 'streaming', label: 'Streaming', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    const before = lists
    await act(async () => {
      session.emit({ rows: [narration(2, 1, 'one')] })
      session.emit({ rows: [narration(3, 1, 'two')] })
    })
    expect(lists).toBe(before)
    await act(async () => {
      session.emit({ status: 'completed', rows: [reply(4, 1, 'done')] })
    })
    expect(lists).toBe(before + 1)
  })
})

describe('a session the target returns as a new handle', () => {
  it('is the one the screen uses, not a handle kept from the last time the id was opened', async () => {
    const client = clientWithSessionA()
    let handles = 0
    client.openSession = async (id) => {
      handles++
      return new FakeSession({ id, workspace: 'w', firstPrompt: `Handle ${handles}`, initial: { turn: 1, target: 1, rows: [prompt(1, 1, `Handle ${handles}`)] } })
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'handles', label: 'Handles', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await press(tree, 'session-a')
    expect(textOf(byTestId(tree, 'chat-screen'))).toContain('Handle 1')
    await press(tree, 'menu')
    await press(tree, 'new-session')
    await press(tree, 'menu')
    await press(tree, 'session-a')
    expect(textOf(byTestId(tree, 'chat-screen'))).toContain('Handle 2')
  })
})

describe('starting under StrictMode', () => {
  it('still opens the first session when effects run twice', async () => {
    const tree = await render(
      <StrictMode>
        <App targets={[fakeTarget(clientWithSessionA(), { id: 'strict', label: 'Strict', description: 'x', suggestions: [] })]} />
      </StrictMode>,
    )
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
    })
    expect(has(tree, 'chat-screen')).toBe(true)
  })
})

describe('the session list when requests finish out of order', () => {
  it('shows the older answer when the newer request failed, instead of a list that never updates', async () => {
    const client = clientWithSessionA()
    const list = client.listSessions.bind(client)
    const answers: ((value: readonly SessionSummary[]) => void)[] = []
    let calls = 0
    client.listSessions = () => {
      calls++
      if (calls === 1) return new Promise<readonly SessionSummary[]>((resolve) => answers.push(resolve))
      return Promise.reject(new Error('the link dropped'))
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'order', label: 'Order', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await act(async () => {
      answers[0]!(await list())
      await Promise.resolve()
    })
    expect(has(tree, 'session-a')).toBe(true)
  })
})

describe('tapping the session on screen while another is still opening', () => {
  it('stays on the session on screen, because the person chose it after the slow one', async () => {
    const client = clientWithSessionA()
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const open = client.openSession.bind(client)
    client.openSession = async (id) => {
      await gate
      return open(id)
    }
    const tree = await render(<App targets={[fakeTarget(client, { id: 'slow-open', label: 'Slow open', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await press(tree, 'session-a')
    await press(tree, 'menu')
    await press(tree, `session-${client.created[0]!.id}`)
    await act(async () => {
      release()
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
    })
    expect(textOf(byTestId(tree, 'chat-screen'))).not.toContain('Prompt A')
  })
})

describe('opening the session that is already on screen', () => {
  it('only closes the drawer, so the draft being typed is kept and the target is not asked again', async () => {
    const client = clientWithSessionA()
    const tree = await render(<App targets={[fakeTarget(client, { id: 'same', label: 'Same', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await act(async () => {
      byTestId(tree, 'composer-input').props.onChangeText('half a thought')
    })
    await press(tree, 'menu')
    await press(tree, `session-${client.created[0]!.id}`)
    expect(client.opened).toEqual([])
    expect(byTestId(tree, 'composer-input').props.value).toBe('half a thought')
  })
})

describe('a workspace name that hides text', () => {
  it('is written out where it is drawn, so it cannot reorder what is around it', async () => {
    const client = new FakeClient({ workspaces: [{ id: 'w', name: 'ev\u202Eil' }], make: (workspace, id) => new FakeSession({ id, workspace }) })
    const tree = await render(<App targets={[fakeTarget(client, { id: 'names', label: 'Names', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    expect(textOf(byTestId(tree, 'composer-workspace'))).toBe('ev[U+202E]il')
  })
})

describe('New session from a session with nothing in it', () => {
  it('only closes the drawer, so tapping it again does not pile up empty sessions', async () => {
    const client = clientWithSessionA()
    const tree = await render(<App targets={[fakeTarget(client, { id: 'empty', label: 'Empty', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await press(tree, 'new-session')
    await press(tree, 'menu')
    await press(tree, 'new-session')
    expect(client.created).toHaveLength(1)
  })

  it('makes another once the session on screen has something in it', async () => {
    const client = clientWithSessionA()
    const tree = await render(<App targets={[fakeTarget(client, { id: 'started', label: 'Started', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await act(async () => {
      client.created[0]!.emit({ rows: [prompt(1, 1, 'go')] })
    })
    await press(tree, 'menu')
    await press(tree, 'new-session')
    expect(client.created).toHaveLength(2)
  })
})

describe('the title of a session opened before the list says what it is', () => {
  it('comes from the first prompt the screen already holds, not from New session', async () => {
    const client = clientWithSessionA()
    client.openSession = async (id) => new FakeSession({ id, workspace: 'w', initial: { turn: 1, rows: [prompt(1, 1, 'Prompt from rows')] } })
    client.listSessions = async () => []
    const tree = await render(<App targets={[fakeTarget(client, { id: 'title', label: 'Title', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    client.listSessions = async () => [{ id: 'a', workspace: 'w', firstPrompt: null, status: 'idle', waiting: false, ended: false }]
    await press(tree, 'menu')
    await press(tree, 'session-a')
    expect(textOf(byTestId(tree, 'topbar-title'))).toContain('Prompt from rows')
  })
})

describe('New session from a session stuck on an unreadable trust question', () => {
  it('makes another, because that is the way out the card points to', async () => {
    const client = new FakeClient({
      workspaces: [{ id: 'w', name: 'work' }],
      make: (workspace, id) => new FakeSession({ id, workspace, trust: { keeping: null } as never }),
    })
    const tree = await render(<App targets={[fakeTarget(client, { id: 'stuck', label: 'Stuck', description: 'x', suggestions: [] })]} />)
    await act(async () => {
      await Promise.resolve()
    })
    await press(tree, 'menu')
    await press(tree, 'new-session')
    expect(client.created).toHaveLength(2)
  })
})
