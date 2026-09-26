import { useCallback, useEffect, useRef, useState } from 'react'
import { BRAVE, findTheme, roleVariables, type Theme } from '../../shared/theme'
import { applyTheme } from '../theme'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Command,
  CommandGroup,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog'
import { Kbd } from '@/components/ui/kbd'

interface Props {
  themes: readonly Theme[]
  /** The name in force when the picker opened, and the row it opens on. */
  chosen: string
  /** Where a palette somebody writes goes. Printed, so that "add your own" says where. */
  directory: string
  /** Keep the theme under the cursor: persist it, and close. */
  onKeep: (name: string) => void
  /** Close without keeping. The previous theme is put back first. */
  onClose: () => void
}

/** The four inks worth showing beside a name: finished, failed, running, and the session's own
 * voice. Four rather than nine because a row is a row, and these are the meaning-bearing ones. */
const SWATCHES = ['ok', 'fail', 'running', 'note'] as const

/**
 * Choosing what this window is painted in.
 *
 * A panel over the transcript rather than a sheet that covers it, because moving the cursor
 * repaints the window *behind* the panel and there would be no point previewing a theme onto
 * something nobody can see. The same shape the agent's own `/theme` draws, for the same reason.
 *
 * Moving previews. Enter keeps, and the name is remembered in `bravebot-ui.json` beside the column
 * widths. Escape puts back whatever was in force when this opened — including the case where that
 * was a palette somebody had just edited by hand, since the list is rebuilt from disk each time
 * this opens.
 *
 * Nothing here is labelled and nothing here is untrusted. The names are read off disk and drawn
 * for a person; they never reach a model, and choosing one is not an answer to anything the agent
 * asked. That is why this may be a menu item at all, which `src/renderer/commands.ts` explains at
 * length about the five things that may not.
 */
export function ThemePicker(props: Props): React.JSX.Element {
  const { themes, chosen, directory, onKeep, onClose } = props
  const opened = useRef(findTheme(themes, chosen) ?? themes[0])
  const [selected, setSelected] = useState(() => {
    const at = themes.findIndex((theme) => theme.name === (chosen === 'system' ? BRAVE : chosen))
    return at === -1 ? 0 : at
  })

  // Previewing is a DOM write and not a render: the transcript behind this panel must not be
  // rebuilt to change the colour of its background. `theme.ts` gives the argument.
  const preview = useCallback(
    (at: number) => {
      const theme = themes[at]
      if (theme) applyTheme(theme)
      setSelected(at)
    },
    [themes],
  )

  const cancel = useCallback(() => {
    if (opened.current) applyTheme(opened.current)
    onClose()
  }, [onClose])

  const keep = (): void => {
    const theme = themes[selected]
    if (!theme) return
    applyTheme(theme)
    onKeep(theme.name)
  }

  const selectedName = themes[selected]?.name ?? ''

  // Keep the live preview in step when Command highlights a different row (arrows / hover).
  // cmdk compares values case-insensitively and may pass a lowercased name.
  const onHighlight = (name: string): void => {
    const needle = name.toLowerCase()
    const at = themes.findIndex((theme) => theme.name.toLowerCase() === needle)
    if (at >= 0 && at !== selected) preview(at)
  }

  const dark = window.matchMedia('(prefers-color-scheme: dark)').matches

  // Drive scripts focus `.theme-list` then press ArrowDown — keep that surface focusable.
  useEffect(() => {
    document.querySelector<HTMLElement>('.theme-list')?.focus()
  }, [])

  return (
    <Dialog open onOpenChange={(next) => { if (!next) cancel() }}>
      <DialogContent
        className="modal theme-picker w-80 gap-0 sm:max-w-none"
        showCloseButton={false}
        // No scrim of its own, unlike the trust dialog. Darkening the window would defeat the
        // point of the thing: moving the cursor repaints what is *behind* this panel, and a
        // preview seen through forty percent black is a preview of the wrong colours. The
        // click-catcher stays, transparent, so that clicking away still cancels.
        overlayClassName="scrim bg-transparent supports-backdrop-filter:backdrop-blur-none"
      >
        <DialogTitle className="mb-2.5 text-[13px] font-semibold text-muted-foreground">Theme</DialogTitle>
        <Command
          shouldFilter={false}
          value={selectedName}
          onValueChange={onHighlight}
          className="bg-transparent p-0"
        >
          <CommandList className="theme-list max-h-[300px] rounded-md" tabIndex={0}>
            <CommandGroup>
              {themes.map((theme) => {
                const inks = roleVariables(theme, dark)
                const at = themes.findIndex((row) => row.name === theme.name)
                return (
                  <CommandItem
                    key={theme.name}
                    value={theme.name}
                    className={cn(
                      'theme-row cursor-pointer py-[5px] hover:bg-foreground/12',
                      // The row under the cursor is also the one cmdk has highlighted, so the
                      // chosen colours have to be said in both spellings or the list's own
                      // highlight paints over the one that means "this is the theme".
                      at === selected &&
                        'active bg-primary text-primary-foreground data-selected:bg-primary data-selected:text-primary-foreground',
                    )}
                    onSelect={() => {
                      applyTheme(theme)
                      onKeep(theme.name)
                    }}
                    onMouseDown={() => preview(at)}
                  >
                    <span className="theme-name flex-1 tabular-nums">{theme.name}</span>
                    {theme.name === opened.current?.name ? (
                      <span className="theme-current text-[10px] tracking-[0.04em] uppercase opacity-75">in use</span>
                    ) : null}
                    {/* The palette's own inks, so a name is not the only thing to go on for
                        twenty-two of them. */}
                    <span className="theme-swatches flex gap-[3px]" aria-hidden="true">
                      {SWATCHES.map((role) => (
                        <i key={role} className="size-2 rounded-[2px]" style={{ background: inks[`--role-${role}`] }} />
                      ))}
                    </span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
        <p className="theme-hint m-0 mt-2.5 text-[11px] text-muted-foreground/70">
          <Kbd>↑</Kbd>
          <Kbd>↓</Kbd> preview · <Kbd>⏎</Kbd> keep · <Kbd>esc</Kbd> cancel
        </p>
        <p className="theme-aside m-0 mt-1.5 text-[11px] text-muted-foreground/70">
          Add your own as JSON in <code className="font-mono text-[10px]">{directory}</code>.
        </p>
        <DialogFooter className="theme-actions mx-0 mb-0 flex-row justify-end gap-2 rounded-none border-0 bg-transparent p-0 pt-3">
          <Button type="button" variant="outline" onClick={cancel}>Cancel</Button>
          <Button type="button" onClick={keep}>Use theme</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
