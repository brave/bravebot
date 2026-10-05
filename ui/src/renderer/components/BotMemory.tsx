import { useEffect, useRef, useState } from 'react'
import { useFitTextArea } from '../hooks'
import { Alert, Button, Collapse, ProgressRing, TextArea } from '../nala'
import { Modal } from './Modal'

type Revision = { at: number; text: string; source: string }

/**
 * The memory a bot keeps in one folder: the top of the text as it is, and a dialog that changes it.
 *
 * The text is shown raw rather than rendered, because it is the bot's own file and what is in it
 * is what the next conversation reads. The dialog holds the editor, earlier versions and reset.
 */
export function BotMemory({ slug, directory }: { slug: string; directory: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [history, setHistory] = useState<Revision[]>([])
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(true)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    let gone = false
    void Promise.all([window.bravebot.readBotMemory(slug, directory), window.bravebot.readMemoryHistory(slug, directory)]).then(([memory, revisions]) => {
      if (!gone) { setText(memory); setHistory(revisions) }
    }).catch(() => { if (!gone) setProblem('Memory could not be loaded.') }).finally(() => { if (!gone) setBusy(false) })
    return () => { gone = true }
  }, [slug, directory])
  const save = async (value: string): Promise<boolean> => {
    setBusy(true); setProblem('')
    try {
      const saved = await window.bravebot.editBotMemory(slug, directory, value, text)
      document.dispatchEvent(new CustomEvent('bravebot:memory-edited', { detail: slug }))
      setText(saved)
      setHistory(await window.bravebot.readMemoryHistory(slug, directory))
      return true
    } catch (error) {
      setProblem(String(error))
      return false
    } finally { setBusy(false) }
  }
  return <section className="bot-memory-panel" data-test="bot-memory" aria-labelledby={`memory-${slug}`}>
    <span className="bot-field-label" id={`memory-${slug}`}>Memory</span>
    {problem && !open && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    {busy && !open && <p role="status" className="memory-loading"><ProgressRing mode="indeterminate" /> Loading…</p>}
    <pre className="bot-memory"><span className="memory-excerpt">{text || 'Nothing remembered yet.'}</span></pre>
    <div className="memory-actions">
      <Button size="small" kind="outline" isDisabled={busy} onClick={() => setOpen(true)} data-test="memory-edit">Edit memory</Button>
    </div>
    {open && <MemoryDialog text={text} history={history} busy={busy} problem={problem} onSave={save} onClose={() => setOpen(false)} />}
  </section>
}

function MemoryDialog({ text, history, busy, problem, onSave, onClose }: {
  text: string | null
  history: Revision[]
  busy: boolean
  problem: string
  onSave: (value: string) => Promise<boolean>
  onClose: () => void
}): React.JSX.Element {
  const [draft, setDraft] = useState(text ?? '')
  const [showHistory, setShowHistory] = useState(false)
  const [confirmReset, setConfirmReset] = useState(false)
  const reset = useRef<HTMLElement>(null)
  const editor = useRef<HTMLElement>(null)
  useFitTextArea(editor, draft, 12, 20)
  useEffect(() => {
    if (confirmReset) {
      reset.current?.scrollIntoView({ block: 'nearest' })
      // Leo's button is a shadow host, so its inner <button> is out of reach of querySelector;
      // the host delegates focus to it.
      reset.current?.querySelector<HTMLElement>('leo-button')?.focus()
    }
  }, [confirmReset])
  const saveAndClose = async (value: string) => { if (await onSave(value)) onClose() }
  return <Modal title="Memory" size="lg" onClose={onClose} className="memory-dialog"
    subtitle="Everything here is carried into every conversation this bot has in this folder."
    actions={<>
      <div className="modal-leading memory-tools">
        <Button kind="plain-faint" aria-expanded={showHistory} onClick={() => setShowHistory(!showHistory)} data-test="memory-history">
          {showHistory ? 'Hide history' : 'History'}
        </Button>
        <Button kind="plain-faint" isDisabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset…</Button>
      </div>
      <Button kind="plain-faint" onClick={onClose}>Cancel</Button>
      <Button kind="filled" isDisabled={busy} onClick={() => void saveAndClose(draft)} data-test="memory-save">Save memory</Button>
    </>}>
    <div className="memory-body">
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    <TextArea ref={editor} autofocus aria-label="Edit memory" value={draft} minRows={12} maxRows={20} data-test="memory-editor"
      onInput={({ value }) => setDraft(value)}
      onChange={({ value }) => setDraft(value)} />
    {confirmReset && <Alert type="warning" size="small" className="memory-reset" ref={reset} hasActions>
      <span>Reset this bot’s saved memory? The current version stays in History. Conversations are kept.</span>
      <div slot="actions" className="memory-actions">
        <Button size="small" kind="filled" isDisabled={busy} onClick={() => void saveAndClose('')}>Reset saved memory</Button>
        <Button size="small" kind="plain-faint" onClick={() => setConfirmReset(false)}>Keep memory</Button>
      </div>
    </Alert>}
    {showHistory && <div className="memory-history">{history.length ? [...history].reverse().map((revision, index) => (
      <Collapse key={`${revision.at}-${index}`} className="flat-collapse memory-revision" isOpen={undefined}
        title={`${new Date(revision.at).toLocaleString()} · ${revision.source === 'user' ? 'Your edit' : 'Bot update'}`}>
        <pre>{revision.text || '(Empty memory)'}</pre>
        <Button size="small" kind="outline" onClick={() => { setDraft(revision.text); setShowHistory(false) }}>Review for restore</Button>
      </Collapse>
    )) : <p className="bot-note">History begins with the next change to this memory. Up to 30 versions are kept.</p>}</div>}
    </div>
  </Modal>
}
