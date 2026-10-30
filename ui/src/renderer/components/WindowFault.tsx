import { Component, type ErrorInfo, type ReactNode } from 'react'
import { detailsOf, reportFault } from '../fault'
import { Alert, Button, Collapse } from '../nala'
import { CopyButton } from './CopyButton'

/**
 * The window when the page itself failed (DIAG-10).
 *
 * An error React meets while drawing otherwise unmounts the whole page and leaves a black window.
 * This says what happened, offers the one thing that helps, and shows the technical details to
 * copy into a bug report. The details are the error's own message and stack, drawn as text.
 */
export function WindowFault({ details, onReload }: { details: string; onReload: () => void }): React.JSX.Element {
  return (
    <main className="window-fault" data-test="window-fault">
      <Alert type="error" className="error-card window-fault-card" role="alert" hasActions>
        <span slot="title">This window hit an error</span>
        <p>Reload to continue, or copy the details for a bug report.</p>
        <div slot="actions" className="error-actions">
          <Button size="small" kind="filled" onClick={onReload} data-test="window-fault-reload">Reload</Button>
          <CopyButton text={() => details} label="Copy details" size="small" data-test="window-fault-copy" />
        </div>
        <Collapse title="Technical details" isOpen={undefined} data-test="window-fault-details">
          <pre>{details}</pre>
        </Collapse>
      </Alert>
    </main>
  )
}

interface State {
  failed: boolean
  error: unknown
  componentStack: string | null
}

/**
 * Catches an error thrown while the page draws or updates, including the browser refusing to
 * remove a node React expected to find, which React meets in the commit.
 *
 * Reporting it sends the main process the name of the error only (see `reportFault`).
 */
export class FaultBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { failed: false, error: null, componentStack: null }

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { failed: true, error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    reportFault('render', error)
    this.setState({ componentStack: info.componentStack ?? null })
  }

  render(): ReactNode {
    if (!this.state.failed) return this.props.children
    return <WindowFault details={detailsOf(this.state.error, this.state.componentStack)} onReload={() => window.bravebot.reloadWindow()} />
  }
}
