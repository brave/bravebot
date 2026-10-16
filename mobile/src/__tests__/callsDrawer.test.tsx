import { callsFrom, type Call } from '../calls'
import { CallsDrawer } from '../components/CallsDrawer'
import { activity, prompt, quarantined } from './support/builders'
import { allText, byTestId, has, press, render, textOf } from './helpers'

const callsWhileRunning = (rows: Parameters<typeof callsFrom>[0]) => callsFrom(rows, true)

function pageCalls(): Call[] {
  return callsWhileRunning([
    prompt(1, 1, 'Summarize the page I saved from the forum'),
    activity(2, 1, { verb: 'fetch', target: 'https://forum.example.org/thread/812', note: '240 lines', untrusted: true }),
    quarantined(3, 1, { origin: 'https://forum.example.org/thread/812', label: '(U,pub)', preview: ['text'], lines: 240 }),
  ])
}

function call(id: number, over: Partial<Call> = {}): Call {
  return { id, verb: 'read', target: `file-${id}.md`, state: 'done', note: '1 line', untrusted: false, ...over }
}

async function show(calls: readonly Call[], open = true) {
  const closed: string[] = []
  const tree = await render(<CallsDrawer open={open} onClose={() => closed.push('close')} calls={calls} />)
  return { tree, closed }
}

describe('the calls drawer', () => {
  it('draws nothing while closed', async () => {
    const { tree } = await show([call(1)], false)
    expect(has(tree, 'calls-drawer')).toBe(false)
  })

  it('lists each call as Verb(target) with a count that equals the rows', async () => {
    const calls = [call(1), call(2, { verb: 'map', target: '../dev/bravebot' }), call(3, { verb: 'list', target: '.' })]
    const { tree } = await show(calls)
    expect(textOf(byTestId(tree, 'calls-count'))).toBe('3')
    expect(textOf(byTestId(tree, 'calls-row-1'))).toContain('Read(file-1.md)')
    expect(textOf(byTestId(tree, 'calls-row-2'))).toContain('Map(../dev/bravebot)')
    expect(textOf(byTestId(tree, 'calls-row-3'))).toContain('List(.)')
    expect(allText(tree)).toContain('Calls made')
    expect(allText(tree)).toContain('Overview')
  })

  it('marks a failed call with a word as well as the colour', async () => {
    const { tree } = await show([call(1), call(2, { state: 'failed' })])
    expect(textOf(byTestId(tree, 'calls-row-2'))).toContain('Failed')
    expect(textOf(byTestId(tree, 'calls-row-1'))).not.toContain('Failed')
  })

  it('labels each row for assistive technology with its call and state', async () => {
    const { tree } = await show([call(1), call(2, { state: 'failed' }), call(3, { state: 'running', note: null })])
    expect(byTestId(tree, 'calls-row-1').props.accessibilityLabel).toBe('Read(file-1.md), done')
    expect(byTestId(tree, 'calls-row-2').props.accessibilityLabel).toBe('Read(file-2.md), failed')
    expect(byTestId(tree, 'calls-row-3').props.accessibilityLabel).toBe('Read(file-3.md), running')
  })

  it('marks a running call and a call on outside content', async () => {
    const { tree } = await show([call(1, { state: 'running', note: null }), call(2, { untrusted: true }), call(3)])
    expect(textOf(byTestId(tree, 'calls-row-1'))).toContain('Running')
    expect(textOf(byTestId(tree, 'calls-row-2'))).toContain('From outside')
    expect(textOf(byTestId(tree, 'calls-row-3'))).not.toContain('Running')
    expect(textOf(byTestId(tree, 'calls-row-3'))).not.toContain('From outside')
  })

  it('shows the calls derived from a session whose page came from outside', async () => {
    const { tree } = await show(pageCalls())
    expect(textOf(byTestId(tree, 'calls-row-2'))).toContain('Fetch(https://forum.example.org/thread/812)')
    expect(textOf(byTestId(tree, 'calls-row-2'))).toContain('From outside')
  })

  it('draws a hostile target escaped, so it cannot reorder what is around it', async () => {
    const { tree } = await show([call(1, { target: 'invoice\u202Etxt.exe' })])
    const text = textOf(byTestId(tree, 'calls-row-1'))
    expect(text).toContain('U+202E')
    expect(text.includes('\u202E')).toBe(false)
    expect(byTestId(tree, 'calls-row-1').props.accessibilityLabel).toBe('Read(invoice[U+202E]txt.exe), done')
  })

  it('keeps each call on one line', async () => {
    const { tree } = await show([call(1, { target: 'a/very/long/path/'.repeat(20) })])
    const lines = tree.root.findAll((node) => node.props.numberOfLines === 1 && typeof node.type === 'string')
    expect(lines.length).toBeGreaterThan(0)
  })

  it('folds the list without losing the count, and unfolds it', async () => {
    const { tree } = await show([call(1), call(2)])
    expect(byTestId(tree, 'calls-toggle').props.accessibilityLabel).toBe('Hide calls made')
    await press(tree, 'calls-toggle')
    expect(has(tree, 'calls-row-1')).toBe(false)
    expect(textOf(byTestId(tree, 'calls-count'))).toBe('2')
    expect(byTestId(tree, 'calls-toggle').props.accessibilityLabel).toBe('Show calls made')
    await press(tree, 'calls-toggle')
    expect(has(tree, 'calls-row-2')).toBe(true)
  })

  it('says so when there are no calls', async () => {
    const { tree } = await show([])
    expect(textOf(byTestId(tree, 'calls-empty'))).toBe('No calls yet.')
    expect(textOf(byTestId(tree, 'calls-count'))).toBe('0')
  })

  it('closes on the scrim', async () => {
    const { tree, closed } = await show([call(1)])
    expect(byTestId(tree, 'calls-scrim').props.accessibilityLabel).toBe('Close calls')
    await press(tree, 'calls-scrim')
    expect(closed).toEqual(['close'])
  })
})
