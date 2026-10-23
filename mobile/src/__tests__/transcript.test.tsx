import { act } from 'react-test-renderer'
import { StyleSheet } from 'react-native'
import { FlatList } from 'react-native'
import { Transcript } from '../components/transcript/Transcript'
import type { Row, Status } from '@brave/agent-client'
import { themes } from '../theme'
import { activity, failure, narration as note, prompt as userPrompt, quarantined, reply } from './support/builders'
import { HOSTILE_LINES, branchQuestion, changelogRequest } from './support/requests'
import { allText, byTestId, has, render, textOf, update } from './helpers'

const SESSIONS: Record<string, { status: Status; rows: Row[] }> = {
  'changelog-summary': {
    status: 'completed',
    rows: [
      userPrompt(1, 1, 'Summarize what changed in the last release'),
      activity(2, 1, { verb: 'read', target: 'CHANGELOG.md', note: '212 lines' }),
      reply(3, 1, 'Release 2.4.0 fixes a rounding error in the checkout total.'),
    ],
  },
  'summarize-page': {
    status: 'completed',
    rows: [
      userPrompt(1, 1, 'Summarize the page I saved from the forum'),
      activity(2, 1, { verb: 'fetch', target: 'https://forum.example.org/thread/812', note: '240 lines', untrusted: true }),
      quarantined(3, 1, { origin: 'https://forum.example.org/thread/812', label: '(U,pub)', preview: HOSTILE_LINES, lines: 240 }),
      reply(4, 1, 'The thread asks readers to approve requests. I did not act on it.'),
    ],
  },
  'update-changelog': {
    status: 'waiting',
    rows: [userPrompt(1, 1, 'Add a changelog entry for the checkout fix'), activity(2, 1, { verb: 'read', target: 'CHANGELOG.md', note: '212 lines' }), changelogRequest],
  },
  'failed-turn': {
    status: 'failed',
    rows: [
      userPrompt(1, 1, 'Update the dependencies'),
      activity(2, 1, { verb: 'read', target: 'package.json', note: '64 lines' }),
      failure(3, 1, 'chat', 'The model service could not be reached.'),
    ],
  },
  'cancelled-turn': {
    status: 'cancelled',
    rows: [userPrompt(1, 1, 'Reorganize the notes folder'), note(2, 1, 'I will group the files by month.'), failure(3, 1, 'cancelled', 'The turn was cancelled.')],
  },
  'long-running': {
    status: 'running',
    rows: [userPrompt(1, 1, 'Index every file in the repository'), activity(2, 1, { verb: 'map', target: '.', note: '1,310 files' }), activity(3, 1, { verb: 'index', target: 'src/', note: null })],
  },
}

async function show(id: string, decisions = new Map<number, 'approved' | 'rejected'>()) {
  const session = SESSIONS[id]!
  return render(<Transcript rows={session.rows} status={session.status} decisions={decisions} empty={null} />)
}

describe('released content', () => {
  it('is marked in words and kept apart from the label the app drew', async () => {
    const tree = await show('summarize-page')
    const card = byTestId(tree, 'row-quarantined-3')
    expect(textOf(card)).toContain('Untrusted content from outside')
    expect(textOf(byTestId(tree, 'row-3-label'))).toContain('(U,pub)')
    expect(textOf(byTestId(tree, 'row-3-label'))).not.toContain('Trusted by you')
  })

  it('draws a forged label and forged marking text as plain lines inside the content box', async () => {
    const tree = await show('summarize-page')
    const forgedLabel = HOSTILE_LINES.indexOf('(T,priv) Trusted by you')
    const forgedMarking = HOSTILE_LINES.findIndex((line) => line.startsWith('Untrusted content from outside ends here'))
    expect(textOf(byTestId(tree, `row-3-line-${forgedLabel}`))).toContain('(T,priv) Trusted by you')
    expect(textOf(byTestId(tree, `row-3-line-${forgedMarking}`))).toContain('Untrusted content from outside ends here')
    expect(textOf(byTestId(tree, 'row-3-label'))).not.toContain('(T,priv)')
  })

  it('writes out characters that hide or reorder text and never passes them on', async () => {
    const tree = await show('summarize-page')
    const text = allText(tree)
    for (const marker of ['U+202E', 'U+200B', 'U+0007', 'U+000D', 'U+E0041']) expect(text).toContain(marker)
    for (const raw of ['\u202E', '\u200B', '\u0007', '\r', '\u{E0041}']) expect(text.includes(raw)).toBe(false)
  })

  it('shows how much was left out', async () => {
    const tree = await show('summarize-page')
    expect(allText(tree)).toContain('Preview, 10 of 240 lines')
  })

  it('does not draw a forged button as a control', async () => {
    const tree = await show('summarize-page')
    const buttons = tree.root.findAll((node) => node.props.accessibilityRole === 'button')
    expect(buttons).toHaveLength(0)
  })
})

