import { memo, isValidElement, useState, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import { CopyIcon, WrapTextIcon } from 'lucide-react'
import { isSubpath } from '../../shared/files'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'

/**
 * The model's own words, formatted.
 *
 * Used for the assistant bubble and nowhere else. That is a trust boundary, not an
 * oversight: this UI marks confined content *structurally* — the hatched border on
 * `.quarantine`, the doubled `--warn` border on an untrusted write — precisely so that
 * text cannot imitate chrome. Formatting is an imitation tool. Give quarantined content
 * headings, bold and links and it has the vocabulary to counterfeit the very signals the
 * reader is meant to trust, sitting inches above it. So the fact that a bubble is
 * formatted is itself information: it says these words came from the planner, released
 * through `reply_for_display()`, rather than from a file somebody fetched.
 *
 * **No HTML path exists.** react-markdown renders to React elements, and without
 * `rehype-raw` — which is deliberately not installed — raw HTML in a reply arrives as
 * escaped, inert text. That is why there is no sanitizer here: there is nothing to
 * sanitise. `dangerouslySetInnerHTML` appears nowhere in this codebase and must not start
 * here.
 *
 * None of the above is left to the comment: `scripts/marking.test.mjs` renders these components
 * and asserts each of them on the markup, which is what
 * [LAYER-5](../../../../docs/specs/layering.md#LAYER-5) names as this surface's answer to the
 * rule that any surface showing released content marks it.
 */

/** Schemes a link may use. Everything else is drawn as text rather than as a link. */
const OPENABLE = new Set(['http:', 'https:', 'mailto:'])

/**
 * The href to use, or `null` to draw the text without a link.
 *
 * Stricter than it looks like it needs to be, because a link here is a real capability:
 * the main process answers a window-open by handing the URL to `shell.openExternal`, so
 * `file:` would ask the OS to open a path of the model's choosing. Parsing rather than
 * matching a prefix is what catches the encoded and whitespace-padded spellings of
 * `javascript:` that a string comparison waves through. A relative URL fails too, and
 * should: there is nothing for it to be relative *to* in a `file://` renderer.
 */
export function safeUrl(href: string | undefined): string | null {
  if (!href) return null
  try {
    return OPENABLE.has(new URL(href).protocol) ? href : null
  } catch {
    return null
  }
}

/**
 * Hoisted so they are not fresh objects on every render.
 *
 * `Row` re-renders whenever anything at all happens in a turn — a token count ticks
 * several times a second — and new plugin and component objects on each pass would defeat
 * the memoisation below for no reason.
 */
const PLUGINS = [
  remarkGfm,
  // Soft breaks become real ones, which is what the plain-text renderer this replaces did
  // and what every chat interface does. Without it a reply written as short unbulleted
  // lines reflows into one paragraph — the one way this change could visibly damage prose
  // that reads correctly today.
  remarkBreaks,
]

const COMPONENTS: Components = {
  p({ children }) {
    return <p className="mb-[0.7em] last:mb-0 first:mt-0">{children}</p>
  },
  h1({ children }) {
    return <h1 className="mt-[0.9em] mb-[0.4em] text-[15px] font-semibold leading-snug first:mt-0">{children}</h1>
  },
  h2({ children }) {
    return <h2 className="mt-[0.9em] mb-[0.4em] text-sm font-semibold leading-snug first:mt-0">{children}</h2>
  },
  h3({ children }) {
    return <h3 className="mt-[0.9em] mb-[0.4em] text-[13px] font-semibold leading-snug first:mt-0">{children}</h3>
  },
  h4({ children }) {
    return <h4 className="mt-[0.9em] mb-[0.4em] text-[13px] font-semibold leading-snug first:mt-0">{children}</h4>
  },
  h5({ children }) {
    return <h5 className="mt-[0.9em] mb-[0.4em] text-[13px] font-semibold leading-snug first:mt-0">{children}</h5>
  },
  h6({ children }) {
    return <h6 className="mt-[0.9em] mb-[0.4em] text-[13px] font-semibold leading-snug first:mt-0">{children}</h6>
  },
  ul({ children, className }) {
    return (
      <ul className={cn(
        'mb-[0.7em] flex list-disc flex-col gap-[0.15em] pl-[1.4em] first:mt-0 last:mb-0',
        className?.includes('contains-task-list') && 'list-none pl-[0.2em]',
        className,
      )}>
        {children}
      </ul>
    )
  },
  ol({ children }) {
    return <ol className="mb-[0.7em] flex list-decimal flex-col gap-[0.15em] pl-[1.4em] first:mt-0 last:mb-0">{children}</ol>
  },
  li({ children }) {
    return <li className="leading-snug">{children}</li>
  },
  blockquote({ children }) {
    return (
      <blockquote className="my-[0.6em] border-l-2 border-border pl-2.5 text-muted-foreground">
        {children}
      </blockquote>
    )
  },
  hr() {
    return <hr className="my-[0.9em] border-0 border-t border-border" />
  },
  code({ className, children }) {
    const fenced = typeof className === 'string' && className.includes('language-')
    if (fenced) {
      return <code className={cn('font-mono text-[11.5px]', className)}>{children}</code>
    }
    return (
      <code className="rounded bg-code px-1 py-px font-mono text-[11.5px]">
        {children}
      </code>
    )
  },
  pre({ children }) { return <CodeBlock>{children}</CodeBlock> },
  a({ href, children }) {
    const url = safeUrl(href)
    const local = href?.replace(/^\.\//, '').replace(/(?::\d+|#L\d+)$/, '')
    if (!url && local && isSubpath(local) && !local.includes(':')) return (
      <Button
        variant="link"
        size="sm"
        // Underlined at rest, not on hover: a path the model wrote into a sentence has to be
        // visibly a thing you can open, since nothing about the words says so.
        className="local-file-link inline h-auto p-0 underline"
        onClick={() => {
          document.dispatchEvent(new CustomEvent('bravebot:preview-file', { detail: local }))
        }}
      >
        {children}
      </Button>
    )
    // `target="_blank"` is load-bearing, not decoration. The main process refuses
    // in-window navigation outright and answers a window-open by opening the user's
    // browser, so this is the only form of link that does anything at all.
    return url ? (
      // The destination in the tooltip, which matters more here than anywhere else in the
      // window: the text of this link was written by the model, and nothing obliges it to
      // describe where the link goes. A browser gives you the URL in a status bar before
      // you commit to it; there is no status bar here, so this is it.
      <a
        className="text-primary underline-offset-2 hover:underline"
        href={url}
        target="_blank"
        rel="noopener noreferrer nofollow"
        title={url}
      >
        {children}
      </a>
    ) : (
      // Drawn as text, because an anchor that cannot be followed is a lie about what
      // clicking it will do. Only the dotted rule is dimmed: dimming the words as well
      // would read as a disabled control rather than as prose carrying a dead reference.
      <span className="md-dead-link underline decoration-muted-foreground/70 decoration-dotted">{children}</span>
    )
  },

  img({ src, alt, title }) {
    // Never an `<img>`. The CSP allows `data:` images, so rendering them would let a reply
    // paint arbitrary pixels beside the app's own chrome, and remote ones are blocked
    // outright and would show as a broken glyph indistinguishable from a bug. A label says
    // more than either, and keeps the alt text the model wrote.
    const url = safeUrl(typeof src === 'string' ? src : undefined)
    const label = alt || title || 'image'
    return url ? (
      // Likewise, and more so: the label here is the model's own alt text, which says even
      // less about the destination than link text usually does.
      <a
        className="md-image rounded-[4px] bg-code px-[5px] text-[10px] text-primary underline-offset-2 hover:underline"
        href={url}
        target="_blank"
        rel="noopener noreferrer nofollow"
        title={url}
      >
        image · {label}
      </a>
    ) : (
      <span className="md-image rounded-[4px] bg-code px-[5px] text-[10px] text-muted-foreground">image · {label}</span>
    )
  },

  table({ children }) {
    // A table wider than the bubble scrolls inside it rather than stretching the column.
    return (
      <div className="md-table-wrap my-[0.7em] max-w-full overflow-x-auto">
        <table className="border-collapse text-xs">{children}</table>
      </div>
    )
  },
  th({ children }) {
    return <th className="border border-border bg-code px-2 py-1 text-left font-semibold">{children}</th>
  },
  td({ children }) {
    return <td className="border border-border px-2 py-1">{children}</td>
  },
}

function plain(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(plain).join('')
  return isValidElement<{ children?: ReactNode }>(node) ? plain(node.props.children) : ''
}

function CodeBlock({ children }: { children: ReactNode }): React.JSX.Element {
  const [wrap, setWrap] = useState(false)
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState(false)
  const language = isValidElement<{ className?: string }>(children) ? children.props.className?.replace('language-', '') : undefined
  return (
    // The same frame the diff review wears, so a block of code the reply wrote and a block of code
    // the agent proposes are the same object on screen.
    <div className="code-block my-[0.7em] flex flex-col gap-0 overflow-hidden rounded-[9px] border border-border">
      <div className="code-toolbar flex items-center gap-2 border-b border-border bg-bubble-agent px-2.5 py-1.5 text-xs text-muted-foreground">
        <span className="min-w-0 flex-1 truncate">{language || 'Code'}</span>
        <Button variant="ghost" size="xs" aria-pressed={wrap} onClick={() => setWrap(!wrap)}>
          <WrapTextIcon data-icon="inline-start" />
          Wrap
        </Button>
        <Button
          variant="ghost"
          size="xs"
          onClick={() => {
            void navigator.clipboard.writeText(plain(children)).then(() => {
              setCopied(true)
              setError(false)
            }).catch(() => setError(true))
          }}
        >
          <CopyIcon data-icon="inline-start" />
          {copied ? 'Copied' : 'Copy code'}
        </Button>
      </div>
      {error && <p role="alert" className="px-2.5 text-xs text-destructive">Could not copy. Select the code and copy it manually.</p>}
      {/* Square, because the frame above already has the corners. Code scrolls rather than wraps
          unless somebody asks: a wrapped line misrepresents what is actually in the file, which is
          the one thing this app does not do to a reader. The `code` inside has to be told as well,
          or the child's own `white-space: pre` keeps winning. */}
      <pre className={cn(
        'm-0 max-w-full overflow-x-auto rounded-none bg-code px-2.5 py-2 font-mono text-[11.5px]',
        wrap && 'code-wrapped whitespace-pre-wrap [overflow-wrap:anywhere] [&_code]:whitespace-pre-wrap [&_code]:[overflow-wrap:anywhere]',
      )}>
        {children}
      </pre>
    </div>
  )
}

/**
 * Memoised on the one string it takes.
 *
 * The parse is the expensive part and the reply never changes once it has arrived — there
 * is no token streaming, so a bubble is parsed exactly once no matter how long the turn
 * runs afterwards.
 */
export const Markdown = memo(function Markdown({ text }: { text: string }): React.JSX.Element {
  return (
    <ReactMarkdown remarkPlugins={PLUGINS} components={COMPONENTS}>
      {text}
    </ReactMarkdown>
  )
})
