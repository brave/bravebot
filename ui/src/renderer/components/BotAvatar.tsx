/**
 * A bot's face.
 *
 * A pixel figure built from the bot's seed (`../avatar/pixels.ts`), drawn as an SVG of one rect
 * per cell, with light and shade over the cells so it reads as a solid toy rather than a flat
 * tile. `BotAvatar` registers it with the shared clock (`../avatar/clock.ts`), which blinks it,
 * moves its eyes and plays its gestures by changing attributes and starting animations, so a blink
 * never re-renders React. `BotFace` is the same picture held still, for lists too long to animate.
 *
 * At `DETAIL` pixels and over, a face also has a catch-light in each pupil, and the animated one
 * breathes (`styles/avatar.css`). Under that, the extra detail is smaller than a screen pixel and
 * would only blur the cells.
 *
 * The face is decorative; a working or attention marker also has an accessible label. The marker
 * remains visible with reduced motion. `data-avatar` names the figure the seed chose, so a test
 * can say two bots have different faces, and that one bot's face survived a rename, without
 * comparing pixels.
 */

import { memo, useEffect, useId, useRef } from 'react'
import { Icon } from '../nala'
import { show, tell, lookOf, pop, type Doing } from '../avatar/clock'
import {
  EYE_PARTS, EYE_PUPIL, EYE_WHITE, LIGHT, SWATCH, placeEyes, spriteOf, swatchOf,
  type EyePart, type Look, type Sprite,
} from '../avatar/pixels'

export type { Doing }

/** The least size, in CSS pixels, at which a face shows its catch-lights and moves its body. */
export const DETAIL = 32

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
}

/** The marker and its label for each state that has one. Thinking is working, as far as a person is told. */
const STATUS = {
  thinking: { icon: 'clock', label: 'Bot working' },
  working: { icon: 'clock', label: 'Bot working' },
  asking: { icon: 'message-bubble-ask', label: 'Bot waiting for your answer' },
  failed: { icon: 'warning-circle-filled', label: 'Bot needs attention' },
} as const