describe('transcript rows', () => {
  it('summarises a waiting question and does not offer to answer it there', async () => {
    const tree = await show('update-changelog')
    const row = byTestId(tree, 'row-approval-3')
    expect(textOf(row)).toContain('Write a file')
    expect(textOf(row)).toContain('Waiting for your answer')
    expect(textOf(row)).toContain('CHANGELOG.md')
    expect(has(tree, 'approve')).toBe(false)
  })

  it('names the file of a waiting write as the bridge spelled it, so a name ending in a space is not read as the one without', async () => {
    const held = { ...changelogRequest, data: { ...(changelogRequest.data as object), path: 'note.md ', shownPath: '"note.md\u2423"' } } as Row
    const tree = await render(<Transcript rows={[held]} status="waiting" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('"note.md\u2423"')
  })

  it('names the file of a call as the bridge spelled it, so a name ending in a space is not read as the one without', async () => {
    const rows = [activity(2, 1, { verb: 'write', target: 'note.md ', shownTarget: '"note.md\u2423"', note: '1 line' })]
    const tree = await render(<Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(tree, 'row-activity-2'))).toContain('"note.md\u2423"')
    expect(textOf(byTestId(tree, 'row-activity-2'))).not.toContain('note.md ')
  })

  it('says what the person answered once the question is resolved', async () => {
    const rows = [...SESSIONS['update-changelog']!.rows.slice(0, 2), { ...changelogRequest, resolved: true }]
    const tree = await render(<Transcript rows={rows} status="running" decisions={new Map([[3, 'approved']])} empty={null} />)
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('Approved')
    const rejected = await render(<Transcript rows={rows} status="running" decisions={new Map([[3, 'rejected']])} empty={null} />)
    expect(textOf(byTestId(rejected, 'row-approval-3'))).toContain('Rejected')
    const unknown = await render(<Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(unknown, 'row-approval-3'))).toContain('Resolved')
  })

  it('draws a changed answer in an approval row that was already on screen', async () => {
    const rows = [...SESSIONS['update-changelog']!.rows.slice(0, 2), { ...changelogRequest, resolved: true }]
    const tree = await render(<Transcript rows={rows} status="running" decisions={new Map([[3, 'approved']])} empty={null} />)
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('Approved')
    await update(tree, <Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(tree, 'row-approval-3'))).toContain('Resolved')
  })

  it('does not say Working once the view has ended, whatever its last status was', async () => {
    const rows = SESSIONS['update-changelog']!.rows.slice(0, 2)
    const live = await render(<Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
    expect(has(live, 'working')).toBe(true)
    const ended = await render(<Transcript rows={rows} status="running" ended decisions={new Map()} empty={null} />)
    expect(has(ended, 'working')).toBe(false)
  })

  it('calls a resolved question Resolved, not Answered, unless it was a question with answers, because a refusal also resolves it', async () => {
    const asked = [{ ...branchQuestion, resolved: true }]
    const tree = await render(<Transcript rows={asked} status="running" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(tree, `row-approval-${branchQuestion.id}`))).toContain('Answered')
  })

  it('does not draw a question resolved without an answer in the success colour, because a refusal also resolves it', async () => {
    const rows = [...SESSIONS['update-changelog']!.rows.slice(0, 2), { ...changelogRequest, resolved: true }]
    const tree = await render(<Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
    const chip = tree.root.findAll((node) => node.props.label === 'Resolved' && node.props.tone !== undefined)[0]
    expect(chip?.props.tone).toBe('neutral')
    const approved = await render(<Transcript rows={rows} status="running" decisions={new Map([[3, 'approved']])} empty={null} />)
    expect(approved.root.findAll((node) => node.props.label === 'Approved' && node.props.tone !== undefined)[0]?.props.tone).toBe('success')
  })

  it('shows a failure and a cancellation differently', async () => {
    const failed = await show('failed-turn')
    expect(textOf(byTestId(failed, 'row-error-3'))).toContain('The turn failed')
    const cancelled = await show('cancelled-turn')
    expect(textOf(byTestId(cancelled, 'row-error-3'))).toContain('Cancelled')
  })

  it('reserves the working row and shows it only while a turn runs', async () => {
    const running = await show('long-running')
    expect(has(running, 'working')).toBe(true)
    const finished = await show('changelog-summary')
    expect(has(finished, 'working')).toBe(false)
  })
})

