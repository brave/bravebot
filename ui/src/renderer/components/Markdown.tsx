import { memo, isValidElement, useState, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import { isSubpath } from '../../shared/files'
import { Button } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import { Alert, AlertDescription } from './ui/alert'
import { Card, CardContent, CardHeader } from './ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table'
import { Toggle } from './ui/toggle'

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
  pre({ children }) { return <CodeBlock>{children}</CodeBlock> },
  a({ href, children }) {
    const url = safeUrl(href)
    const local = href?.replace(/^\.\//, '').replace(/(?::\d+|#L\d+)$/, '')
    if (!url && local && isSubpath(local) && !local.includes(':')) return <Button variant="link" className="local-file-link h-auto p-0 font-inherit" onClick={() => {
      document.dispatchEvent(new CustomEvent('bravebot:preview-file', { detail: local }))
    }}>{children}</Button>
    // `target="_blank"` is load-bearing, not decoration. The main process refuses
    // in-window navigation outright and answers a window-open by opening the user's
    // browser, so this is the only form of link that does anything at all.
    return url ? (
      // The destination in the tooltip, which matters more here than anywhere else in the
      // window: the text of this link was written by the model, and nothing obliges it to
      // describe where the link goes. A browser gives you the URL in a status bar before
      // you commit to it; there is no status bar here, so this is it.
      <Button asChild variant="link" className="h-auto p-0 font-inherit">
        <a href={url} target="_blank" rel="noopener noreferrer nofollow" title={url}>
          {children}
        </a>
      </Button>
    ) : (
      // Drawn as text, because an anchor that cannot be followed is a lie about what
      // clicking it will do.
      <span className="md-dead-link">{children}</span>
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
        className="md-image"
        href={url}
        target="_blank"
        rel="noopener noreferrer nofollow"
        title={url}
      >
        image · {label}
      </a>
    ) : (
      <span className="md-image">image · {label}</span>
    )
  },

  table({ children }) {
    return <div className="md-table-wrap my-[0.9em] max-w-full overflow-x-auto rounded-lg border border-border"><Table>{children}</Table></div>
  },
  thead({ children }) { return <TableHeader>{children}</TableHeader> },
  tbody({ children }) { return <TableBody>{children}</TableBody> },
  tr({ children }) { return <TableRow>{children}</TableRow> },
  th({ children }) { return <TableHead>{children}</TableHead> },
  td({ children }) { return <TableCell>{children}</TableCell> },
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
  return <Card className="code-block my-3 gap-0 overflow-hidden py-0">
    <CardHeader className="code-toolbar flex-row items-center gap-2 border-b px-2.5 py-1.5 [.border-b]:pb-1.5">
      <span className="min-w-0 flex-1 text-[11px] font-medium text-muted-foreground">{language || 'Code'}</span>
      <ButtonGroup>
        <Toggle pressed={wrap} onPressedChange={setWrap}>Wrap</Toggle>
        <Button variant="outline" size="sm" onClick={() => { void navigator.clipboard.writeText(plain(children)).then(() => { setCopied(true); setError(false) }).catch(() => setError(true)) }}>{copied ? 'Copied' : 'Copy code'}</Button>
      </ButtonGroup>
    </CardHeader>
    {error && <Alert variant="destructive"><AlertDescription>Could not copy. Select the code and copy it manually.</AlertDescription></Alert>}
    <CardContent className="p-0"><pre className={wrap ? 'code-wrapped m-0 whitespace-pre-wrap break-anywhere' : 'm-0'}>{children}</pre></CardContent>
  </Card>
}

/**
 * Memoised on the one string it takes.
 *
 * The parse is the expensive part and the reply never changes once it has arrived — there
 * is no token streaming, so a bubble is parsed exactly once no matter how long the turn
 * runs afterwards.
 *
 * Note there is deliberately no `code` override: react-markdown dropped the `inline` prop
 * in v9, and the usual workaround sniffs a class name to guess what it was. CSS already
 * knows the difference between `code` and `pre code` without guessing.
 */
export const Markdown = memo(function Markdown({ text }: { text: string }): React.JSX.Element {
  return (
    <ReactMarkdown remarkPlugins={PLUGINS} components={COMPONENTS}>
      {text}
    </ReactMarkdown>
  )
})
