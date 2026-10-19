import { StaleActionError, type Row } from '@brave/agent-client'
import type { Target } from '../../targets'
import { FakeClient, FakeSession } from './fake'
import { activity, approval, failure, narration, pendingOf, prompt, reply, resolved } from './builders'

export const WORKSPACES = [
  { id: 'storefront', name: 'storefront' },
  { id: 'notes', name: 'notes' },
] as const

export const SUGGESTIONS = [
  'Explain what this project does',
  'Add a retry to the upload function',
  'Run the tests',
  'Read the release notes page',
] as const

const FIRST = { turn: 1, target: 1 }

/** Refuses a reply for a question that is not on screen, as the client does before it sends anything. */
export function refuseStale(session: FakeSession, request: number): void {
  const pending = session.view.pending
  if (pending === null || pending.request !== request) throw new StaleActionError(`request ${request} is not the question on screen`)
}

export function changelogRequest(): Row {
  return approval(3, 1, 'confirm', {
    request: 1,
    path: 'CHANGELOG.md',
    intent: 'edit',
    untrusted: false,
    existing: true,
    added: 3,
    removed: 0,
    exact: true,
    credentials: [],
    writtenSinceCheckout: false,
    changes: [
      { kind: 'kept', text: '# Changelog' },
      { kind: 'kept', text: '' },
      { kind: 'added', text: '## 2.4.1' },
      { kind: 'added', text: '- Fix the rounding error in the checkout total.' },
      { kind: 'added', text: '' },
      { kind: 'elided', lines: 209 },
    ],
  })
}

/** A write waiting for approval. Approving or rejecting it plays the rest of the turn. */
export function changelogSession(): FakeSession {
  const request = changelogRequest()
  const session = new FakeSession({
    id: 'update-changelog',
    workspace: 'storefront',
    firstPrompt: 'Add a changelog entry for the checkout fix',
    initial: {
      ...FIRST,
      status: 'waiting',
      rows: [prompt(1, 1, 'Add a changelog entry for the checkout fix'), activity(2, 1, { verb: 'read', target: 'CHANGELOG.md', note: '212 lines' }), request],
      pending: pendingOf(request),
    },
  })
  session.on.decide = (asked, decision) => {
    refuseStale(session, asked)
    session.emit({ status: 'running', pending: null, rows: [resolved(request)] })
    if (decision === 'approve') {
      session.emit({ rows: [activity(4, 1, { verb: 'write', target: 'CHANGELOG.md', note: '3 lines added' })] })
      session.emit({ status: 'completed', rows: [reply(5, 1, 'Added an entry under 2.4.1 that describes the rounding fix.')] })
    } else {
      session.emit({ status: 'completed', rows: [reply(4, 1, 'Understood. I did not change CHANGELOG.md.')] })
    }
  }
  return session
}

export function runRequest(id: number, turn: number, request: number): Row {
  return approval(id, turn, 'run', {
    request,
    line: 'npm test -- --runInBand',
    plan: 'npm test -- --runInBand',
    stages: [
      {
        program: 'npm',
        resolved: '/opt/homebrew/bin/npm',
        startedAs: '/opt/homebrew/bin/npm',
        binary: '/opt/homebrew/Cellar/node/24.1.0/bin/node',
        args: ['test', '--', '--runInBand'],
        display: 'npm test -- --runInBand',
      },
    ],
    directory: '/Users/test/storefront',
    writes: [],
    stdin: null,
    releasesPrivate: false,
    confinement: { heading: 'Limited to', directories: ['/Users/test/storefront'], sentences: ['Cannot reach the network.', 'Can write only inside the project.'] },
    requestedScopes: [],
    ambient: [],
    canBeRemembered: true,
    summary: 'Run the project tests',
  })
}

/** A command waiting for approval. Cancelling the turn refuses it. */
export function runTestsSession(): FakeSession {
  const request = runRequest(3, 1, 1)
  const session = new FakeSession({
    id: 'run-tests',
    workspace: 'storefront',
    firstPrompt: 'Run the tests',
    initial: {
      ...FIRST,
      status: 'waiting',
      rows: [prompt(1, 1, 'Run the tests'), narration(2, 1, "I'll run the test suite from the project directory."), request],
      pending: pendingOf(request),
    },
  })
  session.on.cancel = () => {
    session.emit({ status: 'cancelled', pending: null, rows: [resolved(request), failure(4, 1, 'cancelled', 'The turn was cancelled and the command was not run.')] })
    return { cancelled: true }
  }
  return session
}

