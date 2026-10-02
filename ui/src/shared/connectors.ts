/**
 * Connectors: MCP servers the person declares, approves and turns on from the window.
 *
 * The catalog here is a set of forms. Each one builds the declaration its setup guide tells a
 * person to type at `bravebot mcp add`, from the few values the guide asks them for. Nothing in the
 * catalog is trusted for anything: what a form builds goes to the agent, which resolves it, shows
 * it back with its fingerprint, and declares and approves exactly that only on the person's
 * "Connect" (docs/specs/mcp-servers.md SERVERS-3).
 */

/** One variable a connector receives: stored with a value, read from the environment, or kept. */
export interface ConnectorVariable {
  name: string
  /** Stored in the declaration as given. */
  value?: string
  /** Keep the value the existing declaration stores under this name. */
  keep?: boolean
}

/** What a form sends the agent. */
export interface ConnectorForm {
  alias: string
  transport: 'http' | 'stdio'
  url?: string
  command?: string[]
  variables?: ConnectorVariable[]
  directory?: string
}

/** A declared connector, as the agent reports it. A stored value is named, never sent. */
export interface Connector {
  alias: string
  transport?: 'http' | 'stdio'
  command?: string[] | null
  url?: string | null
  variables?: { name: string; stored: boolean }[]
  reads?: string[]
  directory?: string | null
  digest?: string
  approved?: boolean
  requested: boolean
  connected: boolean
  /** The approved declaration was edited since, so it asks again before it starts. */
  changed?: boolean
  /** Why this machine's administrator keeps it from starting. */
  refused?: string | null
  /** Why the declaration cannot be used, where it cannot. */
  problem: string | null
}

export interface ConnectorList {
  /** The person's home directory, which a `~/` in a catalog default stands for. */
  home: string | null
  state: string | null
  writable: boolean
  unavailable: string | null
  connectors: Connector[]
}

/** A declaration resolved from a form, put to the person before it is connected. */
export interface ConnectorPreview extends Connector {
  fingerprint: string
  exists: boolean
  same: boolean
  fetching: string[]
}

/** One value a catalog form asks for. */
export interface ConnectorField {
  key: string
  label: string
  /** A secret is stored in `~/.bravebot/mcp.json` and never shown again. */
  kind: 'path' | 'secret' | 'text'
  /** A `~/` here stands for the home directory. */
  initial?: string
  help: string
}

export interface CatalogEntry {
  alias: string
  name: string
  icon: string
  summary: string
  /** Where to read how to set it up. Opened in the browser. */
  guide: string
  /** What has to be done outside the app first. */
  before: string
  fields: ConnectorField[]
  /** The form these values describe. `kept` names the secrets left blank to keep. */
  build: (values: Record<string, string>, kept: Set<string>) => ConnectorForm
  /** The values a declared connector was set up with, for its settings page. Never a secret. */
  values: (connector: Connector) => Record<string, string>
}

const DOCS = 'https://brave.github.io/bravebot/customize'

/** Every Workspace feature group off but the one a connector is for. */
const GMAIL_ONLY = 'docs.read:off,docs.write:off,drive.read:off,drive.write:off,calendar.read:off,calendar.write:off,chat.read:off,chat.write:off,gmail.write:off,gmail.downloadAttachment:off,people.read:off,slides.read:off,sheets.read:off,time.read:off'
const CALENDAR_ONLY = 'docs.read:off,docs.write:off,drive.read:off,drive.write:off,calendar.write:off,chat.read:off,chat.write:off,gmail.read:off,gmail.write:off,people.read:off,slides.read:off,sheets.read:off,time.read:off'

/** A stored secret: its value where one was typed, kept where it was left blank on a settings page. */
const secret = (name: string, key: string, values: Record<string, string>, kept: Set<string>): ConnectorVariable =>
  kept.has(key) ? { name, keep: true } : { name, value: values[key] ?? '' }

const workspace = (alias: string, features: string) =>
  (values: Record<string, string>): ConnectorForm => {
    const directory = (values.directory ?? '').replace(/\/+$/, '')
    return {
      alias,
      transport: 'stdio',
      command: ['node', `${directory}/dist/index.js`],
      variables: [
        { name: 'GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE', value: 'true' },
        { name: 'WORKSPACE_FEATURE_OVERRIDES', value: features },
        { name: 'BROWSER', value: 'www-browser' },
      ],
      directory,
    }
  }

