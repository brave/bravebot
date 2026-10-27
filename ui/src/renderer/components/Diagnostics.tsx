import { useEffect, useState } from 'react'
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
  const check = async () => {
    setBusy(true); setProblem(''); setChecked(null)
    try {
      const answer = await window.bravebot.request<Checked>('permissions.check', { session, family, specifier: specifier.trim() })
      if (answer.error) setProblem(answer.error.message)
      else if (answer.ok) setChecked(answer.ok)
    } catch { setProblem('The check could not be run. Try again.') }
    finally { setBusy(false) }
  }
  const ruled = checked?.decision ? DECISION[checked.decision] : undefined
  return <section className="grant-section" data-test="diagnostics-check">
    <h3>Permission check</h3>
    <p className="grant-note">Which rule from your settings files decides a call. The call is only matched against the rules this conversation opened under; nothing is read, run or fetched.</p>
    <form className="diagnostics-check" onSubmit={(event) => { event.preventDefault(); if (specifier.trim()) void check() }}>
      <Dropdown value={family} size="small" data-test="diagnostics-family"
        onChange={(detail) => { setFamily(String(detail.value) as Family); setChecked(null) }}>
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
          if (value !== null) { setSpecifier(value); setChecked(null) }
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
        {checked.reason && <span className="grant-meta">{`Reason: ${checked.reason}`}</span>}
      </div>
    </li></ul>}
  </section>
}

/** Spans longer than this are collapsed until opened. */
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
  const load = async () => {
    setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<LastRequest | null>('request.last', { session })
      if (answer.error) setProblem(answer.error.message)
      else setLast(answer.ok ?? null)
    } catch { setProblem('The last request could not be loaded. Try again.') }
    finally { setBusy(false) }
  }
  useEffect(() => { void load() }, [session])
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
  const long = span.text.length > COLLAPSE_AT
  const [open, setOpen] = useState(false)
  return <li className="diagnostics-span" data-kind={span.kind}>
    <div className="diagnostics-span-head">
      <span className="grant-meta">{span.role}</span>
      <Label color="neutral" mode="outline" data-test="diagnostics-span-label">{span.label}</Label>
      <CopyButton text={() => span.text} label="Copy this part" className="diagnostics-copy" />
    </div>
    <pre className="diagnostics-span-text">{long && !open ? `${span.text.slice(0, COLLAPSE_AT)}…` : span.text}</pre>
    {long && <Button size="small" kind="plain-faint" className="diagnostics-toggle" onClick={() => setOpen((value) => !value)}>{open ? 'Show less' : 'Show all'}</Button>}
  </li>
}
