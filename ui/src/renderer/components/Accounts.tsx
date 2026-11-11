import { useEffect, useRef, useState } from 'react'
import { accountLogo, accountProviderKey, isKeyWay, isSignInWay, parseAccounts, type AccountRow, type KeyWay, type SignInWay } from '../../shared/accounts'
import type { ModelCatalogue } from '../../shared/protocol'
import { t } from '../i18n'
import { Alert, Button, Icon, Label, type IconName } from '../nala'
import { useSignIn, type Waiting } from '../use-sign-in'
import { sameKeyTarget, useKeyEntry, type KeyResult } from '../use-key-entry'
import { CopyButton } from './CopyButton'
import { Modal } from './Modal'
import { ModelWarnings } from './ModelWarnings'
import { ProviderModels } from './ProviderModels'
import { SIGN_IN_NAMES, SignInWaiting, signInSentence } from './SignInButton'

/** Reading the rows again when the window comes back is held off this long after the last read. */
export const FOCUS_REFRESH_MS = 5000

function standingText(row: AccountRow): string {
  if (row.standing === 'notSignedIn') return t.uiAccountsNotSignedIn()
  if (row.standing === 'unusable') return t.uiAccountsUnusable()
  if (row.standing === 'misconfigured') return t.uiAccountsMisconfigured()
  if (row.way !== 'brave' && row.way !== 'gateway') return t.uiAccountsSignedIn()
  if (row.keyFrom === 'environment') return t.uiAccountsKeyFromEnvironment()
  if (row.keyFrom === 'none') return t.uiAccountsNoKeyNeeded()
  return t.uiAccountsKeyStored()
}

const STANDING_COLOR = { signedIn: 'green', notSignedIn: 'neutral', unusable: 'yellow', misconfigured: 'red' } as const

/** The ink a standing is said in where it is a line under the name rather than a label. */
const STANDING_INK = { signedIn: 'success', notSignedIn: 'tertiary', unusable: 'warning', misconfigured: 'error' } as const

const rowKey = (row: AccountRow) => `${row.way}:${row.id ?? ''}`

/** The row's key window: open for it, and how to ask for one. */
export interface RowKeyEntry {
  open: boolean
  busy: boolean
  onEnter: (way: KeyWay, id: string | null) => void
  onShow: () => void
}

/** One sign-in: its standing, why it can't be used, and what can be done about it from here. */
export function AccountRowView({ row, waiting, busy, signingOut, onSignIn, onSignOut, onShow, onCancel, keyEntry, logo = true }: {
  row: AccountRow
  /**
   * Draw the account's mark and its standing as a label beside the name. A detail pane beside a list
   * of marks leaves the mark out and says the standing as a line under the name, as the list does.
   */
  logo?: boolean
  waiting: Waiting | null
  busy: boolean
  signingOut: boolean
  onSignIn: (way: SignInWay, id: string | null) => void
  onSignOut: () => void
  onShow: () => void
  onCancel: () => void
  keyEntry?: RowKeyEntry
}): React.JSX.Element {
  // An AWS account is signed in to only where the bridge gave its command, which it does where a
  // sign-in is the remedy: not for a missing AWS CLI or a profile it does not have.
  const way = isSignInWay(row.way) && (row.standing === 'notSignedIn' || row.standing === 'unusable')
    && (row.way !== 'bedrock' || row.command !== null) ? row.way : null
  const keyWay = keyEntry && row.canEnterKey && isKeyWay(row.way) ? row.way : null
  const keyOpen = keyWay !== null && keyEntry?.open === true
  const command = !way && !keyWay && row.standing !== 'signedIn' ? row.command : null
  const actions = (way && !waiting) || keyWay || row.canSignOut
  return <li className={`account-row${logo ? '' : ' no-logo'}`} data-test={`account-${rowKey(row)}`}>
    {logo && <Icon className="account-logo" name={accountLogo(row.way, row.id)} aria-hidden="true" data-test="account-logo" />}
    <div className={`account-head${logo ? '' : ' stacked'}`}>
      <span className="account-name">{row.name}</span>
      {logo
        ? <Label color={STANDING_COLOR[row.standing]} mode="outline" data-test="account-standing">{standingText(row)}</Label>
        : <span className={`account-standing-line ${STANDING_INK[row.standing]}`} data-test="account-standing">{standingText(row)}</span>}
    </div>
    {(row.detail || waiting || keyOpen || command) && <div className="account-body">
      {row.detail && (row.standing === 'unusable' || row.standing === 'misconfigured')
        ? <Alert type="warning" size="small" data-test="account-detail">{row.detail}</Alert>
        : row.detail && <span className="settings-caption" data-test="account-detail">{row.detail}</span>}
      {waiting && <SignInWaiting waiting={waiting} onShow={onShow} onCancel={onCancel} />}
      {keyOpen && <span className="settings-caption" role="status" data-test="account-key-open">{t.uiAccountsKeyWindowOpen()}</span>}
      {command && <span className="account-command" data-test="account-command">
        <span>{t.uiAccountsWithCli()}</span> <span className="account-command-text"><code>{command}</code>
          <CopyButton text={() => command} label={t.uiAccountsCopyCommand()} size="small" data-test="account-copy" /></span>
      </span>}
    </div>}
    {actions && <div className="account-actions">
      {way && !waiting && <Button size="small" kind="filled" isDisabled={busy} onClick={() => onSignIn(way, row.id)} data-test="account-sign-in">{t.uiAccountsSignIn()}</Button>}
      {keyWay && (keyOpen
        ? <Button size="small" kind="outline" onClick={keyEntry?.onShow} data-test="account-key-show">{t.uiAccountsShowKeyWindow()}</Button>
        : <Button size="small" kind={row.canSignOut ? 'outline' : 'filled'} isDisabled={keyEntry?.busy} onClick={() => keyEntry?.onEnter(keyWay, row.id)} data-test="account-enter-key">
          {row.canSignOut ? t.uiAccountsReplaceKey() : t.uiAccountsEnterKey()}</Button>)}
      {row.canSignOut && <Button size="small" kind="outline" isDisabled={signingOut || waiting !== null || keyOpen} onClick={onSignOut} data-test="account-sign-out">{t.uiAccountsSignOut()}</Button>}
    </div>}
  </li>
}