export const CATALOG: CatalogEntry[] = [
  {
    alias: 'github',
    name: 'GitHub',
    icon: 'social-github',
    summary: 'Read issues and pull requests in the repositories your token names.',
    guide: `${DOCS}/mcp/github`,
    before: 'Download GitHub’s MCP server release into a directory of its own, and create a fine-grained token that can read issues and pull requests.',
    fields: [
      { key: 'program', label: 'Server program', kind: 'path', initial: '~/github-mcp-server/github-mcp-server', help: 'The github-mcp-server binary you downloaded.' },
      { key: 'token', label: 'Personal access token', kind: 'secret', help: 'Stored in ~/.bravebot/mcp.json and handed to this server alone.' },
      { key: 'toolsets', label: 'Toolsets', kind: 'text', initial: 'issues,pull_requests', help: 'Which of the server’s tool groups it offers.' },
    ],
    build: (values, kept) => ({
      alias: 'github',
      transport: 'stdio',
      command: [values.program ?? '', 'stdio', '--read-only', '--lockdown-mode', '--toolsets', values.toolsets ?? ''],
      variables: [secret('GITHUB_PERSONAL_ACCESS_TOKEN', 'token', values, kept)],
    }),
    values: (connector) => ({ program: connector.command?.[0] ?? '', toolsets: connector.command?.[5] ?? '' }),
  },
  {
    alias: 'gmail',
    name: 'Gmail',
    icon: 'email',
    summary: 'Read and search your mail, with read access and nothing else.',
    guide: `${DOCS}/mcp/gmail`,
    before: 'Download Google’s Workspace MCP server into a directory of its own, and sign in once with node dist/headless-login.js, asking for Gmail read access only.',
    fields: [
      { key: 'directory', label: 'Server directory', kind: 'path', initial: '~/google-workspace-mcp', help: 'Where you unpacked the server and signed in. It keeps its token there.' },
    ],
    build: workspace('gmail', GMAIL_ONLY),
    values: (connector) => ({ directory: connector.directory ?? '' }),
  },
  {
    alias: 'calendar',
    name: 'Google Calendar',
    icon: 'calendar',
    summary: 'Read your calendars and events, with read access and nothing else.',
    guide: `${DOCS}/mcp/calendar`,
    before: 'Download Google’s Workspace MCP server into a directory of its own, separate from Gmail’s, and sign in once asking for Calendar read access only.',
    fields: [
      { key: 'directory', label: 'Server directory', kind: 'path', initial: '~/google-workspace-calendar-mcp', help: 'Where you unpacked the server and signed in. It keeps its token there.' },
    ],
    build: workspace('calendar', CALENDAR_ONLY),
    values: (connector) => ({ directory: connector.directory ?? '' }),
  },
  {
    alias: 'brave-search',
    name: 'Brave Search',
    icon: 'brave-icon-search-color',
    summary: 'Search the web with the Brave Search API.',
    guide: `${DOCS}/mcp-servers`,
    before: 'Get a Brave Search API key, and save it in a file of its own. The server is fetched with npx each time it starts.',
    fields: [
      { key: 'keyFile', label: 'API key file', kind: 'path', initial: '~/keys/brave-api-key', help: 'The file holding your key. The server may read that one file.' },
    ],
    build: (values) => ({
      alias: 'brave-search',
      transport: 'stdio',
      command: ['npx', '-y', '@brave/brave-search-mcp-server'],
      variables: [{ name: 'BRAVE_API_KEY_FILE', value: values.keyFile ?? '' }],
    }),
    // The key file's path is a stored value, so it is not sent back; the read it was granted is.
    values: (connector) => ({ keyFile: connector.reads?.[0] ?? '' }),
  },
]

/** `~/x` with the home directory in place of `~`. */
export const expandHome = (value: string, home: string | null): string =>
  home && (value === '~' || value.startsWith('~/')) ? `${home.replace(/\/+$/, '')}${value.slice(1)}` : value

/** Whether a catalog entry claims an alias, so the list below it draws only the person's own. */
export const inCatalog = (alias: string): boolean => CATALOG.some((entry) => entry.alias === alias)

/** How a connector stands, as one word an indicator draws. */
export type Standing = 'connected' | 'off' | 'not-set-up' | 'attention'

export function standing(connector: Connector | undefined): Standing {
  if (!connector) return 'not-set-up'
  if (connector.problem || connector.refused || connector.changed) return 'attention'
  return connector.connected ? 'connected' : 'off'
}

export const STANDING_LABEL: Record<Standing, string> = {
  connected: 'Connected',
  off: 'Not connected',
  'not-set-up': 'Not set up',
  attention: 'Needs attention',
}

/**
 * A command's words as one line, each drawn as itself: a word holding a space, or an empty one, is
 * quoted, so `a b` reads apart from the two words `a` and `b` as it is apart in argv. The one
 * drawing every card and page uses, so a command reads the same wherever it is put to a person.
 */
export const drawCommand = (command: readonly string[] | null | undefined): string =>
  (command ?? []).map((word) => (/\s|^$/.test(word) ? JSON.stringify(word) : word)).join(' ')

/**
 * The words a person typed for a local command, split as a shell would split a simple line:
 * on spaces, with single or double quotes keeping a word whole. Nothing else is interpreted, and
 * the agent runs the words as argv, never as a line.
 */
export function splitCommand(line: string): string[] {
  const words: string[] = []
  let word = ''
  let quote: '"' | "'" | null = null
  let started = false
  for (const character of line) {
    if (quote) {
      if (character === quote) quote = null
      else word += character
    } else if (character === '"' || character === "'") {
      quote = character
      started = true
    } else if (/\s/.test(character)) {
      if (started || word) words.push(word)
      word = ''
      started = false
    } else {
      word += character
    }
  }
  if (started || word) words.push(word)
  return words
}

/**
 * The form a declared connector was made from, so it can be turned on again as it is. A stored
 * value is kept rather than sent, since the window never has it.
 */
export function formOf(connector: Connector): ConnectorForm {
  if (connector.transport === 'http') return { alias: connector.alias, transport: 'http', url: connector.url ?? '' }
  return {
    alias: connector.alias,
    transport: 'stdio',
    command: connector.command ?? [],
    // In the order the agent reports them, which keeps the declaration, and so its fingerprint, the same.
    variables: (connector.variables ?? []).map((variable) =>
      variable.stored ? { name: variable.name, keep: true } : { name: variable.name }),
    directory: connector.directory ?? undefined,
  }
}