export function BotAvatar({ seed, size = 38, doing = 'idle' }: Props): React.JSX.Element {
  const svg = useRef<SVGSVGElement>(null)
  // The state at mount goes in with the registration; changes after that are told to the clock.
  // A ref rather than a dependency, so a state change does not re-register the face.
  const current = useRef({ doing })
  current.current = { doing }

  // The seed first drawn here, so a face changed in place (a new look in the editor) pops in, and
  // one drawn for the first time does not: nothing animates on first paint.
  const first = useRef(seed)
  useEffect(() => {
    const element = svg.current
    if (!element) return
    const stop = show(element, seed, current.current.doing, size >= DETAIL)
    if (seed !== first.current) pop(element)
    first.current = seed
    return stop
  }, [seed, size])

  useEffect(() => {
    if (svg.current) tell(svg.current, doing)
  }, [doing])

  const hasStatus = doing === 'thinking' || doing === 'working' || doing === 'asking' || doing === 'failed'
  const status = STATUS[doing as keyof typeof STATUS]
  return (
    <span
      className="bot-avatar-frame"
      style={{ width: size, height: size }}
      role={hasStatus ? 'img' : undefined}
      aria-label={hasStatus ? status.label : undefined}
      aria-hidden={hasStatus ? undefined : true}
      data-doing={doing}
      data-detail={size >= DETAIL ? '' : undefined}
    >
      <PixelSprite ref={svg} sprite={spriteOf(seed)} size={size} look={lookOf(spriteOf(seed), doing)} />
      {hasStatus && (
        <span
          className={`bot-avatar-status bot-avatar-status-${doing}`}
          style={{ '--status-size': `${Math.max(10, Math.round(size * 0.26))}px` } as React.CSSProperties}
          aria-hidden="true"
        >
          <Icon name={status.icon} />
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
    <span className="bot-face" style={{ width: size, height: size }} aria-hidden="true" data-detail={size >= DETAIL ? '' : undefined}>
      <PixelSprite sprite={sprite} size={size} look={lookOf(sprite, 'idle')} />
    </span>
  )
}

/** A colour as a square of pixels mixed the way a face in it is, for the avatar editor's swatches. */
export function PixelSwatch({ colour, size }: { colour: string; size: number }): React.JSX.Element {
  return (
    <svg className="pixel-swatch" width={size} height={size} viewBox={`1 1 ${SWATCH} ${SWATCH}`}
      shapeRendering="crispEdges" aria-hidden="true" focusable="false">
      {swatchOf(colour).map((cell) => <rect key={`${cell.x},${cell.y}`} x={cell.x} y={cell.y} width={1} height={1} fill={cell.fill} />)}
    </svg>
  )
}

const EYE_FILL: Record<EyePart, string> = {
  white: EYE_WHITE, pupil: EYE_PUPIL, glint: EYE_WHITE, lid: EYE_PUPIL, 'foot-0': EYE_PUPIL, 'foot-1': EYE_PUPIL,
}

/**
 * The cells and the light on them, which never change after the first render, kept apart so a pose
 * does not diff them. Each part is a group of its own, which a gesture moves; `shading` is the id
 * of the gradient that darkens each part toward its lower right.
 */
const Cells = memo(function Cells({ sprite, shading }: { sprite: Sprite; shading: string }) {
  return <>{sprite.groups.map((group) => (
    <g key={`${group.part}-${group.side ?? ''}`} data-part={group.part} data-side={group.side}>
      {group.cells.map((cell) => <rect key={`${cell.x},${cell.y}`} x={cell.x} y={cell.y} width={1} height={1} fill={cell.fill} />)}
      <path d={group.outline} fill={`url(#${shading})`} />
      {group.light.map((light) => (
        <rect key={`${light.x},${light.y}`} x={light.x} y={light.y} width={1} height={1}
          fill={LIGHT[light.tone].fill} fillOpacity={LIGHT[light.tone].opacity} />
      ))}
    </g>
  ))}</>
})

/**
 * The picture. The `data-part` names are what the clock looks up to move things, so they and the
 * attributes set here are the same ones `../avatar/clock.ts` writes.
 */
function PixelSprite({ sprite, size, look, ref }: { sprite: Sprite; size: number; look: Look; ref?: React.Ref<SVGSVGElement> }): React.JSX.Element {
  // An id per picture, since two faces on one page would otherwise share one gradient.
  const shading = `avatar-shade-${useId().replace(/[^a-zA-Z0-9_-]/g, '')}`
  const detail = size >= DETAIL
  const { sheen } = LIGHT
  return (
    <svg
      ref={ref}
      className="bot-avatar"
      width={size}
      height={size}
      viewBox={`${sprite.view.x} ${sprite.view.y} ${sprite.view.size} ${sprite.view.size}`}
      // The nod moves the figure one cell past the view for a moment, and a hop moves it further.
      overflow="visible"
      shapeRendering="crispEdges"
      data-avatar={sprite.signature}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <radialGradient id={shading} cx={sheen.cx} cy={sheen.cy} r={sheen.r}>
          <stop offset={sheen.clear} stopColor={EYE_PUPIL} stopOpacity={0} />
          <stop offset={1} stopColor={EYE_PUPIL} stopOpacity={sheen.opacity} />
        </radialGradient>
      </defs>
      <g data-part="figure" transform={look.nod ? 'translate(0 1)' : undefined}>
        <Cells sprite={sprite} shading={shading} />
        <g data-part="eyes" transform={look.drop ? `translate(0 ${look.drop})` : undefined}>
          {placeEyes(sprite, look).map((eye, i) => (
            <g key={i} data-eye={i}>
              {EYE_PARTS.filter((part) => detail || part !== 'glint').map((part) => {
                const at = eye.rects[part]
                return <rect key={part} data-part={part} x={at.x} y={at.y} width={at.width} height={at.height}
                  fill={EYE_FILL[part]} visibility={at.hidden ? 'hidden' : undefined} />
              })}
            </g>
          ))}
        </g>
      </g>
    </svg>
  )
}
