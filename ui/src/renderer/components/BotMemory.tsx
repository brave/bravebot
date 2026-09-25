import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useEffect, useState } from 'react'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion'
import { Alert, AlertDescription } from './ui/alert'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from './ui/alert-dialog'
import { Button } from './ui/button'
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from './ui/card'
import { Empty, EmptyDescription } from './ui/empty'
import { Field, FieldLabel } from './ui/field'
import { Spinner } from './ui/spinner'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs'
import { Textarea } from './ui/textarea'

export function BotMemory({ slug }: { slug: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [mode, setMode] = useState<'readable' | 'raw' | 'history'>('readable')
  const [history, setHistory] = useState<{ at: number; text: string; source: string }[]>([])
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(true)
  const [confirmReset, setConfirmReset] = useState(false)
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
  return <Card className="bot-memory-panel" role="region" aria-labelledby="persistent-memory-title">
    <CardHeader>
      <CardTitle><h3 id="persistent-memory-title">Persistent memory</h3></CardTitle>
      <CardDescription>
        <p className="bot-note">Saved memory is included when the bot is briefed in a conversation. It is separate from the full message history. Memory saves independently of bot details. Edits are included with your next message to this bot.</p>
        <p className="bot-note">Up to 30 memory revisions are stored locally. Reset keeps revisions for recovery; deleting the bot removes this local history. Project memory files and conversations remain.</p>
      </CardDescription>
    </CardHeader>
    <CardContent>
      <Tabs value={mode} onValueChange={(value) => setMode(value as typeof mode)}>
        <TabsList className="memory-tabs" aria-label="Memory view">
          <TabsTrigger type="button" value="readable">Read</TabsTrigger>
          <TabsTrigger type="button" value="raw">Raw</TabsTrigger>
          <TabsTrigger type="button" value="history">History</TabsTrigger>
        </TabsList>
        {problem && <Alert variant="destructive"><AlertDescription>{problem}</AlertDescription></Alert>}
        {busy && <div role="status"><Spinner aria-hidden="true" /> Loading…</div>}
        {editing ? <Field>
          <FieldLabel htmlFor="bot-memory-editor" className="sr-only">Edit persistent memory</FieldLabel>
          <Textarea id="bot-memory-editor" autoFocus aria-label="Edit persistent memory" rows={8} value={draft} onChange={(event) => setDraft(event.target.value)} />
          <div className="memory-actions mt-2 flex flex-wrap gap-2"><Button type="button" disabled={busy} onClick={() => void save(draft)}>Save memory</Button><Button variant="outline" type="button" onClick={() => setEditing(false)}>Cancel edit</Button></div>
        </Field> : <>
          <TabsContent value="readable"><div className="memory-readable rounded-lg border border-border p-3"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: ({ children }) => <span>{children}</span>, img: ({ alt }) => <span>{alt}</span> }}>{text || 'Nothing remembered yet.'}</ReactMarkdown></div></TabsContent>
          <TabsContent value="raw"><pre className="bot-memory">{text || 'Nothing remembered yet.'}</pre></TabsContent>
          <TabsContent value="history">
            {history.length ? <Accordion type="single" collapsible className="memory-history">{[...history].reverse().map((revision, index) => <AccordionItem value={`${revision.at}-${index}`} key={`${revision.at}-${index}`}>
              <AccordionTrigger type="button">{new Date(revision.at).toLocaleString()} · {revision.source === 'user' ? 'Your edit' : 'Bot update'}</AccordionTrigger>
              <AccordionContent><pre>{revision.text || "(Empty memory)"}</pre><Button variant="outline" size="sm" type="button" onClick={() => { setDraft(revision.text); setEditing(true) }}>Review for restore</Button></AccordionContent>
            </AccordionItem>)}</Accordion> : <Empty className="memory-history"><EmptyDescription>History begins with memory updates captured by this version.</EmptyDescription></Empty>}
          </TabsContent>
        </>}
      </Tabs>
      <AlertDialog open={confirmReset} onOpenChange={setConfirmReset}>
        <AlertDialogContent className="memory-reset">
          <AlertDialogHeader>
            <AlertDialogTitle>Reset saved memory?</AlertDialogTitle>
            <AlertDialogDescription>The current version remains in History for restoration. Conversation messages are kept.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Keep memory</AlertDialogCancel>
            <AlertDialogAction variant="destructive" disabled={busy} onClick={() => void save('')}>Reset saved memory</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </CardContent>
    {!editing && <CardFooter className="memory-actions flex flex-wrap gap-2"><Button type="button" disabled={busy} onClick={() => { setDraft(text ?? ''); setEditing(true) }}>Edit memory</Button><Button variant="outline" type="button" disabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset memory…</Button></CardFooter>}
  </Card>
}
