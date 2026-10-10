// One question of every kind the window puts to a person, each too tall for a small window, for
// PROMPT-4's tests. Each holds tokens, words no other part of its card draws, in the rows an
// approval rests on: the first and last of a list the answer waits on all of, and the first row
// of one it waits on the first row of. `standing` tokens are in rows only a standing answer waits
// on. A kind whose answer waits on nothing says why in `none`.
//
// shown.test.mjs checks that every token is drawn in a row the card marks as deciding, and
// drive-shown.mjs checks in the window that no approval is taken before every token was on screen.

const range = (count) => [...Array(count).keys()]
const pad = (n) => String(n).padStart(2, '0')
const filler = (words) => range(words).map((n) => ['the', 'checker', 'read', 'this', 'and', 'found', 'nothing', 'that', 'gives', 'orders'][n % 10]).join(' ')

export const CASES = {
  confirm: {
    kind: 'confirm',
    event: 'confirm.request',
    request: {
      request: 101, path: 'docs/zqc04-notes.txt', intent: 'edit', untrusted: false, existing: true, added: 81, removed: 0, exact: true,
      lineEndings: 'zqc05 the file keeps its CRLF line endings',
      remark: { preview: ['zqc06 changed blue to green'], lines: 1, label: 'zqc07 from the isolated processor' },
      credentials: range(24).map((n) => `api key at line ${n + 1}: sk-…${pad(n)}${n === 0 ? ' zqc01' : n === 23 ? ' zqc02' : ''}`),
      changes: [{ kind: 'added', text: 'zqc03 is the first line of the change' }, ...range(80).map((n) => ({ kind: 'added', text: `line ${n + 2} of the change` }))],
    },
    tokens: ['zqc01', 'zqc02', 'zqc03', 'zqc04', 'zqc05', 'zqc06', 'zqc07'],
    standing: [],
  },
  run: {
    kind: 'run',
    event: 'run.request',
    request: {
      request: 102,
      stages: range(30).map((n) => ({ program: 'grep', resolved: '/usr/bin/grep', args: [`zqr${pad(n + 1)}`], display: `grep zqr${pad(n + 1)}` })),
      directory: '/work/zqr36', releasesPrivate: false, canBeRemembered: true,
      line: 'grep zqr31 notes.md', writes: ['/work/project/zqr32.txt'], stdin: 'zqr33',
      ambient: [{ authority: 'agent-socket', named: 'zqr34' }], requestedScopes: ['zqr35'],
      vouches: [{ program: 'grep', args: ['zqr99'], display: 'grep zqr99' }], summary: 'search the notes',
    },
    tokens: ['zqr01', 'zqr30', 'zqr31', 'zqr32', 'zqr33', 'zqr34', 'zqr35', 'zqr36'],
    standing: ['zqr99'],
  },
  output: {
    kind: 'output',
    event: 'output.request',
    request: {
      request: 103, command: 'cat zqo03.md', reference: 'output-1', lines: 120, summary: 'the notes',
      output: ['zqo02 is the first line of the output', ...range(119).map((n) => `line ${n + 2} of the output`)].join('\n'),
      vetting: { verdict: 'safe', reason: `zqo01 ${filler(80)}` },
    },
    tokens: ['zqo01', 'zqo02', 'zqo03'],
    standing: [],
  },
  vet: {
    kind: 'vet',
    event: 'vet.request',
    request: {
      request: 104, origin: 'zqv03.md', expects: 'zqv04 release notes', lines: 120,
      content: ['zqv02 is the first line of the content', ...range(119).map((n) => `line ${n + 2} of the content`)].join('\n'),
      vetting: { verdict: 'safe', reason: `zqv01 ${filler(80)}` },
    },
    tokens: ['zqv01', 'zqv02', 'zqv03', 'zqv04'],
    standing: [],
  },
  'vet-picture': {
    kind: 'vet',
    event: 'vet.request',
    request: {
      request: 105, origin: 'zqp03.png', expects: 'zqp04 a screenshot', content: '', lines: 0,
      picture: { path: `/tmp/bravebot-copies/${'copy-'.repeat(60)}zqp02.png`, media: 'image/png', bytes: 2048 },
      vetting: { verdict: 'safe', reason: `zqp01 ${filler(160)}` },
    },
    tokens: ['zqp01', 'zqp02', 'zqp03', 'zqp04'],
    standing: [],
  },
  vouch: {
    kind: 'vouch',
    event: 'vouch.request',
    request: {
      request: 106, path: `vendor/${'deep/'.repeat(40)}zqu01.md`, truncated: true,
      preview: ['zqu03 is the first line of the file', ...range(119).map((n) => `line ${n + 2} of the file`)].join('\n'),
      vetting: { verdict: 'safe', reason: `zqu02 ${filler(80)}` },
    },
    tokens: ['zqu01', 'zqu02', 'zqu03'],
    standing: [],
  },
  fetch: {
    kind: 'fetch',
    event: 'fetch.request',
    request: {
      request: 107, url: `https://zqf01.example/${'docs/'.repeat(300)}`, host: 'zqf02.example', summary: 'read the docs',
      ambient: range(6).map((n) => ({ authority: 'metadata-service', named: n === 5 ? 'zqf03' : `169.254.169.${n}` })),
    },
    tokens: ['zqf01', 'zqf02', 'zqf03'],
    standing: [],
  },
  server: {
    kind: 'server',
    event: 'server.request',
    request: {
      request: 108, language: 'zqs05', program: `/opt/zqs01/${'bin/'.repeat(150)}zqs02`, args: ['--zqs04'], argumentsLine: '--zqs04',
      workspace: `/work/${'project/'.repeat(40)}zqs03`, runsBuildTooling: true, declared: false, summary: 'start the rust server',
    },
    tokens: ['zqs01', 'zqs02', 'zqs03', 'zqs04', 'zqs05'],
    standing: [],
  },
  manifest: {
    kind: 'manifest',
    event: 'manifest.request',
    request: {
      request: 109, task: `zqm00 ${filler(60)}`,
      steps: range(60).map((n) => `${n + 1}. [act] ${n === 0 ? 'zqm01' : n === 59 ? 'zqm60' : 'step'} writes NOTES.md from log`),
    },
    tokens: ['zqm00', 'zqm01', 'zqm60'],
    standing: [],
  },
  exposure: {
    kind: 'exposure',
    event: 'exposure.request',
    request: {
      request: 110, path: 'zqe41.env', summary: 'send the env file',
      credentials: range(40).map((n) => `api key at line ${n + 1}: sk-…${pad(n)}${n === 0 ? ' zqe01' : n === 39 ? ' zqe40' : ''}`),
    },
    tokens: ['zqe01', 'zqe40', 'zqe41'],
    standing: [],
  },
  'mcp-server': {
    kind: 'mcp-server',
    event: 'mcp-server.request',
    request: {
      request: 111, alias: 'zqa07', transport: 'stdio', command: ['zqa01', ...range(20).map((n) => `--flag-${n}`)], url: null,
      program: '/usr/local/bin/zqa02', variables: [{ name: 'ZQA03', stored: true }],
      reads: range(20).map((n) => `/work/data/${n === 19 ? 'zqa04' : `folder-${n}`}`), directory: '/work/zqa05',
      digest: 'sha256:zqa09', requestedBy: 'zqa08/.mcp.json', changed: true, fetching: ['zqa06 fetches the package it runs each time it starts'],
    },
    tokens: ['zqa01', 'zqa02', 'ZQA03', 'zqa04', 'zqa05', 'zqa06', 'zqa07', 'zqa08', 'zqa09'],
    standing: [],
  },
  'mcp-tools': {
    kind: 'mcp-tools',
    event: 'mcp-tools.request',
    request: {
      request: 112, alias: 'zqt31', refused: 0, changed: false, vetting: { verdict: 'safe' },
      tools: range(30).map((n) => ({ name: n === 0 ? 'zqt01' : n === 29 ? 'zqt30' : `tool_${n}`, arguments: ['path'], description: 'Reads a file.' })),
    },
    tokens: ['zqt01', 'zqt30', 'zqt31'],
    standing: [],
  },
  'mcp-call': {
    kind: 'mcp-call',
    event: 'mcp-call.request',
    request: {
      request: 113, alias: 'files', tool: 'read', name: 'files:zqk31', description: 'Reads a file.', mayStand: true,
      arguments: range(30).map((n) => ({ name: `arg${n}`, value: JSON.stringify(n === 0 ? 'zqk01' : n === 29 ? 'zqk30' : `value ${n}`) })),
    },
    tokens: ['zqk01', 'zqk30', 'zqk31'],
    standing: [],
  },
  'mcp-move': {
    kind: 'mcp-move',
    event: 'mcp-move.request',
    request: {
      request: 114, alias: 'files', declared: 'https://zqd01.example/mcp', authority: 'zqd03.example:443', mayRecord: true,
      destination: `https://zqd02.example/${'path/'.repeat(300)}`,
    },
    tokens: ['zqd01', 'zqd02', 'zqd03'],
    standing: [],
  },
  ask: {
    kind: 'ask',
    event: 'ask.request',
    request: {
      request: 115,
      prompts: [{ header: 'Branch', question: 'Which branch should the notes go on?', rows: [{ index: 0, label: 'main', detail: null }, { index: 1, label: 'release', detail: null }], multiple: false, key: 'branch' }],
    },
    tokens: [],
    standing: [],
    none: 'its answer is a choice among the planner’s own options or words the person typed, which grants nothing, and the terminal waits on no row of it either',
  },
}
