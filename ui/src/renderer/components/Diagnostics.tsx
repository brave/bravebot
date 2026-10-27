import { useEffect, useRef, useState } from 'react'
import { Modal } from './Modal'
import { Alert, Button, Dropdown, Icon, Input, Label } from '../nala'
import { IconButton } from './IconButton'
import { CopyButton } from './CopyButton'

type Family = 'Read' | 'Edit' | 'Bash' | 'WebFetch' | 'Mcp'
interface Checked { decision: 'deny' | 'ask' | 'allow' | null; rule?: string; file?: string | null; grantedFrom?: string | null; reason?: string | null }
interface Span { role: string; label: string; kind: string; text: string }
interface LastRequest { model: string; tools: string[]; toolsBytes: number; spans: Span[] }

const PLACEHOLDER: Record<Family, string> = { Read: 'src/lib.rs', Edit: 'src/lib.rs', Bash: 'git push origin main', WebFetch: 'https://example.com/docs', Mcp: 'server:tool' }
const DECISION: Record<'deny' | 'ask' | 'allow', [string, 'red' | 'yellow' | 'green']> = {
  deny: ['Refused', 'red'], ask: ['Always asked', 'yellow'], allow: ['Not asked', 'green'],
}

/** Debugging views for one conversation: which rule decides a call, and the request last sent to the model. */
export function Diagnostics({ session, onClose }: { session: string; onClose: () => void }): React.JSX.Element {
  return <Modal title="Diagnostics" size="lg" onClose={onClose}
    subtitle="Check a permission rule and read the request this conversation last sent to the model."
    actions={<Button kind="filled" onClick={onClose} data-test="diagnostics-done">Done</Button>}>
    <PermissionCheck session={session} />
    <LastRequestView session={session} />
  </Modal>
}

function PermissionCheck({ session }: { session: string }): React.JSX.Element {
  const [family, setFamily] = useState<Family>('Bash')
  const [specifier, setSpecifier] = useState('')
  const [checked, setChecked] = useState<Checked | null>(null)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const asked = useRef(latestOnly()).current
  useEffect(() => () => asked.drop(), [])
  const check = async () => {
    const current = asked.begin()
    setBusy(true); setProblem(''); setChecked(null)
    try {
      const answer = await window.bravebot.request<Checked>('permissions.check', { session, family, specifier: specifier.trim() })
      if (!current()) return
      if (answer.error) setProblem(answer.error.message)
      else if (answer.ok) setChecked(answer.ok)
    } catch { if (current()) setProblem('The check could not be run. Try again.') }
    finally { if (current()) setBusy(false) }
  }
  const edited = () => { asked.drop(); setChecked(null); setBusy(false) }
  const ruled = checked?.decision ? DECISION[checked.decision] : undefined
  return <section className="grant-section" data-test="diagnostics-check">
    <h3>Permission check</h3>
    <p className="grant-note">Which rule from your settings files decides a call. The call is only matched against the rules this conversation opened under; nothing is read, run or fetched.</p>
    <form className="diagnostics-check" onSubmit={(event) => { event.preventDefault(); if (specifier.trim()) void check() }}>
      <Dropdown value={family} size="small" positionStrategy="fixed" data-test="diagnostics-family"
        onChange={(detail) => { setFamily(String(detail.value) as Family); edited() }}>
        <span slot="label">Kind</span>
        <leo-option value="Read">Read</leo-option>
        <leo-option value="Edit">Edit</leo-option>
        <leo-option value="Bash">Bash</leo-option>
        <leo-option value="WebFetch">WebFetch</leo-option>
        <leo-option value="Mcp">Mcp</leo-option>
      </Dropdown>
      <Input value={specifier} size="small" placeholder={PLACEHOLDER[family]} data-test="diagnostics-specifier"
        onInput={(event) => {
          const detail = event as { value?: unknown; target?: EventTarget | null }
          const value = typeof detail.value === 'string' ? detail.value
            : detail.target && typeof detail.target === 'object' && 'value' in detail.target && typeof detail.target.value === 'string' ? detail.target.value
              : null
          if (value !== null) { setSpecifier(value); edited() }
        }}>Call</Input>
      <Button size="small" kind="outline" isDisabled={busy || !specifier.trim()} onClick={() => void check()} data-test="diagnostics-run-check">Check</Button>
    </form>
    {problem && <Alert type="error" size="small" data-test="diagnostics-check-error">{problem}</Alert>}
    {checked && !ruled && <p className="grant-empty" data-test="diagnostics-check-none"><Icon name="settings" />No rule matches, so the ordinary questions decide.</p>}
    {checked && ruled && <ul className="grant-list" data-test="diagnostics-check-result"><li className="grant-row">
      <Label color={ruled[1]} mode="outline">{ruled[0]}</Label>
      <div className="grant-text">
        <code className="grant-path">{checked.rule}</code>
        {checked.file && <span className="grant-meta">{`From ${checked.file}`}</span>}
        {!checked.file && checked.grantedFrom && <span className="grant-meta">{`Granted from ${checked.grantedFrom}`}</span>}
        {!checked.file && !checked.grantedFrom && <span className="grant-meta" data-test="diagnostics-check-moved">No settings file holds this rule now. It was read when this conversation opened, and still decides its calls.</span>}
        {checked.reason && <span className="grant-meta">{`Reason: ${checked.reason}`}</span>}
      </div>
    </li></ul>}
  </section>
}

