import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import { BODY_NAMES, COLOURS, choiceOf, pickedSeed, type Body, type Colour } from '../avatar/pixels'
import { Button } from '../nala'
import { BotAvatar, BotFace, PixelSwatch, type Doing } from './BotAvatar'
import { IconButton } from './IconButton'

const BODY_LABELS: Record<Body, string> = {
  dome: 'Dome', squat: 'Squat', block: 'Block', round: 'Round', tall: 'Tall', diamond: 'Diamond', blob: 'Blob', bust: 'Bust',
}

const capital = (word: string) => word[0]!.toUpperCase() + word.slice(1)
const colourLabel = ({ paints: [top, bottom] }: Colour) => `${capital(top)} to ${bottom}`

/** How far the popover stays from the face and from the window's edges, in CSS pixels. */
const GAP = 8
/**
 * A bot's face with the button that edits it: a popover of the eight shapes and the colours, and
 * Randomize. Every choice is saved as it is made, so a click outside or Escape only closes the
 * popover.
 *
 * The popover is a manual `popover`, so it is drawn in the top layer: the inspector's scrolling
 * body does not clip it, and inside the create dialog it is still part of the modal dialog's
 * subtree, which keeps it from being inert.
 */
export function AvatarEditor({ seed, size, doing, className, onCommit }: {
  seed: string
  size: number
  doing: Doing
  className: string
  /** Called with the new seed each time a choice changes the face. */
  onCommit: (seed: string) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(seed)
  const frame = useRef<HTMLDivElement>(null)
  const panel = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLElement>(null)
  const panelId = useId()
  const titleId = useId()

  const close = () => setOpen(false)
  const closeRef = useRef(close)
  closeRef.current = close

  // The last face this editor saved. A face that changes to any other one comes from elsewhere
  // (another bot opened in the same panel) and ends the edit.
  const saved = useRef(seed)
  useEffect(() => { if (seed !== saved.current) setOpen(false) }, [seed])

  const choice = choiceOf(open ? draft : seed)
  const choose = (body: Body, colour: string) => {
    const next = pickedSeed(body, colour)
    if (next === (open ? draft : seed)) return
    saved.current = next
    setDraft(next)
    onCommit(next)
  }
  const randomize = () => {
    let body = choice.body
    let colour = choice.colour
    while (body === choice.body && colour === choice.colour) {
      body = BODY_NAMES[Math.floor(Math.random() * BODY_NAMES.length)]!
      colour = COLOURS[Math.floor(Math.random() * COLOURS.length)]!.name
    }
    choose(body, colour)
  }

  useLayoutEffect(() => {
    const element = panel.current
    const anchor = frame.current
    if (!open || !element || !anchor) return
    element.showPopover()
    const place = () => {
      const face = anchor.getBoundingClientRect()
      const { offsetWidth: width, offsetHeight: height } = element
      const left = Math.min(Math.max(GAP, face.left + face.width / 2 - width / 2), window.innerWidth - width - GAP)
      const below = face.bottom + GAP
      const wanted = below + height <= window.innerHeight - GAP ? below : face.top - GAP - height
      const top = Math.max(GAP, Math.min(wanted, window.innerHeight - height - GAP))
      element.style.left = `${Math.round(left)}px`
      element.style.top = `${Math.round(top)}px`
    }
    place()
    element.querySelector<HTMLElement>('.avatar-editor-shapes [aria-pressed="true"]')?.focus({ preventScroll: true })

    let dismissedAt = -Infinity
    const inside = (event: Event) => {
      const path = event.composedPath()
      return path.includes(element) || (trigger.current !== null && path.includes(trigger.current))
    }
    const pointerDown = (event: PointerEvent) => {
      if (inside(event)) return
      dismissedAt = event.timeStamp
      closeRef.current()
    }
    // The click that ends a press on a modal dialog's backdrop would close the dialog as well.
    const click = (event: MouseEvent) => {
      if (event.timeStamp - dismissedAt < 1000 && event.composedPath()[0] instanceof HTMLDialogElement) event.stopPropagation()
    }
    // Escape closes the editor wherever focus is, ahead of the create dialog, which would otherwise
    // close on the same key.
    const keyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      event.stopPropagation()
      closeRef.current()
      trigger.current?.focus({ preventScroll: true })
    }
    document.addEventListener('pointerdown', pointerDown, true)
    window.addEventListener('keydown', keyDown, true)
    window.addEventListener('click', click, true)
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      document.removeEventListener('pointerdown', pointerDown, true)
      window.removeEventListener('keydown', keyDown, true)
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
      // Kept a moment longer than the edit, for the click that follows the press that ended it.
      window.setTimeout(() => window.removeEventListener('click', click, true), 1000)
      if (element.matches(':popover-open')) element.hidePopover()
    }
  }, [open])

  return (
    <div ref={frame} className={className}>
      <BotAvatar seed={open ? draft : seed} size={size} doing={doing} />
      <IconButton
        ref={trigger}
        icon="edit-pencil"
        label="Edit avatar"
        kind="filled"
        size="tiny"
        hasPopup="dialog"
        expanded={open}
        controls={panelId}
        className="bot-avatar-edit"
        onClick={() => {
          if (open) { close(); return }
          saved.current = seed
          setDraft(seed)
          setOpen(true)
        }}
      />
      <div ref={panel} id={panelId} popover="manual" role="dialog" aria-labelledby={titleId}
        className="avatar-editor" data-test="avatar-editor">
        <div className="avatar-editor-head">
          <strong id={titleId}>Edit bot avatar</strong>
          <Button kind="plain" size="tiny" className="avatar-editor-randomize" onClick={randomize}>Randomize</Button>
        </div>
        <div className="avatar-editor-shapes" role="group" aria-label="Shape">
          {BODY_NAMES.map((body) => (
            <button key={body} type="button" className="avatar-editor-option" aria-pressed={body === choice.body}
              aria-label={BODY_LABELS[body]} data-body={body} onClick={() => choose(body, choice.colour)}>
              <BotFace seed={pickedSeed(body, choice.colour)} size={32} />
            </button>
          ))}
        </div>
        <div className="avatar-editor-colors" role="group" aria-label="Color">
          {COLOURS.map((colour) => (
            <button key={colour.name} type="button" className="avatar-editor-option" aria-pressed={colour.name === choice.colour}
              aria-label={colourLabel(colour)} data-colour={colour.name} onClick={() => choose(choice.body, colour.name)}>
              <span className="avatar-editor-swatch"><PixelSwatch colour={colour.name} size={32} /></span>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
