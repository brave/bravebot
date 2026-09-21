import { Modal } from './Modal'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { BRAVE, findTheme, roleVariables, type Theme } from '../../shared/theme'
import { applyTheme } from '../theme'

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
  const list = useRef<HTMLDivElement>(null)
  const rows = useRef<(HTMLDivElement | null)[]>([])
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

  useLayoutEffect(() => {
    list.current?.focus()
  }, [])

  // Keeping the cursor on screen, which matters here more than in a short menu: there are
  // twenty-two built-ins before anybody has written one of their own.
  useEffect(() => {
    rows.current[selected]?.scrollIntoView({ block: 'nearest' })
  }, [selected])

  const keep = (): void => {
    const theme = themes[selected]
    if (!theme) return
    applyTheme(theme)
    onKeep(theme.name)
  }

  const onKeyDown = (event: React.KeyboardEvent): void => {
    const last = themes.length - 1
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      preview(Math.min(selected + 1, last))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      preview(Math.max(selected - 1, 0))
    } else if (event.key === 'Home') {
      event.preventDefault()
      preview(0)
    } else if (event.key === 'End') {
      event.preventDefault()
      preview(last)
    } else if (event.key === 'Enter') {
      event.preventDefault()
      keep()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      cancel()
    }
  }

  const dark = window.matchMedia('(prefers-color-scheme: dark)').matches

  return (
    <Modal title="Theme" className="theme-picker" onClose={cancel}>
        <h2 id="theme-title">Theme</h2>
        <div
          className="theme-list"
          role="listbox"
          aria-activedescendant={`theme-${selected}`}
          tabIndex={0}
          ref={list}
          onKeyDown={onKeyDown}
        >
          {themes.map((theme, at) => {
            const inks = roleVariables(theme, dark)
            return (
              <div
                key={theme.name}
                id={`theme-${at}`}
                ref={(node) => {
                  rows.current[at] = node
                }}
                className={`theme-row${at === selected ? ' active' : ''}`}
                role="option"
                aria-selected={at === selected}
                onMouseDown={() => preview(at)}
                onDoubleClick={keep}
              >
                <span className="theme-name">{theme.name}</span>
                {theme.name === opened.current?.name ? (
                  <span className="theme-current">in use</span>
                ) : null}
                <span className="theme-swatches" aria-hidden="true">
                  {SWATCHES.map((role) => (
                    <i key={role} style={{ background: inks[`--role-${role}`] }} />
                  ))}
                </span>
              </div>
            )
          })}
        </div>
        <p className="theme-hint">
          <kbd>↑</kbd>
          <kbd>↓</kbd> preview · <kbd>⏎</kbd> keep · <kbd>esc</kbd> cancel
        </p>
        <p className="theme-aside">
          Add your own as JSON in <code>{directory}</code>.
        </p>
      <div className="theme-actions"><button onClick={cancel}>Cancel</button><button onClick={keep}>Use theme</button></div>
    </Modal>
  )
}
