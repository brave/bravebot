import { useEffect, useRef, useState } from 'react'
import { BotAvatar } from './BotAvatar'
import { cn, SCRIM } from '@/lib/utils'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'

/** Undecorated until it is wanted: a card with five underlined links on it is a table of them. */
const LINK =
  'text-primary no-underline underline-offset-[3px] hover:underline focus-visible:rounded-[3px] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary'

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

  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      <DialogContent
        // A small stage for the app's own mascot, so the card arrives rather than appears. The
        // arrival is named on `data-open` rather than unconditionally, because that is the
        // spelling that displaces the shared dialog's own zoom — a bare `animate-[…]` loses to it
        // on specificity and never runs. Reduced motion is answered in globals.css, beside the
        // keyframes, since a media query cannot be a utility. The rhythm below the mascot is in
        // the margins rather than in a gap on the grid, which is why there is none.
        className="modal about max-h-[calc(100vh-64px)] w-[min(440px,calc(100vw-32px))] gap-0 overflow-y-auto px-7 pt-[30px] pb-[22px] data-open:animate-[about-arrive_220ms_ease-out] sm:max-w-none"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogTitle className="sr-only">About Brave Bot</DialogTitle>
        <button
          className="about-close absolute top-3 right-3 z-[1] grid size-8 place-items-center rounded-full border border-transparent bg-transparent p-0 text-muted-foreground hover:bg-code hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
          aria-label="Close About Brave Bot"
          onClick={onClose}
        >
          <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></svg>
        </button>
        <div className="about-hero text-center">
          {/* The glow is the stage, not the figure: a pool of the accent behind the mascot so it
              stands on something instead of floating at the top of an empty card. */}
          <div className="about-stage relative grid justify-items-center pt-2.5 pb-[22px] before:pointer-events-none before:absolute before:top-[-4px] before:h-45 before:w-55 before:content-[''] before:[background:radial-gradient(ellipse,color-mix(in_srgb,var(--primary)_19%,transparent),transparent_70%)]">
            <button className="about-mascot relative grid size-[156px] cursor-pointer place-items-center rounded-full border-0 bg-transparent p-0 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary" aria-label="Make Brave Bot wink"
              onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
              onFocus={() => setFocused(true)} onBlur={() => setFocused(false)}
              onClick={wink}>
              {/* Deliberately untransformed: a drive script reads this element's computed
                  transform to prove the figure itself is never scaled or nudged. */}
              <span className="about-figure block">
                <BotAvatar seed="brave-bot-mascot" size={144} doing="waiting" expression={winking ? 'wink' : hovered || focused ? 'curious' : 'neutral'} />
              </span>
            </button>
          </div>
          <h2 className="mt-3.5 mb-2 text-[32px] leading-[1.15] font-[650] tracking-[-1.1px]">Brave Bot</h2>
          <span className="about-version mt-[18px] inline-block max-w-full rounded-[20px] border border-border bg-code px-[11px] py-[5px] text-xs text-muted-foreground wrap-anywhere">Version {agentVersion}</span>
        </div>
        <nav className="about-links mt-6 mb-7 flex flex-wrap justify-center gap-6 text-[13px]" aria-label="Project resources">
          <a className={LINK} href={project} target="_blank" rel="noreferrer">GitHub <span className="ml-0.5 opacity-65" aria-hidden="true">↗</span></a>
          <a className={LINK} href={`${project}/releases`} target="_blank" rel="noreferrer">Release notes <span className="ml-0.5 opacity-65" aria-hidden="true">↗</span></a>
        </nav>
        <details className="about-details border-y border-border text-xs">
          <summary className="cursor-pointer py-[15px] text-muted-foreground hover:text-foreground focus-visible:rounded-[3px] focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary" tabIndex={0}>Build &amp; storage details</summary>
          {/* Selectable, because the whole point of these three lines is pasting them into a
              bug report. */}
          <dl className="m-0 mb-4 [&>div]:my-2.5 [&>div]:grid [&>div]:grid-cols-[65px_minmax(0,1fr)] [&>div]:gap-3 [&_dd]:m-0 [&_dd]:font-mono [&_dd]:text-[11px] [&_dd]:leading-[1.6] [&_dd]:wrap-anywhere [&_dd]:select-text [&_dt]:text-muted-foreground">
            <div><dt>Interface</dt><dd>{info.version}</dd></div>
            <div><dt>Agent</dt><dd>{info.build}</dd></div>
            <div><dt>Sessions</dt><dd>{info.home ?? 'Session folder unavailable'}</dd></div>
          </dl>
          <div className="about-copy flex flex-wrap items-center gap-2.5 pb-4">
            <button
              className="min-h-8 rounded-full border border-border bg-background px-3 py-1.5 text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              onClick={() => void copyBuildInfo()}
            >
              Copy build info
            </button>
            <span className="text-[11px] text-muted-foreground" role="status">{copyStatus}</span>
          </div>
        </details>
        <footer className="about-footer mt-5 flex flex-wrap justify-center gap-2 text-[11px] text-muted-foreground">
          <span>Built with <a className={cn(LINK, 'text-inherit')} href={project} target="_blank" rel="noreferrer">bravebot</a></span>
          <span aria-hidden="true">·</span>
          <a className={cn(LINK, 'text-inherit')} href={`${project}/blob/main/LICENSE`} target="_blank" rel="noreferrer">MPL-2.0</a>
        </footer>
      </DialogContent>
    </Dialog>
  )
}
