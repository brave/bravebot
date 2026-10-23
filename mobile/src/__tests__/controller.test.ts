import { RpcError, StaleActionError, UnsupportedError, type AgentSession } from '@brave/agent-client'
import { SessionController, describeError } from '../session/controller'
import { FakeSession } from './support/fake'
import { approval, pendingOf, prompt, resolved } from './support/builders'

const ask = approval(5, 1, 'ask', {
  request: 2,
  prompts: [{ header: 'Branch', question: 'Which branch?', rows: [{ index: 0, label: 'main', detail: null }], multiple: false, key: 'branch' }],
})

const SUGGESTION = 'Summarize what changed in the last release'

const changelog = approval(3, 1, 'confirm', {
  request: 1,
  path: 'CHANGELOG.md',
  intent: 'edit',
  untrusted: false,
  existing: true,
  added: 1,
  removed: 0,
  exact: true,
  credentials: [],
  writtenSinceCheckout: false,
  changes: [{ kind: 'added', text: '## 2.4.1' }],
})

/** A session with a write waiting for an answer, which the runtime moves on from when it is answered or refused. */
function waiting() {
  const session = new FakeSession({
    id: 'update-changelog',
    initial: { turn: 1, target: 1, status: 'waiting', rows: [prompt(1, 1, 'Add a changelog entry'), changelog], pending: pendingOf(changelog) },
  })
  session.on.decide = (request) => {
    if (request !== session.view.pending?.request) throw new StaleActionError(`request ${request} is not the question on screen`)
    session.emit({ status: 'running', pending: null, rows: [resolved(changelog)] })
  }
  session.on.cancel = () => {
    session.emit({ status: 'cancelled', pending: null, rows: [resolved(changelog)] })
    return { cancelled: true }
  }
  // The runtime accepts a prompt only when no turn is working or waiting.
  session.on.send = () => {
    if (session.view.status === 'running' || session.view.status === 'waiting') throw new RpcError('turn_in_flight', 'a turn is already running', 'rejected')
    session.emit({ status: 'running' })
  }
  return session
}

function open() {
  const session = waiting()
  let activity = 0
  const controller = new SessionController(session, () => activity++)
  return { session, controller, activity: () => activity }
}

describe('session controller', () => {
  it('remembers an answer against the row it was given for, not the request number', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    await controller.decide(session.view.pending!.request, 'approve')
    expect(controller.decisions.get(row)).toBe('approved')
    expect(controller.decisions.size).toBe(1)
  })

  it('records a rejection as a rejection', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    await controller.decide(session.view.pending!.request, 'reject')
    expect(controller.decisions.get(row)).toBe('rejected')
    expect(session.calls[0]).toEqual({ method: 'decide', args: [1, 'reject'] })
  })

  it('sends the request number the card showed, so an answer for a question that has moved on is refused', async () => {
    const { session, controller } = open()
    const shown = session.view.pending!.request
    await controller.decide(shown + 5, 'approve')
    expect(session.calls[0]).toEqual({ method: 'decide', args: [shown + 5, 'approve'] })
    expect(controller.error).toBe(describeError(new StaleActionError('x')))
    expect(controller.decisions.size).toBe(0)
    expect(session.view.pending?.request).toBe(shown)
  })

  it('keeps the draft when a send is refused and clears it once a send is accepted', async () => {
    const { session, controller } = open()
    controller.setDraft('another thing')
    await controller.send()
    expect(controller.draft).toBe('another thing')
    expect(controller.error).toBe('A turn is already running.')
    await session.cancel()
    await controller.send()
    expect(controller.draft).toBe('')
    expect(controller.error).toBeNull()
  })

  it('sends suggested text without touching the draft', async () => {
    const session = new FakeSession({ id: 'new', initial: { status: 'idle' } })
    session.on.send = () => {
      session.emit({ status: 'running' })
    }
    const controller = new SessionController(session, () => undefined)
    controller.setDraft('half written')
    await controller.send(SUGGESTION)
    expect(session.calls).toEqual([{ method: 'send', args: [SUGGESTION] }])
    expect(controller.draft).toBe('half written')
    expect(session.view.status).toBe('running')
  })

  it('already holds the answer when the view first shows the question resolved', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    const seen: (string | undefined)[] = []
    session.subscribe((view) => {
      if (view.pending === null) seen.push(controller.decisions.get(row))
    })
    await controller.decide(session.view.pending!.request, 'approve')
    expect(seen[0]).toBe('approved')
  })

  it('forgets the answer again when the reply is refused', async () => {
    const stub = {
      id: 's',
      view: { pending: { request: 1, row: 7, kind: 'confirm', supported: true, data: {} } },
      decide: async () => {
        throw new Error('refused')
      },
    } as unknown as AgentSession
    const controller = new SessionController(stub, () => undefined)
    await controller.decide(1, 'approve')
    expect(controller.decisions.has(7)).toBe(false)
    expect(controller.error).not.toBeNull()
  })

  it('tells the list to refresh after each action', async () => {
    const { controller, activity } = open()
    await controller.cancel()
    expect(activity()).toBe(1)
  })

  it('does not let a reply in flight hold back a cancel', async () => {
    const calls: string[] = []
    const never = new Promise<void>(() => undefined)
    const stub = {
      id: 's',
      view: { pending: { request: 1, row: 1, kind: 'confirm', supported: true, data: {} } },
      decide: () => {
        calls.push('decide')
        return never
      },
      cancel: async () => {
        calls.push('cancel')
        return { cancelled: true }
      },
    } as unknown as AgentSession
    const controller = new SessionController(stub, () => undefined)
    void controller.decide(1, 'approve')
    expect(controller.replying).toBe(true)
    await controller.cancel()
    expect(calls).toEqual(['decide', 'cancel'])
  })

  it('refuses a second reply while one is being sent', async () => {
    const calls: number[] = []
    const never = new Promise<void>(() => undefined)
    const stub = {
      id: 's',
      view: { pending: { request: 1, row: 1, kind: 'confirm', supported: true, data: {} } },
      decide: (request: number) => {
        calls.push(request)
        return never
      },
    } as unknown as AgentSession
    const controller = new SessionController(stub, () => undefined)
    void controller.decide(1, 'approve')
    await controller.decide(1, 'reject')
    expect(calls).toEqual([1])
  })
})

