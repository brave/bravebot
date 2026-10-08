/**
 * A bot's face.
 *
 * A pixel figure built from the bot's seed (`../avatar/pixels.ts`), drawn as an SVG of one rect
 * per cell. `BotAvatar` registers it with the shared clock (`../avatar/clock.ts`), which blinks
 * it, moves its eyes and nods it by changing attributes, so a blink never re-renders React.
 * `BotFace` is the same picture held still, for lists too long to animate.
 *
 * The face is decorative; a working or attention marker also has an accessible label. The marker
 * remains visible with reduced motion. `data-avatar` names the figure the seed chose, so a test
 * can say two bots have different faces, and that one bot's face survived a rename, without
 * comparing pixels.
 */

import { memo, useEffect, useRef } from 'react'
import { Icon } from '../nala'
import { show, tell, express, lookOf, type Expression, type Doing } from '../avatar/clock'
import { EYE_PUPIL, EYE_WHITE, LID, placeEyes, spriteOf, type Look, type Sprite } from '../avatar/pixels'

export type { Doing }

interface Props {
  /** The bot's stored seed. Anything goes; this only ever hashes it. */
  seed: string
  /** The side of the square, in CSS pixels. */
  size?: number
  /**
   * What the bot is doing, which decides how it holds its eyes; see `Doing` in `../avatar/clock`.
   * Defaults to glancing idly about, which is right for a row in a list that is not the one on
   * screen.
   */
  doing?: Doing
  /** An interactive expression for the About mascot; independent of task status. */
  expression?: Expression
}

export function BotAvatar({ seed, size = 38, doing = 'idle', expression = 'neutral' }: Props): React.JSX.Element {
  const svg = useRef<SVGSVGElement>(null)
  // The state at mount goes in with the registration; changes after that are told to the clock.
  // A ref rather than a dependency, so a state change does not re-register the face.
  const current = useRef({ doing, expression })
  current.current = { doing, expression }

  useEffect(() => {
    const element = svg.current
    if (!element) return
    return show(element, seed, current.current.doing, current.current.expression)
  }, [seed])

  useEffect(() => {
    if (svg.current) tell(svg.current, doing)
  }, [doing])

  useEffect(() => {
    if (svg.current) express(svg.current, expression)
  }, [expression])

  const hasStatus = doing === 'working' || doing === 'failed'
  return (
    <span
      className="bot-avatar-frame"
      style={{ width: size, height: size }}
      role={hasStatus ? 'img' : undefined}
      aria-label={hasStatus ? (doing === 'failed' ? 'Bot needs attention' : 'Bot working') : undefined}
      aria-hidden={hasStatus ? undefined : true}
      data-doing={doing}
    >
      <PixelSprite ref={svg} sprite={spriteOf(seed)} size={size} look={lookOf(spriteOf(seed), doing, expression)} />
      {hasStatus && (
        <span
          className={`bot-avatar-status bot-avatar-status-${doing}`}
          style={{ '--status-size': `${Math.max(10, Math.round(size * 0.26))}px` } as React.CSSProperties}
          aria-hidden="true"
        >
          <Icon name={doing === 'failed' ? 'warning-circle-filled' : 'clock'} />
        </span>
      )}
    </span>
  )
}

/**
 * A bot's face as a still picture, for places too numerous to animate: the mark beside a bot's
 * conversation in the session list, and an archived bot's row.
 */
export function BotFace({ seed, size = 16 }: { seed: string; size?: number }): React.JSX.Element {
  const sprite = spriteOf(seed)
  return (
    <span className="bot-face" style={{ width: size, height: size }} aria-hidden="true">
      <PixelSprite sprite={sprite} size={size} look={lookOf(sprite, 'idle', 'neutral')} />
    </span>
  )
}

/** The cells, which never change after the first render, kept apart so a pose does not diff them. */
const Cells = memo(function Cells({ sprite }: { sprite: Sprite }) {
  return <>{sprite.cells.map((cell) => <rect key={`${cell.x},${cell.y}`} x={cell.x} y={cell.y} width={1} height={1} fill={cell.fill} />)}</>
})

/**
 * The picture. The `data-part` names are what the clock looks up to move things, so they and the
 * attributes set here are the same ones `../avatar/clock.ts` writes.
 */
function PixelSprite({ sprite, size, look, ref }: { sprite: Sprite; size: number; look: Look; ref?: React.Ref<SVGSVGElement> }): React.JSX.Element {
  return (
    <svg
      ref={ref}
      className="bot-avatar"
      width={size}
      height={size}
      viewBox={`${sprite.view.x} ${sprite.view.y} ${sprite.view.size} ${sprite.view.size}`}
      // The nod moves the figure one cell past the view for a moment.
      overflow="visible"
      shapeRendering="crispEdges"
      data-avatar={sprite.signature}
      aria-hidden="true"
      focusable="false"
    >
      <g data-part="figure" transform={look.nod ? 'translate(0 1)' : undefined}>
        <Cells sprite={sprite} />
        <g data-part="eyes" transform={look.drop ? `translate(0 ${look.drop})` : undefined}>
          {placeEyes(sprite, look).map((eye, i) => (
            <g key={i} data-eye={i}>
              <rect data-part="white" x={eye.whiteX} y={eye.y} width={1} height={eye.height} fill={EYE_WHITE} visibility={eye.closed ? 'hidden' : undefined} />
              <rect data-part="pupil" x={eye.pupilX} y={eye.y} width={1} height={eye.height} fill={EYE_PUPIL} visibility={eye.closed ? 'hidden' : undefined} />
              <rect data-part="lid" x={Math.min(eye.whiteX, eye.pupilX)} y={sprite.eyes[i]!.y + LID.offset} width={2} height={LID.height} fill={EYE_PUPIL} visibility={eye.closed ? undefined : 'hidden'} />
            </g>
          ))}
        </g>
      </g>
    </svg>
  )
}
