/**
 * The sign-ins the agent can use, as `accounts.list` reports them, and the sign-in the window can
 * start for the ChatGPT and xAI accounts and an AWS account Bedrock uses (CHATGPT-15, XAI-20,
 * BACKEND-70), and the key the window can take for Brave Premium and a gateway in a key window its
 * main process owns (PREM-16, CRED-24).
 *
 * No row carries an account's identity, a key or a token, or any part of one (CHATGPT-11, XAI-9).
 */

export type AccountWay = 'brave' | 'bedrock' | 'gateway' | 'chatgpt' | 'xai'
export type Standing = 'signedIn' | 'notSignedIn' | 'unusable' | 'misconfigured'
export type KeyFrom = 'stored' | 'environment' | 'none'

export interface AccountRow {
  way: AccountWay
  id: string | null
  name: string
  standing: Standing
  keyFrom: KeyFrom | null
  /** The bridge's sentence for an unusable or misconfigured row. */
  detail: string | null
  /** The command for the bravebot command-line tool that signs this row in, drawn as code. */
  command: string | null
  canSignOut: boolean
  /** Where a gateway's key works, for the sign-out confirmation. */
  host: string | null
  /** Whether the window's key window can take a key for this row. */
  canEnterKey: boolean
}

/** The ways the window signs in itself, and the only words `bravebot:signIn` accepts. */
export type SignInWay = 'chatgpt' | 'xai' | 'bedrock'

/**
 * Each account's mark in Leo's icon set. ChatGPT has none of its own and is shown by OpenAI's, and
 * AWS has none and is shown by Amazon's.
 */
export const SIGN_IN_LOGOS = { chatgpt: 'openai', xai: 'xai', bedrock: 'amazon-color' } as const satisfies Record<SignInWay, string>

/** The gateways with a mark of their own in Leo's icon set; any other shows a key. */
const GATEWAY_LOGOS: Record<string, string> = { openrouter: 'openrouter', openai: 'openai', anthropic: 'anthropic-color' }

/** A row's mark in Leo's icon set. */
export function accountLogo(way: AccountWay, id: string | null): string {
  if (isSignInWay(way)) return SIGN_IN_LOGOS[way]
  if (way === 'brave') return 'brave-icon-release-color'
  return (way === 'gateway' && id && GATEWAY_LOGOS[id]) || 'key'
}

/** The ways the window takes a key for, and the only words `bravebot:enterKey` accepts. */
export type KeyWay = 'brave' | 'gateway'

export const isKeyWay = (way: unknown): way is KeyWay => way === 'brave' || way === 'gateway'

/**
 * Everything the main-owned key window shows. Never the key: the window's page reads it from its
 * own field and hands it to the main process, which hands it to the bridge.
 */
export interface KeyWindowState {
  way: KeyWay
  id: string | null
  name: string
  /** Where a gateway's key works. */
  host: string | null
  /** A key is stored already, and saving replaces it. */
  replace: boolean
  /** The variable a session sends instead of the stored key, by name. */
  variable: string | null
  /** Saving also records the block that sends a known service its key. */
  records: boolean
}

/** What saving in the key window came to, for that window. */
export type KeySaveAnswer = { saved: true } | { refused: string }

/** What the page is told of a key window. Never the key. */
export type KeyNotice =
  | { kind: 'opened'; way: KeyWay; id: string | null }
  | { kind: 'ended'; way: KeyWay; id: string | null; outcome: 'stored' | 'cancelled'; sentence: string | null }

/** What an ask to enter a key came to. `null` is an ask dropped, which says nothing. */
export type KeyAnswer = { opened: true } | { error: string } | null

export type Attempt = string | number

export type SignInOutcome = 'signedIn' | 'cancelled' | 'failed'

/**
 * What the page is told of a sign-in. Never the URL, the address or the code (CHATGPT-16, XAI-21):
 * those are shown only in the window the main process owns.
 */
export type SignInNotice =
  | { kind: 'started'; attempt: Attempt; way: SignInWay; id: string | null; port: number | null; expiresIn: number | null }
  | { kind: 'ended'; attempt: Attempt; way: SignInWay | null; id: string | null; outcome: SignInOutcome; sentence: string | null }

/** The attempt running, for a page that reloaded while it waits. */
export interface SignInCurrent {
  attempt: Attempt
  way: SignInWay
  /** The row's id: an AWS account's profile, `null` for the default chain and the other ways. */
  id: string | null
  started: boolean
  port: number | null
  expiresIn: number | null
}

/** What an ask to sign in came to. `null` is an ask dropped or answered no, which says nothing. */
export type SignInAnswer = { attempt: Attempt } | { error: string } | null

/** Everything the main-owned sign-in window shows. Never sent to the main window's page. */
export interface SignInWindowState {
  way: SignInWay
  id: string | null
  url: string | null
  port: number | null
  code: string | null
  address: string | null
  expiresIn: number | null
}

const WAYS: readonly string[] = ['brave', 'bedrock', 'gateway', 'chatgpt', 'xai']
const STANDINGS: readonly string[] = ['signedIn', 'notSignedIn', 'unusable', 'misconfigured']
const KEY_FROM: readonly string[] = ['stored', 'environment', 'none']

export const isSignInWay = (way: unknown): way is SignInWay => way === 'chatgpt' || way === 'xai' || way === 'bedrock'
export const isAttempt = (value: unknown): value is Attempt =>
  (typeof value === 'string' && value.length > 0) || (typeof value === 'number' && Number.isFinite(value))

const textOrNull = (value: unknown): value is string | null => value === null || typeof value === 'string'

function row(value: unknown): AccountRow | null {
  if (typeof value !== 'object' || value === null) return null
  const v = value as Record<string, unknown>
  if (typeof v.way !== 'string' || !WAYS.includes(v.way)) return null
  if (typeof v.standing !== 'string' || !STANDINGS.includes(v.standing)) return null
  if (v.keyFrom !== null && (typeof v.keyFrom !== 'string' || !KEY_FROM.includes(v.keyFrom))) return null
  if (typeof v.name !== 'string' || typeof v.canSignOut !== 'boolean') return null
  if (!textOrNull(v.id) || !textOrNull(v.detail) || !textOrNull(v.command) || !textOrNull(v.host)) return null
  return {
    way: v.way as AccountWay, id: v.id, name: v.name, standing: v.standing as Standing, keyFrom: v.keyFrom as KeyFrom | null,
    detail: v.detail, command: v.command, canSignOut: v.canSignOut, host: v.host,
    // A bridge from before the key window says nothing, which is no.
    canEnterKey: v.canEnterKey === true,
  }
}

/**
 * The rows of an `accounts.list` or `accounts.signOut` answer, or `null` where it is not one. A
 * bridge without the method, or a fixture that answers `{}`, is told apart from an empty list.
 */
export function parseAccounts(value: unknown): AccountRow[] | null {
  const accounts = (value as { accounts?: unknown } | null)?.accounts
  if (!Array.isArray(accounts)) return null
  const rows = accounts.map(row)
  return rows.every((r): r is AccountRow => r !== null) ? rows : null
}

/**
 * The way a command sent beside a failure or a picker warning signs in, where it is one the
 * window can start itself. The command is a field the bridge sent as a command, not words read
 * out of a sentence.
 */
export function signInWayOf(command: string | null | undefined): SignInWay | null {
  const match = /^bravebot auth login (chatgpt|xai)$/.exec(command?.trim() ?? '')
  return match ? (match[1] as SignInWay) : null
}