/** The question asked before a sign-in is forgotten. Escape and Cancel forget nothing. */
export function SignOutConfirm({ row, busy, onConfirm, onCancel }: { row: AccountRow; busy: boolean; onConfirm: () => void; onCancel: () => void }): React.JSX.Element {
  return <Modal title={t.uiAccountsSignOutTitle({ name: row.name })} size="sm" compact className="confirm-sign-out" onClose={onCancel}
    actions={<>
      <Button kind="plain-faint" isDisabled={busy} onClick={onCancel} data-test="sign-out-cancel">{t.uiAccountsCancel()}</Button>
      <Button kind="filled" isDisabled={busy} onClick={onConfirm} data-test="sign-out-confirm">{t.uiAccountsSignOut()}</Button>
    </>}>
    <p>{t.uiAccountsSignOutBody()}</p>
    {row.way === 'gateway' && row.host && <p data-test="sign-out-host">{t.uiAccountsSignOutGatewayBody({ host: row.host })}</p>}
  </Modal>
}

type Said = { type: 'success' | 'error'; text: string } | null

/** What a key window came to, as the row's name says it, with the bridge's sentence where it gave one. */
export function KeyResultAlert({ result, name }: { result: KeyResult; name: string }): React.JSX.Element {
  const stored = result.outcome === 'stored'
  const first = stored ? t.uiAccountsKeyStoredFor({ name }) : t.uiAccountsKeyNotStoredFor({ name })
  // A key stored with a sentence is one stored but not yet sent, which is a warning.
  const type = !stored ? 'error' : result.sentence ? 'warning' : 'success'
  return <Alert type={type} size="small" role={type === 'success' ? 'status' : 'alert'} data-test="accounts-key-result">
    {result.sentence ? `${first} ${result.sentence}` : first}
  </Alert>
}

/**
 * The sign-ins the agent can use, as `bravebot auth status` reports them, with sign-out; sign-in for
 * the ChatGPT and xAI accounts and each AWS account; and, for Brave Premium and a gateway, a key
 * entered in the main process's key window. One of the two windows is open at a time. No row shows
 * who is signed in, a key or a token.
 *
 * The rows are a list on the left and the one chosen is drawn beside it, with what can be done about
 * it and, once it is signed in, its models: whether they appear in the model picker, which of them
 * do, and in what order (`ProviderModels`).
 *
 * `refresh` reads the rows again when it changes. The window coming back reads them too, at most
 * once in `FOCUS_REFRESH_MS`, and not where an AWS Bedrock row is listed: reading that row runs
 * the AWS command-line tool. `pick` chooses the row of that service, for a link elsewhere on the page
 * that points at it. A row is named by its service key (`accountProviderKey`), since two gateways
 * may share a display name.
 */
