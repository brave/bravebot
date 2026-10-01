import { Alert, Icon } from '../nala'
import { dismissToast, useToasts } from '../toasts'
import { IconButton } from './IconButton'

/** Confirmations for exports and copies, stacked in the corner of the conversation card. */
export function Toasts(): React.JSX.Element {
  const toasts = useToasts()
  return (
    <div className="status-toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <Alert key={toast.id} type="success" size="small" isToast className="status-toast" data-test="status-toast">
          <Icon name="check-circle-filled" slot="icon" />
          <span slot="title">{toast.title}</span>
          {toast.body && <span className="status-toast-body">{toast.body}</span>}
          <IconButton slot="content-after" icon="close" label="Dismiss" size="tiny" onClick={() => dismissToast(toast.id)} />
        </Alert>
      ))}
    </div>
  )
}
