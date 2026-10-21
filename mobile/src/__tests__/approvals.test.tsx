import type { Pending } from '@brave/agent-client'
import { StyleSheet } from 'react-native'
import { themes } from '../theme'
import { ApprovalPanel, type ApprovalHandlers } from '../components/approvals/ApprovalPanel'
import { pendingOf } from './support/builders'
import { branchQuestion, changelogRequest, fetchRequest, hostileWrite, testRequest } from './support/requests'
import { allText, byTestId, has, isDisabled, press, render, textOf, update } from './helpers'

const REQUESTS: Record<string, Pending> = {
  'update-changelog': pendingOf(changelogRequest),
  'save-fetched-text': pendingOf(hostileWrite),
  'run-tests': pendingOf(testRequest),
  'fetch-release-notes': pendingOf(fetchRequest),
  'pick-branch': pendingOf(branchQuestion),
}

function request(name: string): Pending {
  return REQUESTS[name]!
}

function handlers() {
  const calls: unknown[][] = []
  const value: ApprovalHandlers = {
    onDecide: (...args) => void calls.push(['decide', ...args]),
    onAnswer: (...args) => void calls.push(['answer', ...args]),
    onTrust: (...args) => void calls.push(['trust', ...args]),
  }
  return { value, calls }
}

async function show(pending: Pending | null, startupTrust: unknown = null, busy = false) {
  const { value, calls } = handlers()
  const tree = await render(<ApprovalPanel pending={pending} startupTrust={startupTrust as never} target="Test" busy={busy} handlers={value} />)
  return { tree, calls }
}

describe('write approval', () => {
  it('draws every field of the request', async () => {
    const { tree } = await show(request('update-changelog'))
    const text = allText(tree)
    for (const expected of ['Edit a file', 'Path', 'CHANGELOG.md', 'Changes a file that exists', '3 added, 0 removed', '## 2.4.1', '- Fix the rounding error in the checkout total.', '209 unchanged lines not shown']) {
      expect(text).toContain(expected)
    }
    expect(textOf(byTestId(tree, 'confirm-card-target'))).toBe('Test')
    expect(has(tree, 'untrusted-write')).toBe(false)
  })

  it('draws the optional fields when the request has them', async () => {
    const { tree } = await show(request('save-fetched-text'))
    const text = allText(tree)
    for (const expected of ['Credentials found in the text', 'AWS access key id', 'Line endings', 'Keeps the file as LF', 'Note, 1 lines', 'isolated reader']) {
      expect(text).toContain(expected)
    }
  })

  it('marks a write of untrusted text with words, in a band the text cannot imitate', async () => {
    const { tree } = await show(request('save-fetched-text'))
    expect(has(tree, 'untrusted-write')).toBe(true)
    expect(textOf(byTestId(tree, 'untrusted-write'))).toContain('Untrusted content')
    expect(allText(tree)).toContain('U+202E')
    expect(allText(tree).includes('\u202E')).toBe(false)
  })

  it('replies with the request number it was shown', async () => {
    const pending = request('update-changelog')
    const { tree, calls } = await show(pending)
    await press(tree, 'approve')
    await press(tree, 'reject')
    expect(calls).toEqual([
      ['decide', pending.request, 'approve'],
      ['decide', pending.request, 'reject'],
    ])
  })

  it('offers no answer while a reply is being sent', async () => {
    const { tree, calls } = await show(request('update-changelog'), null, true)
    expect(isDisabled(tree, 'approve')).toBe(true)
    expect(isDisabled(tree, 'reject')).toBe(true)
    await press(tree, 'approve')
    expect(calls).toEqual([])
  })
})