export function Accounts({ session, refresh, onChanged, pick }: {
  session?: string
  refresh: number
  onChanged: () => void
  pick?: { key: string; asked: number } | null
}): React.JSX.Element {
  const [rows, setRows] = useState<AccountRow[] | null>(null)
  const [chosen, setChosen] = useState<string | null>(null)
  const [catalogue, setCatalogue] = useState<ModelCatalogue | null>(null)
  const [catalogueProblem, setCatalogueProblem] = useState<string | null>(null)
  const [listed, setListed] = useState(0)
  const list = useRef<HTMLUListElement>(null)
  const [unread, setUnread] = useState(false)
  const [said, setSaid] = useState<Said>(null)
  const [leaving, setLeaving] = useState<AccountRow | null>(null)
  const [signingOut, setSigningOut] = useState(false)
  const lastRead = useRef(0)
  const picked = useRef(0)
  const changed = useRef(onChanged)
  changed.current = onChanged

  const read = async () => {
    lastRead.current = Date.now()
    try {
      const result = await window.bravebot.request('accounts.list', { session })
      const listed = result.ok ? parseAccounts(result.ok) : null
      setUnread(listed === null)
      if (listed) setRows(listed)
    } catch { setUnread(true) }
  }
  const signIn = useSignIn(() => { void read(); setListed((n) => n + 1); changed.current() }, session)
  const keys = useKeyEntry(() => { void read(); setListed((n) => n + 1); changed.current() }, session)
  useEffect(() => { void read() }, [session, refresh])
  // The models each signed-in row offers, read once for the page and again after a sign-in, a key
  // or a sign-out changes what the services offer.
  useEffect(() => {
    let gone = false
    setCatalogue(null)
    setCatalogueProblem(null)
    void window.bravebot.request<ModelCatalogue>('models.list', { session }).then((answer) => {
      if (gone) return
      if (answer.ok) setCatalogue(answer.ok)
      else setCatalogueProblem(answer.error?.message ?? 'Could not load models. Try again.')
    }).catch(() => { if (!gone) setCatalogueProblem('Could not load models. Try again.') })
    return () => { gone = true }
  }, [session, refresh, listed])
  // The first signed-in row is chosen when nothing is, and a row that went away gives its place up.
  useEffect(() => {
    if (!rows?.length) return
    if (chosen && rows.some((row) => rowKey(row) === chosen)) return
    setChosen(rowKey(rows.find((row) => row.standing === 'signedIn') ?? rows[0]!))
  }, [rows, chosen])
  useEffect(() => {
    if (!pick || !rows || picked.current === pick.asked) return
    const row = rows.find((each) => accountProviderKey(each) === pick.key)
    if (!row) return
    // Each ask applies once. A later read of the rows must not take the selection back from a row
    // the person has chosen since.
    picked.current = pick.asked
    setChosen(rowKey(row))
    list.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(rowKey(row))}"]`)?.focus()
  }, [pick, rows])
  const rowsNow = useRef(rows)
  rowsNow.current = rows
  useEffect(() => window.bravebot.onWindowActive((active) => {
    if (!active || Date.now() - lastRead.current < FOCUS_REFRESH_MS) return
    if (rowsNow.current?.some((row) => row.way === 'bedrock')) return
    void read()
  }), [session])

  const signOut = async (row: AccountRow) => {
    setSigningOut(true)
    try {
      const result = await window.bravebot.request('accounts.signOut', { session, way: row.way, id: row.id })
      const listed = result.ok ? parseAccounts(result.ok) : null
      if (result.error || !listed) {
        setSaid({ type: 'error', text: result.error?.message ?? t.uiAccountsSignOutFailed({ name: row.name }) })
        void read()
      } else {
        setRows(listed)
        setSaid({ type: 'success', text: t.uiAccountsSignedOut({ name: row.name }) })
        setListed((n) => n + 1)
        changed.current()
      }
    } catch (error) {
      setSaid({ type: 'error', text: String(error) })
    } finally {
      setSigningOut(false)
      setLeaving(null)
    }
  }

  const ended = signIn.result
  const keyed = keys.result
  const keyedName = keyed ? rows?.find((row) => sameKeyTarget(keyed, row.way, row.id))?.name
    ?? (keyed.way === 'brave' ? 'Brave Premium' : keyed.id ?? keyed.way) : ''
  const endedName = ended?.way ? rows?.find((row) => row.way === ended.way && (ended.way !== 'bedrock' || row.id === ended.id))?.name
    ?? SIGN_IN_NAMES[ended.way] : ''
  const current = rows?.find((row) => rowKey(row) === chosen) ?? null
  const currentKey = current ? accountProviderKey(current) : null
  const move = (by: number) => {
    if (!rows?.length) return
    const at = Math.max(0, rows.findIndex((row) => rowKey(row) === chosen))
    const next = rows[Math.min(rows.length - 1, Math.max(0, at + by))]
    if (!next) return
    setChosen(rowKey(next))
    list.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(rowKey(next))}"]`)?.focus()
  }
  return <section className="settings-block accounts" data-test="accounts">
    <p>{t.uiAccountsEnvironmentNote()}</p>
    {said && <Alert type={said.type} size="small" role={said.type === 'error' ? 'alert' : 'status'} data-test="accounts-said">{said.text}</Alert>}
    {ended && <Alert type={ended.outcome === 'signedIn' ? 'success' : ended.outcome === 'failed' ? 'error' : 'info'} size="small"
      role={ended.outcome === 'failed' ? 'alert' : 'status'} data-test="accounts-sign-in-result">{signInSentence(ended, endedName)}</Alert>}
    {keyed && <KeyResultAlert result={keyed} name={keyedName} />}
    {unread && <Alert type="warning" size="small" role="alert" data-test="accounts-unread">{t.uiAccountsUnread()}</Alert>}
    {catalogue && catalogue.warnings.length > 0 && <div className="providers-warnings" data-test="providers-warnings">
      <ModelWarnings warnings={catalogue.warnings} commands={catalogue.warningCommands} session={session}
        onSignedIn={() => { void read(); setListed((n) => n + 1); changed.current() }} />
      <Button size="small" kind="outline" onClick={() => setListed((n) => n + 1)} data-test="providers-warnings-retry">Try again</Button>
    </div>}
    {rows && <div className="providers-panes">
      <ul ref={list} className="providers-list" role="listbox" aria-label={t.uiAccountsHeading()} data-test="providers-list"
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); move(event.key === 'ArrowDown' ? 1 : -1) }
          else if (event.key === 'Home' || event.key === 'End') { event.preventDefault(); move(event.key === 'Home' ? -rows.length : rows.length) }
        }}>
        {rows.map((row) => {
          const key = rowKey(row)
          const selected = key === chosen
          return <li key={key} role="option" aria-selected={selected} tabIndex={selected ? 0 : -1} data-key={key}
            className="providers-item" data-test={`provider-${key}`} onClick={() => setChosen(key)}>
            <Icon className="providers-item-logo" name={accountLogo(row.way, row.id) as IconName} aria-hidden="true" />
            <span className="providers-item-text">
              <span className="providers-item-name">{row.name}</span>
              <span className={`account-standing-line ${STANDING_INK[row.standing]}`}>{standingText(row)}</span>
            </span>
          </li>
        })}
      </ul>
      <div className="providers-detail" data-test="provider-detail">
        {current && <ul className="account-list">
          <AccountRowView key={rowKey(current)} row={current} logo={false}
            waiting={signIn.waiting?.way === current.way && (current.way !== 'bedrock' || signIn.waiting.id === current.id) ? signIn.waiting : null}
            busy={signIn.busy || keys.busy} signingOut={signingOut} onShow={signIn.show} onCancel={signIn.cancel}
            onSignIn={(way, id) => { setSaid(null); keys.dismiss(); void signIn.start(way, way === 'bedrock' ? id : null) }}
            onSignOut={() => { setSaid(null); signIn.dismiss(); keys.dismiss(); setLeaving(current) }}
            keyEntry={{
              open: sameKeyTarget(keys.open, current.way, current.id), busy: keys.busy || signIn.busy, onShow: keys.show,
              onEnter: (way, id) => { setSaid(null); signIn.dismiss(); void keys.start(way, id) },
            }} />
        </ul>}
        {current && currentKey && current.standing === 'signedIn' && <ProviderModels key={currentKey} providerKey={currentKey} name={current.name}
          models={catalogue ? catalogue.models.filter((model) => model.providerKey === currentKey) : null}
          problem={catalogueProblem} onRetry={() => setListed((n) => n + 1)} />}
      </div>
    </div>}
    {leaving && <SignOutConfirm row={leaving} busy={signingOut} onConfirm={() => void signOut(leaving)} onCancel={() => { if (!signingOut) setLeaving(null) }} />}
  </section>
}
