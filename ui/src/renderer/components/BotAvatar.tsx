/**
 * A bot's face.
 *
 * A small three-dimensional figure, built from the bot's seed and turning slowly. What it is made
 * of and why it looks the way it does is `../avatar/figure.ts`; how twenty of them share one WebGL
 * context is `../avatar/stage.ts`. This is the part that puts one in a row.
 *
 * It is a `<canvas>` that the shared renderer copies into, rather than a canvas with a context of
 * its own — a page gets a limited number of WebGL contexts and a list of bots is exactly where
 * that limit is met.
 *
 * ## The fallback, and why there is one
 *
 * WebGL can be unavailable: software rendering, a driver that will not start, a machine with the
 * GPU process off. A list of bots with no faces in it is a worse list but still a list, so where
 * there is no renderer this draws the same face flat — the head, the body, the two eyes and their
 * catchlights, in the same shades, from the same seed. It used to be a mirrored grid, which was a
 * mark rather than a face and a different visual language from the figure it stood in for; a bot
 * seen on two machines should look like the same bot, allowing for one of them being a drawing.
 * Nothing about a bot depends on the picture, and a column that failed to render its rows because
 * of a driver would be the tail wagging the dog.
 *
 * ## What it says
 *
 * The face is decorative; a working or attention marker also has an accessible label.
 * The marker remains visible with reduced motion and without WebGL. `data-avatar` carries the figure
 * the seed chose — `blue-squat-bobble-plain-big-pill-collar`, its colour and then its form — which
 * is there so a test can say two bots have different faces, and that one bot's face survived a
 * rename, without comparing pixels of a picture that is moving while it is compared.
 */

import { useEffect, useId, useRef } from 'react'
import { available, show, tell, express, type Expression, type Doing } from '../avatar/stage'
import { paintsOf, signature, traitsOf, headWidth, eyeDimensions, torsoDimensions, type Traits } from '../avatar/figure'

export type { Doing }

interface Props {
  /** The bot's stored seed. Anything goes; this only ever hashes it. */
  seed: string
  /** The side of the square, in CSS pixels. */
  size?: number
  /**
   * What the bot is doing, which decides its posture — see `Doing` in `../avatar/stage`. Defaults
   * to looking idly about, which is right for a row in a list that is not the one on screen.
   */
  doing?: Doing
  /** An interactive expression for the About mascot; independent of task status. */
  expression?: Expression
}

export function BotAvatar({ seed, size = 38, doing = 'idle', expression = 'neutral' }: Props): React.JSX.Element {
  const canvas = useRef<HTMLCanvasElement>(null)
  // The state at mount goes in with the figure, so a bot that is already working when its row
  // appears takes up the posture rather than blending into it from idle. Changes after that are
  // told to the stage, which blends them. A ref rather than a dependency: a state change must not
  // rebuild the figure, since rebuilding it would restart its turn.
  const current = useRef(doing)
  current.current = doing

  useEffect(() => {
    const element = canvas.current
    if (!element) return
    return show(element, seed, current.current, size)
  }, [seed, size])

  useEffect(() => {
    if (canvas.current) tell(canvas.current, doing)
  }, [doing])

  useEffect(() => {
    if (canvas.current) express(canvas.current, expression)
  }, [expression, seed, size])

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
      {available() ? (
        <canvas
          ref={canvas}
          className="bot-avatar"
          width={size * 2}
          height={size * 2}
          style={{ width: size, height: size }}
          data-avatar={signature(seed)}
          aria-hidden="true"
        />
      ) : <FlatAvatar seed={seed} size={size} doing={doing} expression={expression} />}
      {hasStatus && (
        <span
          className={`bot-avatar-status bot-avatar-status-${doing}`}
          style={{ width: Math.max(7, Math.round(size * 0.23)), height: Math.max(7, Math.round(size * 0.23)) }}
          aria-hidden="true"
        >
          {doing === 'failed' ? (
            <svg viewBox="0 0 12 12" focusable="false"><path d="M6 2v4M6 9v.1" /></svg>
          ) : (
            <svg viewBox="0 0 12 12" focusable="false"><path d="M6 2v4l2.5 1.5" /></svg>
          )}
        </span>
      )}
    </span>
  )
}

/** The head's half-widths, by trait, in a 100-unit square. The same proportions as the figure's. */
const HEADS: Record<Traits['head'], { rx: number; ry: number }> = {
  round: { rx: 32, ry: 31 },
  squat: { rx: 35, ry: 27 },
  tall: { rx: 29, ry: 35 },
  boxy: { rx: 34, ry: 28 },
}

/**
 * The flat face, for a machine that cannot draw the other one.
 *
 * The same figure drawn as a picture rather than a model: the head in the colour itself, the body
 * in the deep shade under it, the small pieces in the pale one, two dark eyes low on the face with a
 * catchlight each, and the same drawn edge around everything. Read straight from the traits, so it
 * is recognisably the bot the figure would have been — the same head shape, the same thing on top,
 * the same set of the eyes — rather than a different mark in the same colour.
 */