describe('a write\'s changes', () => {
  function background(tree: Awaited<ReturnType<typeof show>>['tree'], label: string): string | undefined {
    const row = tree.root.findAll((node) => node.props.accessibilityLabel === label && typeof node.type === 'string')[0]
    return (StyleSheet.flatten(row?.props.style) as { backgroundColor?: string } | undefined)?.backgroundColor
  }

  it('wash an added line green and a removed line red, and leave a kept line plain', async () => {
    const { tree } = await show(request('save-fetched-text'))
    expect(background(tree, 'Added: # Summary of the forum thread')).toBe(themes.light.status.successBg)
    const edit = await show(request('update-changelog'))
    expect(background(edit.tree, 'Kept: # Changelog')).toBe('transparent')
    const retry = {
      row: 1,
      request: 5,
      kind: 'confirm',
      supported: true,
      data: { request: 5, path: 'a.ts', intent: 'edit', untrusted: false, existing: true, added: 1, removed: 1, exact: true, credentials: [], writtenSinceCheckout: false, changes: [{ kind: 'removed', text: 'old' }, { kind: 'added', text: 'new' }] },
    } as Pending
    const both = await show(retry)
    expect(background(both.tree, 'Removed: old')).toBe(themes.light.status.errorBg)
    expect(background(both.tree, 'Added: new')).toBe(themes.light.status.successBg)
  })

  it('says how many lines were added and removed, in words', async () => {
    const { tree } = await show(request('update-changelog'))
    expect(allText(tree)).toContain('3 added, 0 removed')
  })
})

describe('a card taller than its viewport', () => {
  it('says there is more to read until the end has been reached', async () => {
    const { tree } = await show(request('run-tests'))
    const scroll = byTestId(tree, 'run-card-scroll')
    const { act } = require('react-test-renderer')
    expect(has(tree, 'run-card-more')).toBe(false)
    await act(async () => {
      scroll.props.onLayout({ nativeEvent: { layout: { height: 200 } } })
      scroll.props.onContentSizeChange(0, 600)
    })
    expect(has(tree, 'run-card-more')).toBe(true)
    await act(async () => {
      scroll.props.onScroll({ nativeEvent: { contentOffset: { y: 400 } } })
    })
    expect(has(tree, 'run-card-more')).toBe(false)
  })

  it.each([
    ['run-tests', 'run-card'],
    ['update-changelog', 'confirm-card'],
  ])('keeps Approve off until the end of the %s card has been reached, and never holds back Reject', async (name, card) => {
    const pending = request(name)
    const { tree, calls } = await show(pending)
    const scroll = byTestId(tree, `${card}-scroll`)
    const { act } = require('react-test-renderer')
    await act(async () => {
      scroll.props.onLayout({ nativeEvent: { layout: { height: 200 } } })
      scroll.props.onContentSizeChange(0, 600)
    })
    expect(isDisabled(tree, 'approve')).toBe(true)
    expect(isDisabled(tree, 'reject')).toBe(false)
    await press(tree, 'approve')
    expect(calls).toEqual([])
    await act(async () => {
      scroll.props.onScroll({ nativeEvent: { contentOffset: { y: 400 } } })
    })
    expect(isDisabled(tree, 'approve')).toBe(false)
    await press(tree, 'approve')
    expect(calls).toEqual([['decide', pending.request, 'approve']])
  })

  it('says nothing when everything fits', async () => {
    const { tree } = await show(request('run-tests'))
    const scroll = byTestId(tree, 'run-card-scroll')
    const { act } = require('react-test-renderer')
    await act(async () => {
      scroll.props.onLayout({ nativeEvent: { layout: { height: 500 } } })
      scroll.props.onContentSizeChange(0, 480)
    })
    expect(has(tree, 'run-card-more')).toBe(false)
  })
})

describe('command approval', () => {
  it('draws the command, the program it resolves to, where it runs and what it is limited to', async () => {
    const { tree } = await show(request('run-tests'))
    const text = allText(tree)
    for (const expected of ['Run a command', 'npm test -- --runInBand', 'Directory', '/work/storefront', 'Limited to', 'Cannot reach the network.', 'Approve once']) {
      expect(text).toContain(expected)
    }
    expect(textOf(byTestId(tree, 'run-stage-0-name'))).toContain('Name')
    expect(textOf(byTestId(tree, 'run-stage-0-name'))).toContain('npm')
    expect(textOf(byTestId(tree, 'run-stage-0-resolved'))).toContain('Resolves to')
    expect(textOf(byTestId(tree, 'run-stage-0-resolved'))).toContain('/opt/homebrew/bin/npm')
    expect(textOf(byTestId(tree, 'run-stage-0-binary'))).toContain('Runs')
    expect(textOf(byTestId(tree, 'run-stage-0-binary'))).toContain('/opt/homebrew/Cellar/node/24.1.0/bin/node')
  })

  it('says approval covers this run only and offers no way to remember it', async () => {
    const { tree } = await show(request('run-tests'))
    const text = allText(tree).toLowerCase()
    expect(text).toContain('never remembered')
    expect(text).not.toContain('trust this command')
    expect(textOf(byTestId(tree, 'approve'))).toContain('Approve once')
  })

  it('warns when the command hands private data to a program, and lists lent access', async () => {
    const base = request('run-tests')
    const pending = { ...base, data: { ...(base.data as object), releasesPrivate: true, requestedScopes: ['aws:read'], ambient: [{ authority: 'ssh-agent', named: 'SSH_AUTH_SOCK' }] } } as Pending
    const { tree } = await show(pending)
    const text = allText(tree)
    for (const expected of ['hands your private data', 'aws:read', 'ssh-agent: SSH_AUTH_SOCK']) expect(text).toContain(expected)
  })
})

