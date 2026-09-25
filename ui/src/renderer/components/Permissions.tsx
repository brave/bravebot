import { useEffect, useState } from 'react'
import { DialogClose, Modal } from './Modal'
import { Alert, AlertDescription } from './ui/alert'
import { Button } from './ui/button'
import { DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
import { Empty, EmptyDescription, EmptyHeader } from './ui/empty'
import { Item, ItemActions, ItemContent } from './ui/item'
import { Spinner } from './ui/spinner'

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
  return <Modal title="Conversation permissions" onClose={onClose}>
    <DialogHeader>
      <DialogTitle>Conversation permissions</DialogTitle>
      <DialogDescription>These grants are saved with this conversation. Revocation affects future actions; it cannot remove content already read by the model.</DialogDescription>
    </DialogHeader>
    {problem && <Alert variant="destructive"><AlertDescription>{problem}</AlertDescription></Alert>}
    <Button variant="outline" disabled={busy} onClick={() => void request('permissions.list')}>
      {busy && <Spinner aria-hidden="true" />}{busy ? 'Loading…' : 'Refresh'}
    </Button>
    <h3>Trusted paths</h3>
    <p className="bot-note">A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.</p>
    {grants?.paths.filter((grant) => grant.integrity === 'trusted').map((grant) => <Item className="permission-row" key={grant.path}><ItemContent><code>{grant.path || 'Project root'}</code></ItemContent><ItemActions><Button variant="outline" size="sm" disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'path', path: grant.path })}>Revoke</Button></ItemActions></Item>)}
    {grants && !grants.paths.some((grant) => grant.integrity === 'trusted') && <Empty className="p-0 text-left md:p-0"><EmptyHeader className="items-start text-left"><EmptyDescription>No trusted path grants.</EmptyDescription></EmptyHeader></Empty>}
    {grants?.paths.some((grant) => grant.integrity !== 'trusted') && <><h3>Untrusted path exceptions</h3><p className="bot-note">These paths remain untrusted even when a parent is trusted.</p>{grants.paths.filter((grant) => grant.integrity !== 'trusted').map((grant) => <Item className="permission-row" key={grant.path}><ItemContent><code>{grant.path || 'Project root'}</code></ItemContent><ItemActions><span>Untrusted</span></ItemActions></Item>)}</>}
    <h3>Remembered commands</h3>
    <p className="bot-note">Each grant covers the resolved program and its exact arguments, including trust in its output.</p>
    {grants?.commands.map((command) => <Item className="permission-row" key={JSON.stringify(command)}><ItemContent><code>{command.display}</code></ItemContent><ItemActions><Button variant="outline" size="sm" disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, args: command.args } })}>Revoke</Button></ItemActions></Item>)}
    {grants?.commands.length === 0 && <Empty className="p-0 text-left md:p-0"><EmptyHeader className="items-start text-left"><EmptyDescription>No remembered command grants.</EmptyDescription></EmptyHeader></Empty>}
    <DialogFooter><DialogClose asChild><Button onClick={onClose}>Done</Button></DialogClose></DialogFooter>
  </Modal>
}
