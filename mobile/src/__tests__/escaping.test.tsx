import type { Pending, Row, SessionSummary } from '@brave/agent-client'
import { ApprovalPanel } from '../components/approvals/ApprovalPanel'
import { Drawer } from '../components/Drawer'
import { Transcript } from '../components/transcript/Transcript'
import { pendingOf } from './support/builders'
import { branchQuestion, hostileWrite, testRequest } from './support/requests'
import { allText, byTestId, render } from './helpers'

// Characters that hide or reorder text: an override, a zero-width space and a bell.
const OVERRIDE = '\u202E'
const ZERO_WIDTH = '\u200B'
const BELL = '\u0007'
const RAW = [OVERRIDE, ZERO_WIDTH, BELL]

const WORKSPACES = [{ id: 'storefront', name: 'storefront' }]

const REQUESTS: Record<string, Pending> = {
  'run-tests': pendingOf(testRequest),
  'pick-branch': pendingOf(branchQuestion),
  'save-fetched-text': pendingOf(hostileWrite),
}

function request(name: string): Pending {
  return REQUESTS[name]!
}

function withData(base: Pending, patch: Record<string, unknown>): Pending {
  return { ...base, data: { ...(base.data as object), ...patch } } as Pending
}

async function show(pending: Pending) {
  return render(
    <ApprovalPanel pending={pending} startupTrust={null} target="Test" busy={false} handlers={{ onDecide: () => undefined, onAnswer: () => undefined, onTrust: () => undefined }} />,
  )
}

function expectEscaped(text: string, marks: string[]) {
  for (const mark of marks) expect(text).toContain(mark)
  for (const raw of RAW) expect(text.includes(raw)).toBe(false)
}

function labelsOf(tree: Awaited<ReturnType<typeof render>>): string[] {
  return tree.root.findAll((node) => typeof node.props.accessibilityLabel === 'string').map((node) => node.props.accessibilityLabel as string)
}

describe('a command approval draws every field of the program through the escaping text', () => {
  const stage = {
    program: `cu${OVERRIDE}rl`,
    resolved: `/usr/bin/${ZERO_WIDTH}curl`,
    startedAs: `/usr/bin/x${BELL}`,
    binary: `/bin/${OVERRIDE}sh`,
    shownBinary: `/usr/bin/x${BELL} -> /usr/bin/${ZERO_WIDTH}curl`,
    args: [],
    display: 'curl',
  }

  it('writes out what hides or reorders the name and the file the step runs, even where the bridge did not spell it', async () => {
    const tree = await show(withData(request('run-tests'), { stages: [stage] }))
    expectEscaped(allText(tree), ['U+202E', 'U+200B', 'U+0007'])
    for (const id of ['run-stage-0-name', 'run-stage-0-binary']) {
      expect(allText({ root: byTestId(tree, id) } as never)).toContain('U+')
    }
  })

  it('still says when the name did not resolve to a file', async () => {
    const tree = await show(withData(request('run-tests'), { stages: [{ ...stage, resolved: null }] }))
    expect(allText(tree)).toContain('Did not resolve to a file')
  })
})

describe('a question draws the planner words through the escaping text', () => {
  const prompts = [
    {
      header: `Br${OVERRIDE}anch`,
      question: 'Which branch?',
      rows: [{ index: 0, label: `ma${ZERO_WIDTH}in`, detail: `the ${BELL}default` }],
      multiple: false,
      key: 'branch',
    },
  ]

  it('writes out the header, and escapes it in the labels a screen reader speaks', async () => {
    const tree = await show(withData(request('pick-branch'), { prompts }))
    expectEscaped(allText(tree), ['U+202E'])
    const spoken = labelsOf(tree).join('|')
    expectEscaped(spoken, ['U+202E', 'U+200B'])
  })
})

describe('a write draws its changes through the escaping text, and says them escaped to a screen reader', () => {
  it('has no raw override in what is spoken', async () => {
    const tree = await show(request('save-fetched-text'))
    const spoken = labelsOf(tree).filter((label) => label.startsWith('Added'))
    expect(spoken.length).toBeGreaterThan(0)
    expect(spoken.some((label) => label.includes('[U+202E]'))).toBe(true)
    for (const label of spoken) for (const raw of RAW) expect(label.includes(raw)).toBe(false)
  })

  it('escapes the credentials and note label too', async () => {
    const tree = await show(withData(request('save-fetched-text'), { remark: { preview: ['x'], lines: 1, label: `(U,${OVERRIDE}pub)` } }))
    expectEscaped(allText(tree), ['U+202E'])
  })
})

describe('an activity row draws every word of the call through the escaping text', () => {
  const row: Row = {
    id: 1,
    turn: 1,
    kind: 'activity',
    event: 'tool.finished',
    data: { verb: `re${ZERO_WIDTH}ad`, target: 'a.txt', why: '', note: `done${OVERRIDE}reversed`, failed: false, untrusted: false, waitedSeconds: null, changes: [] },
    resolved: false,
  }

  it('writes out the verb and the note, and the spoken label', async () => {
    const tree = await render(<Transcript rows={[row]} status="idle" decisions={new Map()} empty={null} />)
    expectEscaped(allText(tree), ['U+200B', 'U+202E'])
    expectEscaped(labelsOf(tree).join('|'), ['U+200B'])
  })
})

describe('the session list speaks a prompt escaped', () => {
  it('has no raw override in the row label', async () => {
    const sessions: SessionSummary[] = [{ id: 'a', workspace: 'storefront', firstPrompt: `Fix${OVERRIDE}it`, status: 'idle', waiting: false, ended: false }]
    const tree = await render(
      <Drawer open onClose={() => undefined} target={{ label: 'Test', description: 'x' }} sessions={sessions} workspaces={WORKSPACES} workspace="storefront" onWorkspace={() => undefined} activeId={null} onOpenSession={() => undefined} onNewSession={() => undefined} />,
    )
    const label = byTestId(tree, 'session-a').props.accessibilityLabel as string
    expectEscaped(label, ['[U+202E]'])
  })
})