describe('fetch approval', () => {
  it('shows the host the request connects to apart from the address as sent', async () => {
    const { tree } = await show(request('fetch-release-notes'))
    expect(textOf(byTestId(tree, 'fetch-host-value'))).toContain('releases.example.net')
    expect(textOf(byTestId(tree, 'fetch-host-value'))).not.toContain('docs.example.dev')
    expect(textOf(byTestId(tree, 'fetch-url-value'))).toContain('https://docs.example.dev@releases.example.net/2.4')
  })
})

describe('questions', () => {
  it('offers Send answers only once something is chosen or typed, because nothing answered is the same as Decline', async () => {
    const pending = request('pick-branch')
    const { tree, calls } = await show(pending)
    expect(isDisabled(tree, 'send-answers')).toBe(true)
    await press(tree, 'send-answers')
    expect(calls).toEqual([])
    await press(tree, 'ask-row-0-1')
    expect(isDisabled(tree, 'send-answers')).toBe(false)
  })

  it('enables Send answers for typed text alone, and for one answered question out of two, and says why it is off until then', async () => {
    const pending = request('pick-branch')
    const { tree } = await show(pending)
    expect(has(tree, 'ask-hint')).toBe(true)
    const { act } = require('react-test-renderer')
    await act(async () => {
      byTestId(tree, 'ask-typed-1').props.onChangeText('Keep it short')
    })
    expect(isDisabled(tree, 'send-answers')).toBe(false)
    expect(has(tree, 'ask-hint')).toBe(false)
    await act(async () => {
      byTestId(tree, 'ask-typed-1').props.onChangeText('')
    })
    expect(isDisabled(tree, 'send-answers')).toBe(true)
    await press(tree, 'ask-row-0-1')
    expect(isDisabled(tree, 'send-answers')).toBe(false)
  })

  it('sends the chosen rows and the typed text, one answer per question', async () => {
    const pending = request('pick-branch')
    const { tree, calls } = await show(pending)
    await press(tree, 'ask-row-0-1')
    const { act } = require('react-test-renderer')
    await act(async () => {
      byTestId(tree, 'ask-typed-1').props.onChangeText('Keep it short')
    })
    await press(tree, 'send-answers')
    expect(calls).toEqual([['answer', pending.request, [{ chosen: [1] }, { typed: 'Keep it short' }]]])
  })

  it('declines every question when declined', async () => {
    const pending = request('pick-branch')
    const { tree, calls } = await show(pending)
    await press(tree, 'decline')
    expect(calls).toEqual([['answer', pending.request, [null, null]]])
  })
})

describe('a question with a choice and a typed answer', () => {
  const { act } = require('react-test-renderer')

  it('shows the choice as dropped when text is typed, so what is sent is what is shown', async () => {
    const pending = request('pick-branch')
    const { tree, calls } = await show(pending)
    await press(tree, 'ask-row-0-1')
    expect(byTestId(tree, 'ask-row-0-1').props.accessibilityState.checked).toBe(true)
    await act(async () => {
      byTestId(tree, 'ask-typed-0').props.onChangeText('something else')
    })
    expect(byTestId(tree, 'ask-row-0-1').props.accessibilityState.checked).toBe(false)
    await press(tree, 'send-answers')
    expect(calls).toEqual([['answer', pending.request, [{ typed: 'something else' }, null]]])
  })

  it('clears the typed text when a choice is made', async () => {
    const pending = request('pick-branch')
    const { tree, calls } = await show(pending)
    await act(async () => {
      byTestId(tree, 'ask-typed-0').props.onChangeText('something else')
    })
    await press(tree, 'ask-row-0-0')
    expect(byTestId(tree, 'ask-typed-0').props.value).toBe('')
    await press(tree, 'send-answers')
    expect(calls).toEqual([['answer', pending.request, [{ chosen: [0] }, null]]])
  })
})