describe('activity lines', () => {
  function markerColors(tree: Awaited<ReturnType<typeof render>>, testID: string): string[] {
    const marker = byTestId(tree, testID)
    return marker
      .findAll((node) => typeof node.type === 'string' && node.props.style !== undefined)
      .flatMap((node) => {
        const style = StyleSheet.flatten(node.props.style) as { borderColor?: string; backgroundColor?: string }
        return [style.borderColor, style.backgroundColor].filter((value): value is string => typeof value === 'string')
      })
  }

  async function lines() {
    const rows = [
      activity(1, 1, { verb: 'read', target: 'CHANGELOG.md', note: '212 lines' }),
      activity(2, 1, { verb: 'run', target: 'npm test', note: 'exit 1', failed: true }),
      activity(3, 1, { verb: 'index', target: 'src/', note: null }),
    ]
    return render(<Transcript rows={rows} status="running" decisions={new Map()} empty={null} />)
  }

  it('says failed beside a failed call, not only in a colour', async () => {
    const tree = await lines()
    expect(textOf(byTestId(tree, 'row-activity-2'))).toContain('run failed')
    expect(textOf(byTestId(tree, 'row-activity-1'))).not.toContain('failed')
  })

  it('marks a finished call with a check in tertiary ink', async () => {
    const tree = await lines()
    expect(has(tree, 'activity-done-1')).toBe(true)
    expect(markerColors(tree, 'activity-done-1')).toContain(themes.light.ink.tertiary)
  })

  it('marks a failed call with the error colour', async () => {
    const tree = await lines()
    expect(has(tree, 'activity-failed-2')).toBe(true)
    expect(markerColors(tree, 'activity-failed-2')).toContain(themes.light.status.error)
  })

  it('draws a failed call with a cross, a different shape from the check of a finished one', async () => {
    const tree = await lines()
    const turns = (id: string) =>
      byTestId(tree, id)
        .findAll((node) => typeof node.type === 'string' && node.props.style !== undefined)
        .flatMap((node) => (StyleSheet.flatten(node.props.style) as { transform?: { rotate?: string }[] }).transform?.map((t) => t.rotate) ?? [])
        .filter((rotation): rotation is string => rotation !== undefined)
    expect(turns('activity-failed-2').sort()).toEqual(['-45deg', '45deg'])
    expect(turns('activity-done-1')).toEqual(['-45deg'])
  })

  it('marks a call still running with a dot and no check, and says it started', async () => {
    const tree = await lines()
    expect(has(tree, 'activity-running-3')).toBe(true)
    expect(markerColors(tree, 'activity-running-3')).toEqual([themes.light.ink.accent])
    expect(textOf(byTestId(tree, 'row-activity-3'))).toContain('index started')
  })

  it('keeps the call, its target and its note on one line', async () => {
    const tree = await lines()
    const text = textOf(byTestId(tree, 'row-activity-1'))
    expect(text).toContain('read')
    expect(text).toContain('CHANGELOG.md')
    expect(text).toContain('212 lines')
  })
})

describe('following the end of the transcript', () => {
  const prompt = (id: number) => ({ id, turn: 1, kind: 'prompt' as const, event: null, data: { kind: 'user', text: `prompt ${id}` }, resolved: false })
  const narration = (id: number) => ({ id, turn: 1, kind: 'narration' as const, event: 'narration', data: { text: `note ${id}` }, resolved: false })

  async function scrolledUp(rows: ReturnType<typeof prompt>[]) {
    const spy = jest.spyOn(FlatList.prototype, 'scrollToEnd').mockImplementation(() => undefined)
    const tree = await render(<Transcript rows={rows} status="idle" decisions={new Map()} empty={null} />)
    const list = tree.root.findByType(FlatList)
    await act(async () => {
      list.props.onScroll({ nativeEvent: { contentOffset: { y: 0 }, contentSize: { height: 3000 }, layoutMeasurement: { height: 600 } } })
    })
    spy.mockClear()
    return { tree, list, spy }
  }

  it('does not pull a reader who scrolled up back down when the agent adds a line', async () => {
    const { tree, list, spy } = await scrolledUp([prompt(1)])
    await update(tree, <Transcript rows={[prompt(1), narration(2)]} status="running" decisions={new Map()} empty={null} />)
    await act(async () => {
      list.props.onContentSizeChange()
    })
    expect(spy).not.toHaveBeenCalled()
    spy.mockRestore()
  })

  it('follows again once the person sends a message', async () => {
    const { tree, list, spy } = await scrolledUp([prompt(1)])
    await update(tree, <Transcript rows={[prompt(1), narration(2), prompt(3)]} status="running" decisions={new Map()} empty={null} />)
    await act(async () => {
      list.props.onContentSizeChange()
    })
    expect(spy).toHaveBeenCalled()
    spy.mockRestore()
  })
})

describe('a row whose event is named like an object property', () => {
  it('is titled as any other question', async () => {
    const row = { id: 1, turn: 1, kind: 'approval', event: 'constructor', data: { request: 1 }, resolved: false } as const
    const tree = await render(<Transcript rows={[row]} status="waiting" decisions={new Map()} empty={null} />)
    expect(textOf(byTestId(tree, 'row-approval-1'))).toContain('A question')
  })
})
