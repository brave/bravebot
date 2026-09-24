import { useEffect, useRef, useState } from 'react'
import { BotAvatar } from './BotAvatar'
import { Modal } from './Modal'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'

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
    <Button variant="ghost" size="icon-sm" className="about-close" aria-label="Close About Brave Bot" onClick={onClose}>
      <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></svg>
    </Button>
    <DialogHeader className="about-hero">
      <div className="about-stage">
        <Button variant="ghost" className="about-mascot" aria-label="Make Brave Bot wink"
          onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
          onFocus={() => setFocused(true)} onBlur={() => setFocused(false)}
          onClick={wink}>
          <span className="about-figure">
            <BotAvatar seed="brave-bot-mascot" size={144} doing="waiting" expression={winking ? 'wink' : hovered || focused ? 'curious' : 'neutral'} />
          </span>
        </Button>
      </div>
      <DialogTitle>Brave Bot</DialogTitle>
      <Badge variant="outline" className="about-version self-center">Version {agentVersion}</Badge>
    </DialogHeader>
    <nav className="about-links" aria-label="Project resources">
      <Button asChild variant="link"><a href={project} target="_blank" rel="noreferrer">GitHub <span aria-hidden="true">↗</span></a></Button>
      <Button asChild variant="link"><a href={`${project}/releases`} target="_blank" rel="noreferrer">Release notes <span aria-hidden="true">↗</span></a></Button>
    </nav>
    <Accordion type="single" collapsible className="about-details">
      <AccordionItem value="build-storage" className="border-0">
        <AccordionTrigger>Build &amp; storage details</AccordionTrigger>
        <AccordionContent>
          <dl>
            <div><dt>Interface</dt><dd>{info.version}</dd></div>
            <div><dt>Agent</dt><dd>{info.build}</dd></div>
            <div><dt>Sessions</dt><dd>{info.home ?? 'Session folder unavailable'}</dd></div>
          </dl>
          <div className="about-copy">
            <Button variant="outline" size="sm" onClick={() => void copyBuildInfo()}>Copy build info</Button>
            <span role="status">{copyStatus}</span>
          </div>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
    <DialogFooter className="about-footer flex-row">
      <span>Built with <Button asChild variant="link"><a href={project} target="_blank" rel="noreferrer">bravebot</a></Button></span>
      <span aria-hidden="true">·</span>
      <Button asChild variant="link"><a href={`${project}/blob/main/LICENSE`} target="_blank" rel="noreferrer">MPL-2.0</a></Button>
    </DialogFooter>
  </Modal>
}
