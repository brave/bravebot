/**
 * The layout every small window of the main process's own draws: the sign-in window and the key
 * window. A head with the service's mark and the window's title, a body, and the actions at the
 * foot with the primary last. The page says how tall it is drawn so the window fits it.
 */

import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Button, Icon } from './nala'

export function SmallWindowPage({ logo, logoTest, title, test, fit, actions, children }: {
  /** A mark from `accountLogo` or `SIGN_IN_LOGOS`. */
  logo: string
  logoTest: string
  title: string
  test: string
  /** Told the page's drawn height whenever it changes. */
  fit?: (height: number) => void
  actions: ReactNode
  children: ReactNode
}): React.JSX.Element {
  const drawn = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const page = drawn.current
    if (!page || !fit) return
    const observer = new ResizeObserver(() => fit(page.getBoundingClientRect().height))
    observer.observe(page)
    return () => observer.disconnect()
  }, [fit])
  return <main className="small-window" data-test={test}>
    <div ref={drawn} className="small-window-page">
      <header className="small-window-head">
        <Icon name={logo} aria-hidden="true" data-test={logoTest} />
        <h1>{title}</h1>
      </header>
      <div className="small-window-body">{children}</div>
      <footer className="small-window-actions">{actions}</footer>
    </div>
  </main>
}

/**
 * One value a person can copy, on a field of its own. `copy` asks the main process to copy what
 * it recorded; the page hands it nothing.
 */
export function CopyField({ kind, test, copyTest, label, copy, children }: {
  kind: 'code' | 'url'
  test: string
  copyTest: string
  label: string
  copy: () => Promise<boolean> | undefined
  children: ReactNode
}): React.JSX.Element {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), 1500)
    return () => clearTimeout(timer)
  }, [copied])
  return <div className="small-window-field">
    <code className={`small-window-${kind}`} data-test={test}>{children}</code>
    <Button size="small" kind="plain" data-test={copyTest} aria-label={label} onClick={() => void copy()?.then(setCopied)}>
      <Icon name={copied ? 'check-normal' : 'copy'} slot="icon-before" />
      {/* Both words hold the space, so the button keeps its width when the word changes. */}
      <span className="small-window-copy-label" data-copied={copied}><span>Copy</span><span>Copied</span></span>
    </Button>
  </div>
}
