/**
 * Placeholder rows the shape of the content that is loading, in place of a spinner.
 *
 * `rows` draws list rows of a name and a detail line, with a mark before them where `mark` is set;
 * `lines` draws a paragraph. It is mounted when loading starts and hidden by `data-delayed` until
 * loading has lasted 150ms (`useDelayedFlag`), so a quick answer draws nothing and nothing moves
 * when it shows. `label` is what a screen reader is told, and what a test finds the row by.
 */
export function Skeleton({ label, kind = 'rows', count = 3, mark = false, delayed = false, className, 'data-test': dataTest }: {
  label: string
  kind?: 'rows' | 'lines'
  count?: number
  mark?: boolean
  delayed?: boolean
  className?: string
  'data-test'?: string
}): React.JSX.Element {
  return <div className={`skeleton ${kind}${className ? ` ${className}` : ''}`} role="status" aria-busy="true"
    data-delayed={delayed || undefined} data-test={dataTest}>
    <span className="visually-hidden">{label}</span>
    {Array.from({ length: count }, (_, row) => kind === 'lines'
      ? <span className="skeleton-bar skeleton-line" key={row} aria-hidden="true" />
      : <div className="skeleton-row" key={row} aria-hidden="true">
        {mark && <span className="skeleton-bar skeleton-mark" />}
        <span className="skeleton-text"><span className="skeleton-bar skeleton-title" /><span className="skeleton-bar skeleton-detail" /></span>
      </div>)}
  </div>
}