export function finishedSession(): FakeSession {
  return new FakeSession({
    id: 'changelog-summary',
    workspace: 'storefront',
    firstPrompt: 'Summarize what changed in the last release',
    initial: {
      ...FIRST,
      status: 'completed',
      rows: [
        prompt(1, 1, 'Summarize what changed in the last release'),
        activity(2, 1, { verb: 'read', target: 'CHANGELOG.md', why: 'find the latest release entry', note: '212 lines' }),
        reply(3, 1, 'Release 2.4.0 fixes a rounding error in the checkout total.'),
      ],
    },
  })
}

export function branchQuestionSession(): FakeSession {
  const question = approval(3, 1, 'ask', {
    request: 1,
    prompts: [{ header: 'Branch', question: 'Which branch should the fix go on?', rows: [{ index: 0, label: 'main', detail: 'Commit straight to the default branch' }], multiple: false, key: 'branch' }],
  })
  return new FakeSession({
    id: 'pick-branch',
    workspace: 'storefront',
    firstPrompt: 'Fix the rounding bug and commit it',
    initial: { ...FIRST, status: 'waiting', rows: [prompt(1, 1, 'Fix the rounding bug and commit it'), question], pending: pendingOf(question) },
  })
}

/** A turn still in progress when the session is opened, in the middle of the call on row 7. */
export function longRunningSession(): FakeSession {
  const session = new FakeSession({
    id: 'long-running',
    workspace: 'storefront',
    firstPrompt: 'Index every file in the repository',
    initial: {
      ...FIRST,
      status: 'running',
      rows: [
        prompt(1, 1, 'Index every file in the repository'),
        activity(2, 1, { verb: 'map', target: '.', note: '1,310 files' }),
        activity(3, 1, { verb: 'list', target: 'src', note: '48 entries' }),
        activity(4, 1, { verb: 'read', target: 'package.json', note: '64 lines' }),
        activity(5, 1, { verb: 'read', target: 'tsconfig.json', note: '21 lines' }),
        activity(6, 1, { verb: 'search', target: 'export default, src', note: '37 matches' }),
        activity(7, 1, { verb: 'index', target: 'src/', note: null }),
      ],
    },
  })
  session.on.cancel = () => {
    session.emit({
      status: 'cancelled',
      rows: [activity(7, 1, { verb: 'index', target: 'src/', note: 'stopped', failed: true }), failure(13, 1, 'cancelled', 'The turn was cancelled.')],
    })
    return { cancelled: true }
  }
  return session
}

/** A new session, asking for project trust first, then playing the turn a suggested prompt starts. */
export function newSession(workspace: string, id: string): FakeSession {
  const session = new FakeSession({ id, workspace, trust: { directory: `/Users/test/${workspace}`, keeping: null } })
  session.on.answerTrust = () => {
    session.startupTrust = null
    session.emit({ status: 'idle' })
  }
  session.on.send = (text) => {
    const said = prompt(1, 1, text)
    if (text === SUGGESTIONS[2]) {
      const request = runRequest(3, 1, 12)
      session.emit({ ...FIRST, status: 'running', rows: [said, activity(2, 1, { verb: 'read', target: 'package.json', why: 'find the test command', note: '64 lines' })] })
      session.emit({ status: 'waiting', rows: [request], pending: pendingOf(request) })
    } else {
      session.emit({ ...FIRST, status: 'running', rows: [said] })
    }
    return FIRST
  }
  return session
}

/** Every session a target opens at launch is a new one that asks for trust. */
export function fakeClient(sessions: FakeSession[] = []): FakeClient {
  return new FakeClient({ workspaces: WORKSPACES, sessions, make: newSession })
}

export function fakeTarget(client: FakeClient, overrides: Partial<Target> = {}): Target {
  return { id: 'fake', label: 'Fake', description: 'A target made for a test.', suggestions: SUGGESTIONS, create: () => client, ...overrides }
}
