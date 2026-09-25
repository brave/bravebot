import { Modal } from './Modal'
import { useCallback, useRef, useState } from 'react'
import { BRAVE, findTheme, roleVariables, type Theme } from '../../shared/theme'
import { applyTheme } from '../theme'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from './ui/command'
import { DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
import { Kbd, KbdGroup } from './ui/kbd'

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
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(() => {
    const at = themes.findIndex((theme) => theme.name === (chosen === 'system' ? BRAVE : chosen))
    return themes[at === -1 ? 0 : at]?.name ?? BRAVE
  })

  // Previewing is a DOM write and not a render: the transcript behind this panel must not be
  // rebuilt to change the colour of its background. `theme.ts` gives the argument.
  const preview = useCallback(
    (name: string) => {
      const theme = themes.find((candidate) => candidate.name === name)
      if (theme) applyTheme(theme)
      setSelected(name)
    },
    [themes],
  )

  const cancel = useCallback(() => {
    if (opened.current) applyTheme(opened.current)
    onClose()
  }, [onClose])

  const keep = (): void => {
    const theme = themes.find((candidate) => candidate.name === selected)
    if (!theme) return
    applyTheme(theme)
    onKeep(theme.name)
  }

  const onKeyDown = (event: React.KeyboardEvent): void => {
    if (event.key === 'Enter') {
      event.preventDefault()
      event.stopPropagation()
      keep()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      cancel()
    }
  }

  const dark = window.matchMedia('(prefers-color-scheme: dark)').matches

  return (
    <Modal title="Theme" className="theme-picker" onClose={cancel}>
        <DialogHeader>
          <DialogTitle id="theme-title">Theme</DialogTitle>
          <DialogDescription>Preview a palette, then keep it for this app.</DialogDescription>
        </DialogHeader>
        <Command shouldFilter={false} value={selected} onValueChange={preview} onKeyDownCapture={onKeyDown}>
          <CommandInput autoFocus placeholder="Search themes…" aria-label="Search themes" value={query} onValueChange={setQuery} />
          <CommandList className="theme-list" aria-label="Themes">
            <CommandEmpty>No themes match your search.</CommandEmpty>
            <CommandGroup>
              {themes.filter((theme) => theme.name.toLowerCase().includes(query.toLowerCase().trim())).map((theme, at) => {
                const inks = roleVariables(theme, dark)
                return (
                  <CommandItem
                    key={theme.name}
                    id={`theme-${at}`}
                    value={theme.name}
                    className={`theme-row flex items-center gap-2${theme.name === selected ? ' active' : ''}`}
                    onDoubleClick={keep}
                  >
                    <span className="theme-name">{theme.name}</span>
                    {theme.name === opened.current?.name ? (
                      <Badge variant="secondary" className="theme-current">in use</Badge>
                    ) : null}
                    <span className="theme-swatches ml-auto flex gap-1" aria-hidden="true">
                      {SWATCHES.map((role) => (
                        <i key={role} className="inline-block size-2.5 rounded-full" style={{ background: inks[`--role-${role}`] }} />
                      ))}
                    </span>
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </CommandList>
        </Command>
        <p className="theme-hint text-xs text-muted-foreground">
          <KbdGroup><Kbd>↑</Kbd><Kbd>↓</Kbd></KbdGroup> preview · <Kbd>⏎</Kbd> keep · <Kbd>esc</Kbd> cancel
        </p>
        <p className="theme-aside text-xs text-muted-foreground">
          Add your own as JSON in <code>{directory}</code>.
        </p>
      <DialogFooter className="theme-actions"><Button variant="outline" onClick={cancel}>Cancel</Button><Button onClick={keep}>Use theme</Button></DialogFooter>
    </Modal>
  )
}