describe('the draft while a send is in flight', () => {
  it('keeps what was typed after the sent text and takes only the sent text out of the draft', async () => {
    const session = new FakeSession({ id: 's' })
    let accept!: () => void
    session.on.send = () =>
      new Promise<void>((resolve) => {
        accept = resolve
      })
    const controller = new SessionController(session, () => undefined)
    controller.setDraft('fix the bug')
    const sending = controller.send()
    controller.setDraft('fix the bug now')
    accept()
    await sending
    expect(controller.draft).toBe(' now')
  })
})

describe('the error note', () => {
  it('goes away once a cancel succeeds, because the situation it described has ended', async () => {
    const { session, controller } = open()
    await controller.send('another thing')
    expect(controller.error).toBe('A turn is already running.')
    await controller.cancel()
    expect(session.methods).toContain('cancel')
    expect(controller.error).toBeNull()
  })
})

describe('sending a suggestion', () => {
  it('leaves the person\'s own typing alone even when it starts with the suggestion', async () => {
    const session = new FakeSession({ id: 's' })
    const controller = new SessionController(session, () => undefined)
    controller.setDraft('Explain this repo please')
    await controller.send('Explain this repo')
    expect(controller.draft).toBe('Explain this repo please')
  })
})

describe('sending a suggestion that matches the draft', () => {
  it('leaves the draft alone, because it was not sent from the draft', async () => {
    const session = new FakeSession({ id: 's' })
    const controller = new SessionController(session, () => undefined)
    controller.setDraft('Explain this repo')
    await controller.send('Explain this repo')
    expect(controller.draft).toBe('Explain this repo')
  })
})

describe('an answer whose reply failed', () => {
  it('is kept when the view already shows the question resolved, because the reply may have taken effect', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    session.on.decide = () => {
      session.emit({ status: 'running', pending: null, rows: [resolved(changelog)] })
      throw new RpcError('connection_lost', 'the link dropped', 'unknown')
    }
    await controller.decide(session.view.pending?.request ?? 1, 'approve')
    expect(controller.decisions.get(row)).toBe('approved')
  })
})

describe('an answer whose reply was refused', () => {
  it('is dropped even when the view shows the question resolved, because a refused reply took no effect', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    session.on.decide = () => {
      session.emit({ status: 'cancelled', pending: null, rows: [resolved(changelog)] })
      throw new StaleActionError('request 1 is not the question on screen')
    }
    await controller.decide(session.view.pending?.request ?? 1, 'approve')
    expect(controller.decisions.has(row)).toBe(false)
  })
})