/** About ten lines of the dialog's width, so a system prompt doesn't push the rest out of view. */
const COLLAPSE_AT = 600

/**
 * The request the last turn built for the model, as the agent read it out of the request itself.
 *
 * Plain text only, each span under the label saying where its words came from. Content held under
 * a reference is not in the request and appears here as its token.
 */
function LastRequestView({ session }: { session: string }): React.JSX.Element {
  const [last, setLast] = useState<LastRequest | null | undefined>(undefined)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const asked = useRef(latestOnly()).current
  const load = async () => {
    const current = asked.begin()
    setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<LastRequest | null>('request.last', { session })
      if (!current()) return
      if (answer.error) setProblem(answer.error.message)
      else setLast(answer.ok ?? null)
    } catch { if (current()) setProblem('The last request could not be loaded. Try again.') }
    finally { if (current()) setBusy(false) }
  }
  useEffect(() => { void load(); return () => asked.drop() }, [session])
  return <section className="grant-section" data-test="diagnostics-request">
    <div className="diagnostics-head">
      <h3>Last request</h3>
      <IconButton icon="refresh" label="Refresh" tooltip={busy ? 'Loading…' : 'Refresh the last request'} disabled={busy}
        onClick={() => void load()} data-test="diagnostics-request-refresh" />
    </div>
    <p className="grant-note">What the model was sent, labelled with where each part came from. Content the model was not shown appears as its reference.</p>
    {problem && <Alert type="error" size="small" data-test="diagnostics-request-error">{problem}</Alert>}
    {last === null && <p className="grant-empty" data-test="diagnostics-request-none"><Icon name="message-bubble-text" />No request yet. Send a message to see what the model is sent.</p>}
    {last && <>
      <p className="grant-meta diagnostics-summary">{`${last.model} · ${last.spans.length} parts · ${last.tools.length} tools (${last.toolsBytes.toLocaleString()} bytes)`}</p>
      <ol className="diagnostics-spans">{last.spans.map((span, index) => <RequestSpan key={index} span={span} />)}</ol>
    </>}
  </section>
}

/** One part of the request, as plain text under its label. */
export function RequestSpan({ span }: { span: Span }): React.JSX.Element {
  // The blank lines that separate parts in the request would open each block with empty rows; Copy keeps them.
  const text = span.text.replace(/^\n+|\n+$/g, '')
  const shown = collapsed(text)
  const long = shown !== text
  const [open, setOpen] = useState(false)
  return <li className="diagnostics-span" data-kind={span.kind}>
    <div className="diagnostics-span-head">
      <span className="grant-meta">{span.role}</span>
      <Label color="neutral" mode="outline" data-test="diagnostics-span-label">{span.label}</Label>
      <CopyButton text={() => span.text} label="Copy this part" className="diagnostics-copy" />
    </div>
    <pre className="diagnostics-span-text">{long && !open ? shown : text}</pre>
    {long && <Button size="small" kind="plain-faint" className="diagnostics-toggle" onClick={() => setOpen((value) => !value)}>{open ? 'Show less' : 'Show all'}</Button>}
  </li>
}

/** `text` cut to its first {@link COLLAPSE_AT} characters, counted by code point so no pair is split. */
export function collapsed(text: string): string {
  const points = Array.from(text)
  return points.length > COLLAPSE_AT ? `${points.slice(0, COLLAPSE_AT).join('')}…` : text
}

/**
 * Hands out one question at a time: `begin` returns whether that question is still the latest, so
 * an answer that comes back after another question, an edit or the dialog closing is dropped.
 */
export function latestOnly(): { begin: () => () => boolean; drop: () => void } {
  let latest = 0
  return {
    begin: () => { const asked = ++latest; return () => asked === latest },
    drop: () => { latest += 1 },
  }
}
