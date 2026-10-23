import type { Row } from '@brave/agent-client'
import { callLabel, callsFrom } from '../calls'
import { activity as activityRow, narration, prompt, quarantined, reply } from './support/builders'

function activity(id: number, event: string | null, data: unknown, kind: Row['kind'] = 'activity'): Row {
  return { id, turn: 1, kind, event, data: data as Row['data'], resolved: false }
}

const read = { verb: 'read', target: 'a.md', why: '', note: '1 line', failed: false, untrusted: false, waitedSeconds: null, changes: [] }

/** The usual case: the turn the rows belong to is still working. */
const callsWhileRunning = (rows: Row[]) => callsFrom(rows, true)

describe('calls derived from rows', () => {
  it('lists the tool calls of a finished session in order', () => {
    const calls = callsWhileRunning([
      prompt(1, 1, 'Summarize what changed in the last release'),
      activityRow(2, 1, { verb: 'list', target: '.', note: '14 entries' }),
      activityRow(3, 1, { verb: 'read', target: 'CHANGELOG.md', why: 'find the latest release entry', note: '212 lines' }),
      activityRow(4, 1, { verb: 'search', target: '2.4.0, CHANGELOG.md', note: '3 matches' }),
      narration(5, 1, 'Version 2.4.0 is the latest entry.'),
      reply(6, 1, 'Release 2.4.0 fixes a rounding error.'),
    ])
    expect(calls.map((call) => callLabel(call))).toEqual(['List(.)', 'Read(CHANGELOG.md)', 'Search(2.4.0, CHANGELOG.md)'])
    expect(calls[1]).toMatchObject({ id: 3, state: 'done', note: '212 lines', untrusted: false })
  })

  it('marks a call with no note as running', () => {
    const calls = callsWhileRunning([
      prompt(1, 1, 'Index every file in the repository'),
      activityRow(2, 1, { verb: 'map', target: '.', note: '1,310 files' }),
      activityRow(3, 1, { verb: 'read', target: 'package.json', note: '64 lines' }),
      activityRow(4, 1, { verb: 'index', target: 'src/', note: null }),
    ])
    expect(calls.filter((call) => call.state === 'running')).toHaveLength(1)
    expect(calls[calls.length - 1]).toMatchObject({ verb: 'index', target: 'src/', state: 'running', note: null })
  })

  it('marks a failed call and a call on outside content', () => {
    const calls = callsWhileRunning([
      activity(1, 'tool.finished', { ...read, failed: true }),
      activity(2, 'tool.finished', { ...read, target: 'https://example.test/', untrusted: true }),
    ])
    expect(calls.map((call) => [call.state, call.untrusted])).toEqual([['failed', false], ['done', true]])
    const fetched = callsWhileRunning([
      prompt(1, 1, 'Summarize the page I saved from the forum'),
      activityRow(2, 1, { verb: 'fetch', target: 'https://forum.example.org/thread/812', note: '240 lines', untrusted: true }),
      quarantined(3, 1, { origin: 'https://forum.example.org/thread/812', label: '(U,pub)', preview: ['text'], lines: 240 }),
    ])
    expect(fetched[0]).toMatchObject({ verb: 'fetch', untrusted: true, state: 'done' })
  })

  it('reads the started and finished events and nothing else that is an activity', () => {
    const calls = callsWhileRunning([
      activity(1, 'tool.started', { ...read, note: null }),
      activity(2, 'phase.note', { text: 'Not a call' }),
      activity(3, 'tool.finished', read),
      activity(4, 'check.started', read),
      activity(5, null, read),
    ])
    expect(calls.map((call) => call.id)).toEqual([1, 3])
  })

  it('does not treat a row of another kind as a call, even with a tool event name', () => {
    const calls = callsWhileRunning([
      activity(1, 'tool.finished', read, 'narration'),
      activity(2, 'tool.finished', read, 'quarantined'),
      activity(3, 'tool.finished', read, 'reply'),
    ])
    expect(calls).toEqual([])
  })

  it('skips an activity whose payload does not read', () => {
    const calls = callsWhileRunning([
      activity(1, 'tool.finished', { verb: 'read' }),
      activity(2, 'tool.finished', 'read a.md'),
      activity(3, 'tool.finished', { ...read, failed: 'no' }),
      activity(4, 'tool.finished', read),
    ])
    expect(calls.map((call) => call.id)).toEqual([4])
  })

  it('labels a call as the verb with its first letter capitalised and the target in brackets', () => {
    const base = { id: 1, state: 'done', note: null, untrusted: false } as const
    expect(callLabel({ ...base, verb: 'read', target: 'settings.json' })).toBe('Read(settings.json)')
    expect(callLabel({ ...base, verb: 'map', target: '../dev/bravebot' })).toBe('Map(../dev/bravebot)')
    expect(callLabel({ ...base, verb: 'Search', target: 'a, b' })).toBe('Search(a, b)')
  })

  it('leaves the target as it came, whatever it holds', () => {
    const hostile = 'invoice\u202Etxt.exe'
    const [call] = callsWhileRunning([activity(1, 'tool.finished', { ...read, verb: 'read', target: hostile })])
    expect(call?.target).toBe(hostile)
    expect(callLabel(call!)).toBe(`Read(${hostile})`)
  })

  it('lists the target as the bridge spelled it, and as it came where the bridge sent no spelling', () => {
    const rows = [
      activity(1, 'tool.finished', { ...read, verb: 'write', target: 'note.md ', shownTarget: '"note.md\u2423"' }),
      activity(2, 'tool.finished', { ...read, verb: 'write', target: 'note.md' }),
    ]
    expect(callsWhileRunning(rows).map((call) => call.target)).toEqual(['"note.md\u2423"', 'note.md'])
  })

  it('has no calls for a session that has made none, and lists only the one call of a turn that then failed', () => {
    expect(callsWhileRunning([])).toEqual([])
    const failed = callsWhileRunning([
      prompt(1, 1, 'Update the dependencies'),
      activityRow(2, 1, { verb: 'read', target: 'package.json', note: '64 lines' }),
    ])
    expect(failed.map((call) => call.verb)).toEqual(['read'])
  })
})

