import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useEffect, useRef, useState } from 'react'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'

/**
 * What the bot has written about itself, in the raw. Read-only on purpose: it is the bot's own
 * file, and the transcript is where its changes are read — a box that could be typed over would
 * make this a second, quieter way to edit it that nothing records.
 */
const MEMORY = 'bot-memory m-0 overflow-auto bg-code font-mono text-muted-foreground'

/** The chosen mode is filled in rather than underlined, the way the rest of this column's is. */
const MEMORY_TAB =
  'data-active:bg-bubble-agent data-active:font-semibold dark:data-active:bg-bubble-agent'

export function BotMemory({ slug }: { slug: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [mode, setMode] = useState<'readable' | 'raw' | 'history'>('readable')
  const [history, setHistory] = useState<{ at: number; text: string; source: string }[]>([])
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(true)
  const [confirmReset, setConfirmReset] = useState(false)
  const reset = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (confirmReset) {
      reset.current?.scrollIntoView({ block: 'nearest' })
      reset.current?.querySelector('button')?.focus()
    }
  }, [confirmReset])
  useEffect(() => {
    let gone = false
    void Promise.all([window.bravebot.readBotMemory(slug), window.bravebot.readMemoryHistory(slug)]).then(([memory, revisions]) => {
      if (!gone) { setText(memory); setHistory(revisions) }
    }).catch(() => { if (!gone) setProblem('Memory could not be loaded.') }).finally(() => { if (!gone) setBusy(false) })
    return () => { gone = true }
  }, [slug])
  const save = async (value: string) => {
    setBusy(true); setProblem('')
    try {
      const saved = await window.bravebot.editBotMemory(slug, value, text)
      document.dispatchEvent(new CustomEvent('bravebot:memory-edited', { detail: slug }))
      setText(saved); setEditing(false); setConfirmReset(false)
      setHistory(await window.bravebot.readMemoryHistory(slug))
    } catch (error) { setProblem(String(error)) }
    finally { setBusy(false) }
  }
  return (
    <section className="bot-memory-panel mt-6 flex flex-col gap-3 border-t border-border pt-4">
      <h3 className="m-0 text-sm font-semibold">Persistent memory</h3>
      <p className="bot-note text-muted-foreground">Saved memory is included when the bot is briefed in a conversation. It is separate from the full message history. Memory saves independently of bot details. Edits are included with your next message to this bot.</p>
      <p className="bot-note text-muted-foreground">Up to 30 memory revisions are stored locally. Reset keeps revisions for recovery; deleting the bot removes this local history. Project memory files and conversations remain.</p>
      <Tabs value={mode} onValueChange={(value) => setMode(value as typeof mode)}>
        <TabsList className="memory-tabs my-2.5 gap-1.5">
          <TabsTrigger value="readable" className={MEMORY_TAB}>Read</TabsTrigger>
          <TabsTrigger value="raw" className={MEMORY_TAB}>Raw</TabsTrigger>
          <TabsTrigger value="history" className={MEMORY_TAB}>History</TabsTrigger>
        </TabsList>
        {problem && (
          <Alert variant="destructive" role="alert">
            <AlertDescription>{problem}</AlertDescription>
          </Alert>
        )}
        {busy && (
          <p role="status" className="flex items-center gap-2 text-muted-foreground">
            <Spinner />
            Loading…
          </p>
        )}
        {editing ? (
          <div className="flex flex-col gap-2">
            <Textarea autoFocus aria-label="Edit persistent memory" rows={8} className="p-3 font-mono text-[13px] leading-normal" value={draft} onChange={(event) => setDraft(event.target.value)} />
            <div className="memory-actions my-2.5 flex flex-wrap gap-1.5">
              <Button type="button" disabled={busy} onClick={() => void save(draft)}>Save memory</Button>
              <Button type="button" variant="outline" onClick={() => setEditing(false)}>Cancel edit</Button>
            </div>
          </div>
        ) : (
          <>
            <TabsContent value="history" className="memory-history flex flex-col gap-2">
              {history.length ? [...history].reverse().map((revision, index) => (
                <details key={`${revision.at}-${index}`} className="flex flex-col gap-2 border-b border-border py-2.5">
                  <summary className="cursor-pointer">{new Date(revision.at).toLocaleString()} · {revision.source === 'user' ? 'Your edit' : 'Bot update'}</summary>
                  <pre className="max-h-60 overflow-auto text-xs whitespace-pre-wrap">{revision.text || '(Empty memory)'}</pre>
                  <Button type="button" size="sm" variant="outline" onClick={() => { setDraft(revision.text); setEditing(true) }}>Review for restore</Button>
                </details>
              )) : <p>History begins with memory updates captured by this version.</p>}
            </TabsContent>
            <TabsContent value="raw">
              <pre className={cn(MEMORY, 'max-h-40 rounded-[7px] p-2 text-[11px] leading-normal whitespace-pre-wrap')}>{text || 'Nothing remembered yet.'}</pre>
            </TabsContent>
            {/* The same file, set as prose: roomier, and left to wrap where the raw view is left
                to break exactly where the bot broke it. */}
            <TabsContent
              value="readable"
              className={cn(
                MEMORY,
                'memory-readable max-h-[300px] rounded-md bg-bubble-agent p-3.5 text-sm leading-[1.65] whitespace-normal wrap-anywhere',
                '[&_p]:m-0 [&_p]:mb-3 [&_pre]:whitespace-pre-wrap [&_pre]:wrap-anywhere',
                '[&_:is(h1,h2,h3)]:m-0 [&_:is(h1,h2,h3)]:mb-3 [&_:is(h1,h2,h3)]:text-base [&_:is(h1,h2,h3)]:leading-[1.4] [&_:is(h1,h2,h3)]:font-semibold',
                '[&_:is(ul,ol)]:pl-[22px] [&_ul]:list-disc [&_ol]:list-decimal',
              )}
            >
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: ({ children }) => <span>{children}</span>, img: ({ alt }) => <span>{alt}</span> }}>{text || 'Nothing remembered yet.'}</ReactMarkdown>
            </TabsContent>
            <div className="memory-actions my-2.5 flex flex-wrap gap-1.5">
              <Button type="button" disabled={busy} onClick={() => { setDraft(text ?? ''); setEditing(true) }}>Edit memory</Button>
              <Button type="button" variant="outline" disabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset memory…</Button>
            </div>
          </>
        )}
      </Tabs>
      {confirmReset && (
        <div className="memory-reset flex flex-col gap-2 rounded-md border border-warning p-3" ref={reset}>
          <p>Reset this bot’s saved memory? The current version remains in History for restoration. Conversation messages are kept.</p>
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="destructive" disabled={busy} onClick={() => void save('')}>Reset saved memory</Button>
            <Button type="button" variant="outline" onClick={() => setConfirmReset(false)}>Keep memory</Button>
          </div>
        </div>
      )}
    </section>
  )
}