function FlatAvatar({ seed, size, doing, expression }: { seed: string; size: number; doing: Doing; expression: Expression }): React.JSX.Element {
  const gradient = useId()
  const traits = traitsOf(seed)
  const paints = paintsOf(seed)
  const baseHead = HEADS[traits.head]
  const head = { rx: baseHead.rx, ry: baseHead.ry * (traits.proportion === 'compact' ? 0.88 : 1) }
  const torso = torsoDimensions(traits.torso)
  const dimensions = eyeDimensions(traits.eyeShape)
  const cx = 50
  const cy = 45
  const top = cy - head.ry
  // The eyes, low and wide-set as on the figure; `big` is bigger rather than further apart.
  const spread = traits.eyes === 'wide' ? 14 : traits.eyes === 'close' ? 9.5 : 12
  const eye = (traits.eyes === 'big' ? 6.5 : 5.2) * (size <= 30 ? 1.16 : 1)
  const contour = Array.from({ length: 65 }, (_, i) => {
    const angle = i / 64 * Math.PI * 2
    const curve = (v: number) => traits.head === 'boxy' ? Math.sign(v) * Math.sqrt(Math.abs(v)) : v
    const y = curve(Math.sin(angle))
    const x = curve(Math.cos(angle)) * headWidth(traits.proportion, y)
    return `${i === 0 ? 'M' : 'L'}${cx + head.rx * x} ${cy - head.ry * y}`
  }).join(' ') + ' Z'

  return (
    <svg
      className="bot-avatar"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      data-avatar={signature(seed)}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="0" y2="1">
          <stop offset="55%" stopColor={paints.base} />
          <stop offset="100%" stopColor={paints.deep} />
        </linearGradient>
      </defs>
      {/* Retain the body silhouette even when WebGL is unavailable. */}
      {traits.version === 2 && <rect x={40} y={65} width={20} height={30} rx={5} fill={traits.torso === 'neck' ? paints.pale : paints.deep} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />}
      <g transform={`translate(50 ${90 - (torso.y + 1.35) * 30}) scale(${torso.width} ${torso.height}) translate(-50 -90)`}>
      {traits.body === 'dome' ? (
        <ellipse cx={50} cy={94} rx={29} ry={25} fill={paints.deep} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      ) : traits.body === 'barrel' ? (
        <path d="M28 70 Q50 66 72 70 L76 108 H24 Z" fill={paints.deep} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      ) : (
        <rect x={30} y={70} width={40} height={46} rx={18} fill={paints.deep} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      )}
      </g>
      <ellipse cx={50} cy={74} rx={20} ry={5} fill={paints.edge} opacity={0.15} />
      {traits.collar && (
        <rect x={31} y={cy + head.ry - 1} width={38} height={4} rx={3.5} fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      )}
      {traits.ears &&
        [-1, 1].map((side) => (
          <ellipse
            key={side}
            cx={cx + side * head.rx * headWidth(traits.proportion, 0)} cy={cy}
            rx={traits.earShape === 'fin' ? 3.5 : traits.earShape === 'pod' ? 7 : 5}
            ry={traits.earShape === 'fin' ? 11 : traits.earShape === 'pod' ? 6.5 : 7}
            transform={traits.earShape === 'fin' ? `rotate(${side * 14} ${cx + side * head.rx} ${cy})` : undefined}
            fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round"
          />
        ))}
      {/* The head. An ellipse for the three round ones and a rounded rect for the boxy one. */}
      {traits.version === 2 ? (
        <path d={contour} fill={`url(#${gradient})`} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      ) : traits.head === 'boxy' ? (
        <rect x={cx - head.rx} y={top} width={head.rx * 2} height={head.ry * 2} rx={12} fill={`url(#${gradient})`} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      ) : (
        <ellipse cx={cx} cy={cy} rx={head.rx} ry={head.ry} fill={`url(#${gradient})`} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
      )}
      {traits.crown === 'bobble' && <ellipse cx={cx} cy={top + 2} rx={9.5} ry={7.5} fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />}
      {traits.crown === 'antenna' && (
        <>
          <rect x={cx - 1.8} y={top - 8} width={3.6} height={10} fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
          <circle cx={cx} cy={Math.max(5, top - 8)} r={4.5} fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
        </>
      )}
      {traits.crown === 'tuft' &&
        [-1, 0, 1].map((side) => (
          <circle key={side} cx={cx + side * 8.5} cy={Math.max(7, top + (side === 0 ? -3 : 2))} r={7} fill={paints.pale} stroke={paints.edge} strokeWidth={1.4} strokeLinejoin="round" />
        ))}
      {traits.faceShape === 'oval' && <ellipse cx={cx} cy={cy + 5} rx={24} ry={13} fill={paints.face} />}
      {traits.faceShape === 'panel' && <rect x={cx - 24} y={cy - 8} width={48} height={26} rx={7} fill={paints.face} />}
      {/* The eyes and their catchlights, up and to the outside on both as on the figure. */}
      <g transform={expression === 'curious' ? `translate(0 ${cy + 5}) scale(1 1.3) translate(0 ${-(cy + 5)})` : undefined}>
      {[-1, 1].map((side) => (
        <g key={side} transform={expression === 'wink' && side === -1 ? `translate(0 ${cy + 5}) scale(1 .08) translate(0 ${-(cy + 5)})` : doing === 'working' ? 'translate(3 6)' : undefined}>
          {traits.eyeShape === 'square' ? (
            <rect x={cx + side * spread - eye} y={cy + 5 - eye} width={eye * 2} height={eye * 2} rx={eye * 0.4} fill="#20222b" />
          ) : (
            <ellipse cx={cx + side * spread} cy={cy + 5} rx={eye * dimensions.x} ry={eye * dimensions.y} fill="#20222b" />
          )}
          <circle visibility={expression === 'wink' && side === -1 ? 'hidden' : undefined} cx={cx + side * spread + eye * (traits.version === 1 ? 0.34 : dimensions.x * 0.28)} cy={cy + 5 - eye * (traits.version === 1 ? 0.36 : dimensions.y * 0.28)} r={eye * 0.42} fill="#ffffff" />
        </g>
      ))}
      </g>
    </svg>
  )
}
