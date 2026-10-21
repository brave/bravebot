import { readAsk, readConfirm, readFetch, readRun, readShown, readTrust } from '../payloads'

// The payloads the bridge sends for each kind of question (ui/docs/phase-0-rpc-protocol.md, section 8).
const changelog: any = {
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
}

const hostileWrite: any = {
  request: 1,
  path: 'notes/summary.md',
  intent: 'create',
  untrusted: true,
  existing: false,
  added: 1,
  removed: 0,
  exact: true,
  changes: [{ kind: 'added', text: '# Summary of the forum thread' }],
  credentials: ['AWS access key id, masked AKIA…0000, line 3'],
  writtenSinceCheckout: false,
  lineEndings: 'Keeps the file as LF',
  remark: { preview: ['This text was returned by an isolated reader of the forum page.'], lines: 1, label: '(U,pub)' },
}

const runTests: any = {
  request: 1,
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
  directory: '/work/storefront',
  writes: [],
  stdin: null,
  releasesPrivate: false,
  confinement: { heading: 'Limited to', directories: ['/work/storefront'], sentences: ['Cannot reach the network.', 'Can write only inside the project.'] },
  requestedScopes: [],
  ambient: [],
  canBeRemembered: true,
  summary: 'Run the project tests',
}

const fetchNotes: any = {
  request: 1,
  url: 'https://docs.example.dev@releases.example.net/2.4',
  host: 'releases.example.net',
  ambient: [],
  summary: 'Fetch one page to summarize it',
}

const pickBranch: any = {
  request: 1,
  prompts: [
    {
      header: 'Branch',
      question: 'Which branch should the fix go on?',
      rows: [
        { index: 0, label: 'main', detail: 'Commit straight to the default branch' },
        { index: 1, label: 'fix/checkout-rounding', detail: 'A new branch from main' },
      ],
      multiple: false,
      key: 'branch',
    },
    { header: 'Message', question: 'Anything to add to the commit message?', rows: [], multiple: false, key: 'message' },
  ],
}

describe('payload readers', () => {
  it('read every kind of approval completely', () => {
    expect(readConfirm(changelog)).not.toBeNull()
    expect(readConfirm(hostileWrite)?.untrusted).toBe(true)
    expect(readRun(runTests)?.stages[0]?.display).toBe('npm test -- --runInBand')
    expect(readFetch(fetchNotes)?.host).toBe('releases.example.net')
    expect(readAsk(pickBranch)?.prompts).toHaveLength(2)
  })

  it('refuse a payload with a missing or mistyped required field instead of reading part of it', () => {
    const confirm = changelog
    for (const key of ['request', 'path', 'intent', 'untrusted', 'existing', 'added', 'removed', 'exact', 'changes']) {
      const broken = { ...confirm }
      delete broken[key]
      expect(readConfirm(broken)).toBeNull()
    }
    expect(readConfirm({ ...confirm, intent: 'delete' })).toBeNull()
    expect(readConfirm({ ...confirm, changes: [{ kind: 'added' }] })).toBeNull()
    expect(readRun({ ...runTests, stages: [{ program: 'npm' }] })).toBeNull()
    expect(readFetch({ ...fetchNotes, host: 3 })).toBeNull()
    expect(readAsk({ ...pickBranch, prompts: [{ header: 'x' }] })).toBeNull()
    expect(readConfirm(null)).toBeNull()
    expect(readConfirm([])).toBeNull()
  })

  it('refuse an optional field that is present with the wrong type', () => {
    const confirm = changelog
    expect(readConfirm({ ...confirm, credentials: 'AKIA' })).toBeNull()
    expect(readConfirm({ ...confirm, writtenSinceCheckout: 'yes' })).toBeNull()
    expect(readConfirm({ ...confirm, remark: { preview: [], lines: 'one', label: '(U,pub)' } })).toBeNull()
    expect(readRun({ ...runTests, ambient: [{ authority: 'ssh' }] })).toBeNull()
  })

  it('reads trust and released-content payloads', () => {
    expect(readTrust({ directory: '/work', keeping: null })).toEqual({ directory: '/work', keeping: null })
    expect(readTrust({ keeping: null })).toBeNull()
    expect(readShown({ origin: 'o', reach: 'not_the_planner', label: '(U,pub)', preview: ['a'], lines: 1 })?.label).toBe('(U,pub)')
    expect(readShown({ origin: 'o', reach: 'elsewhere', label: '(U,pub)', preview: ['a'], lines: 1 })).toBeNull()
  })
})

describe('a command stage', () => {
  const stage = (extra: object) => ({ ...runTests, stages: [{ program: 'npm', args: [], display: 'npm', ...extra }] })

  it('reads when the optional fields are absent or null', () => {
    expect(readRun(stage({}))?.stages[0]).toMatchObject({ startedAs: null, binary: null })
    expect(readRun(stage({ startedAs: null, binary: null }))?.stages[0]).toMatchObject({ startedAs: null, binary: null })
    expect(readRun(stage({ startedAs: 'npm', binary: '/bin/node' }))?.stages[0]).toMatchObject({ startedAs: 'npm', binary: '/bin/node' })
  })

  it('does not read when an optional field is present and is not text, so the card is not offered without that evidence', () => {
    expect(readRun(stage({ startedAs: 5 }))).toBeNull()
    expect(readRun(stage({ binary: { path: '/bin/node' } }))).toBeNull()
  })
})

describe('a request that leaves out a field the bridge always sends', () => {
  const without = (value: object, key: string) => {
    const copy: Record<string, unknown> = { ...value }
    delete copy[key]
    return copy as never
  }

  it.each(['credentials', 'writtenSinceCheckout'])('is not a complete write request when %s is missing, so the card cannot hide what it grants', (key) => {
    expect(readConfirm(without(changelog, key))).toBeNull()
  })

  it.each(['line', 'plan', 'writes', 'requestedScopes', 'ambient'])('is not a complete command request when %s is missing', (key) => {
    expect(readRun(without(runTests, key))).toBeNull()
  })

  it('is not a complete fetch request when ambient is missing', () => {
    expect(readFetch(without(fetchNotes, 'ambient'))).toBeNull()
  })
})
