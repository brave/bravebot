import { useEffect, useState } from 'react'
import { SCRIM } from '@/lib/utils'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Empty, EmptyDescription } from '@/components/ui/empty'

interface Grants {
  paths: { path: string; integrity: string }[]
  commands: { program: string; args: string[]; display: string }[]
}

export function Permissions({ session, onClose }: { session: string; onClose: () => void }): React.JSX.Element {
  const [grants, setGrants] = useState<Grants | null>(null)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const request = async (method: string, params: Record<string, unknown> = {}) => {
    setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<Grants>(method, { session, ...params })
      if (answer.error) setProblem(answer.error.message)
      else if (answer.ok) setGrants(answer.ok)
    } catch { setProblem('Permissions could not be loaded. Try again.') }
    finally { setBusy(false) }
  }
  useEffect(() => { void request('permissions.list') }, [session])
  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      <DialogContent className="modal max-h-[calc(100vh-64px)] w-[min(680px,calc(100vw-48px))] overflow-y-auto p-7 sm:max-w-none" showCloseButton={false} overlayClassName={SCRIM}>
        <DialogHeader>
          <DialogTitle>Conversation permissions</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          <p>These grants are saved with this conversation. Revocation affects future actions; it cannot remove content already read by the model.</p>
          {problem && (
            <Alert variant="destructive" role="alert">
              <AlertDescription>{problem}</AlertDescription>
            </Alert>
          )}
          <PathPermissions paths={grants?.paths ?? []} busy={busy} onRevoke={(path) => void request('permissions.revoke', { kind: 'path', path })} />
          <div className="flex flex-col gap-2">
            <h3>Remembered commands</h3>
            <p className="bot-note text-muted-foreground">
              Each grant covers the resolved program and its exact arguments, including trust in its output.
            </p>
            {grants?.commands.map((command) => (
              <div className="permission-row flex items-center justify-between gap-3 border-b border-border py-2.5" key={JSON.stringify(command)}>
                <code className="min-w-0 text-xs wrap-anywhere">{command.display}</code>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, args: command.args } })}
                >
                  Revoke
                </Button>
              </div>
            ))}
            {grants?.commands.length === 0 && (
              <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
                <EmptyDescription className="text-left text-inherit">No remembered command grants.</EmptyDescription>
              </Empty>
            )}
          </div>
        </div>
        <DialogFooter className="mx-0 mb-0 rounded-none border-0 bg-transparent p-0 sm:justify-end">
          <Button variant="outline" size="sm" disabled={busy} onClick={() => void request('permissions.list')}>
            {busy ? 'Loading…' : 'Refresh'}
          </Button>
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export function PathPermissions({ paths, busy, onRevoke }: { paths: Grants['paths']; busy: boolean; onRevoke: (path: string) => void }): React.JSX.Element {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <h3>Trusted paths</h3>
        <p className="bot-note text-muted-foreground">
          A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.
        </p>
        {paths.filter((grant) => grant.integrity === 'trusted').map((grant) => (
          <div className="permission-row flex items-center justify-between gap-3 border-b border-border py-2.5" key={grant.path}>
            <code className="min-w-0 text-xs wrap-anywhere">{grant.path || 'Project root'}</code>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => onRevoke(grant.path)}>Revoke</Button>
          </div>
        ))}
        {!paths.some((grant) => grant.integrity === 'trusted') && (
          <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
            <EmptyDescription className="text-left text-inherit">No trusted path grants.</EmptyDescription>
          </Empty>
        )}
      </div>
      {paths.some((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided') && (
        <div className="flex flex-col gap-2">
          <h3>Untrusted path exceptions</h3>
          <p className="bot-note text-muted-foreground">These paths remain untrusted even when a parent is trusted.</p>
          {paths.filter((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided').map((grant) => (
            <div className="permission-row flex items-center justify-between gap-3 border-b border-border py-2.5" key={grant.path}>
              <code className="min-w-0 text-xs wrap-anywhere">{grant.path || 'Project root'}</code>
              <Badge variant="outline">Untrusted</Badge>
            </div>
          ))}
        </div>
      )}
      {paths.some((grant) => grant.integrity === 'undecided') && (
        <div className="flex flex-col gap-2">
          <h3>Paths awaiting a decision</h3>
          <p className="bot-note text-muted-foreground">
            These paths require write approval and their contents remain untrusted until you grant trust.
          </p>
          {paths.filter((grant) => grant.integrity === 'undecided').map((grant) => (
            <div className="permission-row flex items-center justify-between gap-3 border-b border-border py-2.5" key={grant.path}>
              <code className="min-w-0 text-xs wrap-anywhere">{grant.path || 'Project root'}</code>
              <Badge variant="secondary">Not decided</Badge>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
