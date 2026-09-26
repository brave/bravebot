/**
 * A conversation, laid out for paper.
 *
 * The bubbles are the transcript's own: same class names, same `Markdown` component for a
 * reply, same everything. That is the point of drawing the PDF in a renderer at all — there
 * is one implementation of what a reply looks like, so the file cannot drift from the window
 * and a link in an exported reply is gated by the same `safeUrl` that gates it on screen.
 *
 * Print arrangement lives in utilities here and a thin `@page` / overflow block in
 * `globals.css`. Paper has no scroll bars, no dark mode worth having, and no reason to
 * align one speaker against the right margin. The role label carries who spoke instead,
 * because the chat-window trick of position-means-speaker stops being legible the moment a
 * turn breaks across a page.
 */

import { Markdown } from './Markdown'
import { isTool, omitted, toolLine, where, type ExportDocument } from '../../shared/export'
import { Bubble, BubbleContent } from '@/components/ui/bubble'
import { Message, MessageContent, MessageHeader } from '@/components/ui/message'
import { cn } from '@/lib/utils'

/** The same fixed locale the text and markdown exports use, for the same reason. */
function when(at: number): string {
  return new Date(at).toLocaleString('en-GB', { dateStyle: 'long', timeStyle: 'short' })
}

export function ExportView({
  document,
  at,
}: {
  document: ExportDocument
  at: number
}): React.JSX.Element {
  return (
    <article
      className={cn(
        'export flex flex-col gap-4 bg-white text-foreground',
        '[print-color-adjust:exact] [-webkit-print-color-adjust:exact]',
      )}
    >
      <header className="export-head mb-4 flex flex-col gap-1 border-b border-border pb-3">
        <h1 className="m-0 text-xl font-semibold">{document.title}</h1>
        <p className="where m-0 text-xs text-muted-foreground">{where(document)}</p>
        <p className="exported-at m-0 text-[11px] text-muted-foreground">Exported {when(at)}</p>
      </header>

      <div className="flex flex-col gap-3.5">
        {document.turns.map((turn, index) =>
          // Indexed because a turn has no id of its own here — the document that crossed is
          // the parsed one, and giving it ids in the renderer would be inventing a field the
          // boundary does not carry. The list never reorders, so the index is stable.
          isTool(turn) ? (
            // No role line and no bubble. A call is not a third speaker, and drawing it as one
            // would put the machinery on the same footing as the two people in the document.
            // `toolLine` composes the sentence the other two formats print, so a call reads the
            // same however the session was exported.
            <p
              className={cn(
                'export-tool ml-[18px] font-mono text-[10px] text-muted-foreground break-inside-avoid',
                turn.failed && 'failed italic',
              )}
              key={index}
            >
              {toolLine(turn)}
            </p>
          ) : (
            <Message
              key={index}
              align={turn.role === 'user' ? 'end' : 'start'}
              className="export-turn mb-3.5"
            >
              <MessageContent className="max-w-full">
                <MessageHeader className="role mb-0.5 px-0 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase break-after-avoid">
                  {turn.role === 'user' ? 'You' : 'Brave Bot'}
                </MessageHeader>
                {turn.role === 'user' ? (
                  <Bubble
                    variant="default"
                    align="end"
                    className="bubble user m-0 max-w-full break-inside-auto"
                  >
                    <BubbleContent className="whitespace-pre-wrap">{turn.text}</BubbleContent>
                  </Bubble>
                ) : (
                  <Bubble
                    variant="ghost"
                    align="start"
                    className="bubble assistant m-0 max-w-full break-inside-auto [&_a]:underline [&_pre]:overflow-visible [&_pre]:whitespace-pre-wrap [&_pre]:break-words [&_td]:whitespace-normal [&_th]:whitespace-normal"
                  >
                    <BubbleContent>
                      <Markdown text={turn.text} />
                    </BubbleContent>
                  </Bubble>
                )}
              </MessageContent>
            </Message>
          ),
        )}
      </div>

      {/* The same sentence the other two formats end with — and, like theirs, the one that
          matches what this document actually carried. A reader holding the printout should
          not have to know which parts of a session an export leaves behind. */}
      <footer className="export-foot mt-5 border-t border-border pt-2.5 text-[10px] text-muted-foreground">
        {omitted(document)}
      </footer>
    </article>
  )
}