describe('a call with no finish once its turn has ended', () => {
  const rows = [
    prompt(1, 1, 'Index every file'),
    activityRow(2, 1, { verb: 'index', target: 'src/', note: null }),
    prompt(3, 2, 'Try again'),
    activityRow(4, 2, { verb: 'read', target: 'package.json', note: null }),
  ]

  it('did not finish, instead of staying running for ever, when the turn was cancelled or failed', () => {
    expect(callsFrom(rows, false).map((call) => call.state)).toEqual(['unfinished', 'unfinished'])
  })

  it('is running only in the latest turn while a turn is running', () => {
    expect(callsFrom(rows, true).map((call) => call.state)).toEqual(['unfinished', 'running'])
  })
})

describe('a turn number that was reused after a rewind', () => {
  it('counts the call in the last row as the running one, not the call in the highest-numbered turn', () => {
    const rows = [
      prompt(1, 3, 'old turn'),
      activityRow(2, 3, { verb: 'index', target: 'src/', note: null }),
      prompt(3, 1, 'new turn'),
      activityRow(4, 1, { verb: 'read', target: 'package.json', note: null }),
    ]
    expect(callsFrom(rows, true).map((call) => call.state)).toEqual(['unfinished', 'running'])
  })
})

describe('a finish that carries no note', () => {
  it('is a finished call, because the row says it finished, whatever its note holds', () => {
    const finishedWithoutNote = activity(2, 'tool.finished', { ...read, note: null })
    expect(callsFrom([finishedWithoutNote], true)[0]?.state).toBe('done')
    expect(callsFrom([finishedWithoutNote], false)[0]?.state).toBe('done')
  })
})
