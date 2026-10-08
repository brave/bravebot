import type { MentionEntry } from '../../shared/mentions'
import { Hr, Icon } from '../nala'
import { FileGlyph } from './FileGlyph'

/**
 * What a half-typed `@` name could become, drawn above the message box (NAME-4).
 *
 * The field keeps focus the whole time: the keys that walk this list are handled by the composer,
 * and a press on a row is taken without moving focus, so typing carries on where it was.
 */
export function MentionList({ entries, active, onActive, onChoose }: {
  entries: MentionEntry[]
  active: number
  onActive: (index: number) => void
  onChoose: (entry: MentionEntry) => void
}): React.JSX.Element {
  return (
    <div className="mention-list" data-test="mention-list">
      <div className="mention-rows" role="listbox" aria-label="Project files">
        {entries.map((entry, index) => {
          const name = entry.path.replace(/\/$/, '')
          const slash = name.lastIndexOf('/')
          return (
            <div key={entry.path} role="option" aria-selected={index === active} data-test="mention-option"
              className={index === active ? 'mention-option active' : 'mention-option'}
              ref={index === active ? (row) => row?.scrollIntoView({ block: 'nearest' }) : undefined}
              onMouseEnter={() => onActive(index)}
              onMouseDown={(event) => { event.preventDefault(); onChoose(entry) }}>
              {entry.directory ? <Icon name="folder" /> : <FileGlyph name={name} />}
              <span className="mention-path">
                {slash >= 0 && <span className="mention-parent">{name.slice(0, slash + 1)}</span>}
                <span className="mention-name">{name.slice(slash + 1)}{entry.directory ? '/' : ''}</span>
              </span>
            </div>
          )
        })}
      </div>
      <Hr />
      <div className="mention-foot">
        <Icon name="warning-triangle-outline" />
        <span>Named files are sent as trusted context</span>
        <span className="mention-keys">Tab to complete</span>
      </div>
    </div>
  )
}
