import { Alert, Button, Icon } from '../nala'
import { dismissToast, useToasts } from '../toasts'

/** Confirmations for exports and copies, stacked in the corner of the conversation card. */
export function Toasts(): React.JSX.Element {
  const toasts = useToasts()
  return (
    <div className="status-toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <Alert key={toast.id} type={toast.kind === 'note' ? 'info' : 'success'} size="small" isToast className="status-toast" data-test="status-toast">
          <Icon name={toast.kind === 'note' ? 'info-outline' : 'check-circle-filled'} slot="icon" />
          <span slot="title">{toast.title}</span>
          {toast.body && <span className="status-toast-body">{toast.body}</span>}
          <Button slot="content-after" kind="plain-faint" fab size="tiny" aria-label="Dismiss" onClick={() => dismissToast(toast.id)}>
            <Icon name="close" />
          </Button>
        </Alert>
      ))}
    </div>
  )
}