describe('which failures drop a recorded answer', () => {
  function failing(error: Error, resolveFirst: boolean) {
    const { session, controller } = open()
    const row = session.view.pending!.row
    session.on.decide = () => {
      if (resolveFirst) session.emit({ status: 'cancelled', pending: null, rows: [resolved(changelog)] })
      throw error
    }
    return { session, controller, row }
  }

  it.each([
    ['a reply the bridge rejected', new RpcError('busy', 'refused', 'rejected')],
    ['a reply that was never sent', new UnsupportedError('cannot reply: the view ended')],
  ])('drops the answer for %s, even when the view shows the question resolved', async (_name, error) => {
    const { session, controller, row } = failing(error, true)
    await controller.decide(session.view.pending?.request ?? 1, 'approve')
    expect(controller.decisions.has(row)).toBe(false)
  })

  it('drops the answer for a failure of unknown outcome while the question is still waiting', async () => {
    const { session, controller, row } = failing(new RpcError('connection_lost', 'the link dropped', 'unknown'), false)
    await controller.decide(session.view.pending?.request ?? 1, 'approve')
    expect(controller.decisions.has(row)).toBe(false)
  })
})

describe('a cancel that stopped nothing', () => {
  it('tells the person, because the turn had already ended or another had begun', async () => {
    const { session, controller } = open()
    session.on.cancel = () => ({ cancelled: false })
    await controller.cancel()
    expect(controller.error).toBe('That turn had already ended, so nothing was stopped.')
  })
})

describe('repeated Cancel', () => {
  it('sends one cancel while the first is still on its way, so a double tap does not stop a later turn', async () => {
    const { session, controller } = open()
    let release!: () => void
    session.on.cancel = () =>
      new Promise((resolve) => {
        release = () => resolve({ cancelled: true })
      })
    const first = controller.cancel()
    const second = controller.cancel()
    release()
    await Promise.all([first, second])
    expect(session.methods.filter((method) => method === 'cancel')).toHaveLength(1)
  })
})

describe('a cancel while a question is waiting', () => {
  it('records the question as rejected, because cancelling refuses it', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    await controller.cancel()
    expect(controller.decisions.get(row)).toBe('rejected')
  })

  it('records nothing when the cancel stopped nothing', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    session.on.cancel = () => ({ cancelled: false })
    await controller.cancel()
    expect(controller.decisions.has(row)).toBe(false)
  })
})

describe('declining a question', () => {
  function asking() {
    const session = new FakeSession({
      id: 'ask',
      initial: { turn: 1, target: 1, status: 'waiting', rows: [ask], pending: pendingOf(ask) },
    })
    return { session, controller: new SessionController(session, () => undefined), row: ask.id }
  }

  it('records a question declined with no answers as rejected, not as answered', async () => {
    const { session, controller, row } = asking()
    await controller.answer(session.view.pending!.request, [null])
    expect(controller.decisions.get(row)).toBe('rejected')
  })

  it('records nothing for a question that was given an answer', async () => {
    const { session, controller, row } = asking()
    await controller.answer(session.view.pending!.request, [{ typed: 'main' }])
    expect(controller.decisions.has(row)).toBe(false)
  })
})

describe('a cancel after an answer was given', () => {
  it('leaves the answer alone, so an approval in flight is not turned into a rejection', async () => {
    const { session, controller } = open()
    const row = session.view.pending!.row
    let release!: () => void
    session.on.decide = () =>
      new Promise<void>((resolve) => {
        release = resolve
      })
    const deciding = controller.decide(session.view.pending!.request, 'approve')
    await controller.cancel()
    release()
    await deciding
    expect(controller.decisions.get(row)).toBe('approved')
  })
})

describe('declining a question with no prompts', () => {
  it('records nothing, because there was nothing to decline', async () => {
    const empty = approval(6, 1, 'ask', { request: 3, prompts: [] })
    const session = new FakeSession({ id: 'empty-ask', initial: { turn: 1, target: 1, status: 'waiting', rows: [empty], pending: pendingOf(empty) } })
    const controller = new SessionController(session, () => undefined)
    await controller.answer(3, [])
    expect(controller.decisions.has(6)).toBe(false)
  })
})