describe('questions that share a key', () => {
  it('draws every question and keeps each answer with its own question', async () => {
    const base = request('pick-branch')
    const data = base.data as { prompts: { key: string }[] }
    const same = data.prompts.map((prompt) => ({ ...prompt, key: 'same' }))
    const pending = { ...base, data: { ...(base.data as object), prompts: same } } as Pending
    const { tree, calls } = await show(pending)
    expect(allText(tree)).toContain('Which branch should the fix go on?')
    expect(allText(tree)).toContain('Anything to add to the commit message?')
    await press(tree, 'ask-row-0-1')
    const { act } = require('react-test-renderer')
    await act(async () => {
      byTestId(tree, 'ask-typed-1').props.onChangeText('note')
    })
    await press(tree, 'send-answers')
    expect(calls).toEqual([['answer', pending.request, [{ chosen: [1] }, { typed: 'note' }]]])
  })
})

describe('startup trust', () => {
  it('asks the question and answers only when told to', async () => {
    const { tree, calls } = await show(null, { directory: '/work/notes', keeping: null })
    expect(allText(tree)).toContain('Do you trust this directory?')
    expect(calls).toEqual([])
    await press(tree, 'trust-decline')
    expect(calls).toEqual([['trust', false]])
  })
})

describe('a question that cannot be shown completely', () => {
  it('offers no approve or reject when a required field is missing', async () => {
    const base = request('update-changelog')
    const broken = { ...base, data: { request: base.request, path: 'CHANGELOG.md' } } as Pending
    const { tree } = await show(broken)
    expect(has(tree, 'unanswerable-card')).toBe(true)
    expect(has(tree, 'approve')).toBe(false)
    expect(has(tree, 'reject')).toBe(false)
  })

  it('offers no answer when the payload names another request', async () => {
    const base = request('run-tests')
    const { tree } = await show({ ...base, request: base.request + 1 })
    expect(has(tree, 'unanswerable-card')).toBe(true)
    expect(has(tree, 'approve')).toBe(false)
  })

  it('offers no answer for a kind this device does not answer', async () => {
    const { tree } = await show({ row: 1, request: 4, kind: 'output', supported: false, data: { request: 4 } })
    expect(has(tree, 'unanswerable-card')).toBe(true)
    expect(allText(tree)).toContain('Cancel the turn to refuse this question.')
    expect(has(tree, 'approve')).toBe(false)
  })
})

describe('a command nothing confines', () => {
  const unconfined = (notSandboxed: string | null): Pending => {
    const base = request('run-tests')
    return { ...base, data: { ...(base.data as Record<string, unknown>), confinement: null, notSandboxed } as never }
  }

  it('says so beside the line, so a person is not asked to approve an unconfined command without being told', async () => {
    const { tree } = await show(unconfined('Nothing confines this command.'))
    expect(allText(tree)).toContain('Not confined')
    expect(allText(tree)).toContain('Nothing confines this command.')
    expect(has(tree, 'approve')).toBe(true)
  })

  it('is not offered for an answer when the bridge sent neither a confinement nor the sentence for its absence', async () => {
    const { tree } = await show(unconfined(null))
    expect(has(tree, 'unanswerable-card')).toBe(true)
    expect(has(tree, 'approve')).toBe(false)
  })
})

describe('a second question that follows a first', () => {
  it('opens a new card, not the last one with its scroll position, so the top of a write is the first thing seen', async () => {
    const { value } = handlers()
    const first = request('update-changelog')
    const tree = await render(<ApprovalPanel pending={first} startupTrust={null} target="Test" busy={false} handlers={value} />)
    const scrolled = byTestId(tree, 'confirm-card-scroll')
    const second: Pending = { ...first, request: first.request + 1, data: { ...(first.data as Record<string, unknown>), request: first.request + 1 } as never }
    await update(tree, <ApprovalPanel pending={second} startupTrust={null} target="Test" busy={false} handlers={value} />)
    expect(byTestId(tree, 'confirm-card-scroll')).not.toBe(scrolled)
  })
})
