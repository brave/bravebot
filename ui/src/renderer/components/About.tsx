import { useEffect, useRef, useState } from 'react'
import { BotAvatar } from './BotAvatar'
import { Modal } from './Modal'

export interface AboutInfo {
  version: string
  build: string
  home: string | null
}

const project = 'https://github.com/brave/bravebot'

export function About({ info, onClose }: { info: AboutInfo; onClose: () => void }): React.JSX.Element {
  // The pinned agent stamps its build as "version (commit)"; info.version is the bridge version.
  const agentVersion = info.build.split(' ')[0]
  const [winking, setWinking] = useState(false)
  const winkTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [hovered, setHovered] = useState(false)
  const [focused, setFocused] = useState(false)
  const [copyStatus, setCopyStatus] = useState('')

  useEffect(() => () => {
    if (winkTimer.current !== null) clearTimeout(winkTimer.current)
  }, [])

  function wink() {
    // Let an active wink finish even if the mascot is clicked repeatedly.
    if (winkTimer.current !== null) return
    setWinking(true)
    winkTimer.current = setTimeout(() => {
      setWinking(false)
      winkTimer.current = null
    }, 320)
  }

  async function copyBuildInfo() {
    try {
      await navigator.clipboard.writeText(`Brave Bot\nInterface: ${info.version}\nAgent: ${info.build}`)
      setCopyStatus('Build info copied')
    } catch {
      setCopyStatus('Could not copy build info. Please select and copy the details below.')
    }
  }

  return <Modal title="About Brave Bot" onClose={onClose} className="about">
    <button className="about-close" aria-label="Close About Brave Bot" onClick={onClose}>
      <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></svg>
    </button>
    <div className="about-hero">
      <div className="about-stage">
        <button className="about-mascot" aria-label="Make Brave Bot wink"
          onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
          onFocus={() => setFocused(true)} onBlur={() => setFocused(false)}
          onClick={wink}>
          <span className="about-figure">
            <BotAvatar seed="brave-bot-mascot" size={144} doing="waiting" expression={winking ? 'wink' : hovered || focused ? 'curious' : 'neutral'} />
          </span>
        </button>
      </div>
      <h2>Brave Bot</h2>
      <span className="about-version">Version {agentVersion}</span>
    </div>
    <nav className="about-links" aria-label="Project resources">
      <a href={project} target="_blank" rel="noreferrer">GitHub <span aria-hidden="true">↗</span></a>
      <a href={`${project}/releases`} target="_blank" rel="noreferrer">Release notes <span aria-hidden="true">↗</span></a>
    </nav>
    <details className="about-details">
      <summary tabIndex={0}>Build &amp; storage details</summary>
      <dl>
        <div><dt>Interface</dt><dd>{info.version}</dd></div>
        <div><dt>Agent</dt><dd>{info.build}</dd></div>
        <div><dt>Sessions</dt><dd>{info.home ?? 'Session folder unavailable'}</dd></div>
      </dl>
      <div className="about-copy">
        <button onClick={() => void copyBuildInfo()}>Copy build info</button>
        <span role="status">{copyStatus}</span>
      </div>
    </details>
    <footer className="about-footer">
      <span>Built with <a href={project} target="_blank" rel="noreferrer">bravebot</a></span>
      <span aria-hidden="true">·</span>
      <a href={`${project}/blob/main/LICENSE`} target="_blank" rel="noreferrer">MPL-2.0</a>
    </footer>
  </Modal>
}
