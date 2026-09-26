import { useEffect, useState } from 'react'
import { XIcon } from 'lucide-react'
import { cn, SCRIM } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Empty, EmptyDescription } from '@/components/ui/empty'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'

interface Watch { number: number; path: string; remainingSeconds: number; armedBy: number; state: string }
interface Listing { watches: Watch[]; busy: boolean }

export function Watches({ session, onClose }: { session: string; onClose: () => void }): React.JSX.Element {
  const [listing, setListing] = useState<Listing | null>(null)
  const [path, setPath] = useState('')
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  const request = async (method = 'watches.list', params: Record<string, unknown> = {}) => {
    if (method !== 'watches.list') { setBusy(true); setProblem('') }
    try {
      const response = await window.bravebot.request<Listing>(method, { session, ...params })
      if (response.error) throw new Error(response.error.message)
      if (response.ok) setListing(response.ok)
      if (method === 'watches.add') { setPath(''); setStatus('Watch added. File changes can now start a model turn.') }
      if (method === 'watches.stop') setStatus('Watch stopped. Any turn already running can be stopped in the conversation.')
    } catch (error) { setProblem(String(error)) } finally { if (method !== 'watches.list') setBusy(false) }
  }
  useEffect(() => { void request(); const timer = setInterval(() => void request(), 2000); return () => clearInterval(timer) }, [session])
  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      <DialogContent
        className="modal watch-settings max-h-[calc(100dvh-40px)] w-[min(760px,calc(100vw-32px))] overflow-auto sm:max-w-none [&_p]:leading-normal"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogHeader className="settings-heading flex-row items-center justify-between gap-5">
          <DialogTitle>File watches</DialogTitle>
          <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close file watches">
            <XIcon />
          </Button>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <p>A file change starts a turn in this conversation and may use model credits. Normal read and approval rules still apply.</p>
          <p>Up to eight watches, for seven days each. They run while this conversation is open in the app. Closing it ends the watches.</p>
          {problem && (
            <Alert variant="destructive" role="alert">
              <AlertDescription>{problem}</AlertDescription>
            </Alert>
          )}
          {status && <p role="status">{status}</p>}
          {!listing && !problem && <p role="status">Loading watches…</p>}
          {listing?.watches.length === 0 && (
            <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
              <EmptyDescription className="text-left text-inherit">
                No files watched. Add one below, or ask the agent to watch a file.
              </EmptyDescription>
            </Empty>
          )}
          {/* A rule between rows rather than a gap: the list is paths and their states, and a
              path that has wrapped needs a line saying where the next one starts. */}
          <ul className="watch-list list-none p-0">
            {listing?.watches.map((w) => (
              <li key={w.number} className="flex items-center justify-between gap-4 border-b border-border py-3.5">
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <strong className="wrap-anywhere">{w.path}</strong>
                  <p className="my-1.5">
                    {w.state === 'running'
                      ? 'Automatic turn running'
                      : listing.busy
                        ? 'Waiting for this turn to finish'
                        : 'Watching'}
                    {' '}· Expires in {Math.max(1, Math.ceil(w.remainingSeconds / 3600))} hours
                  </p>
                  <small className="text-muted-foreground">{w.armedBy ? `Armed by turn ${w.armedBy}` : 'Added by you'}</small>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void request('watches.stop', { number: w.number })}
                  aria-label={`Stop watching ${w.path}`}
                >
                  Stop
                </Button>
              </li>
            ))}
          </ul>
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void request('watches.add', { path: path.trim() })
            }}
          >
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="watch-path">Project file</FieldLabel>
                <Input
                  id="watch-path"
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                  placeholder="src/example.ts"
                />
              </Field>
            </FieldGroup>
            <Button
              type="submit"
              disabled={busy || !path.trim() || !listing || listing.busy || listing.watches.length >= 8}
            >
              Watch file
            </Button>
          </form>
          {listing?.busy && <p>Wait for the current turn to finish before adding a watch.</p>}
        </div>
        <DialogFooter className={cn('settings-actions mx-0 mt-3.5 mb-1 flex-row flex-wrap justify-end gap-2 rounded-none border-0 bg-transparent p-0')}>
          <Button
            variant="outline"
            disabled={busy || !listing?.watches.length}
            onClick={() => void request('watches.stop', { all: true })}
          >
            Stop all watches
          </Button>
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
