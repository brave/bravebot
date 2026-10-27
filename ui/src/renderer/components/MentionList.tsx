import type { MentionEntry } from '../../shared/mentions'
import type { SummonedBot } from '../staging'
import { Hr, Icon } from '../nala'
import { BotFace } from './BotAvatar'
import { FileGlyph } from './FileGlyph'

/**
 * What a half-typed `@` name could become, drawn above the message box (NAME-4).
 *
 * The person's bots come first and the project's files after them (MEMORY-13). Picking a bot stages
 * it beside the draft to answer this message; picking a file writes its name.
 *
 * The field keeps focus the whole time: the keys that walk this list are handled by the composer,
 * and a press on a row is taken without moving focus, so typing carries on where it was.
 */
export function MentionList({ bots, entries, active, onActive, onChoose, onSummon }: {
  bots: SummonedBot[]
  entries: MentionEntry[]
  active: number
  onActive: (index: number) => void
  onChoose: (entry: MentionEntry) => void
  onSummon: (bot: SummonedBot) => void
}): React.JSX.Element {
  const onBot = active < bots.length
  return (
    <div className="mention-list" data-test="mention-list">
      <div className="mention-rows" role="listbox" aria-label={bots.length ? 'Bots and project files' : 'Project files'}>
        {bots.length > 0 && <div className="mention-section" role="presentation">Bots</div>}
        {bots.map((bot, index) => (
          <div key={`bot:${bot.slug}`} role="option" aria-selected={index === active} data-test="mention-bot"
            className={index === active ? 'mention-option bot active' : 'mention-option bot'}
            ref={index === active ? (row) => row?.scrollIntoView({ block: 'nearest' }) : undefined}
            onMouseEnter={() => onActive(index)}
            onMouseDown={(event) => { event.preventDefault(); onSummon(bot) }}>
            <BotFace seed={bot.avatar} size={16} />
            <span className="mention-bot-name">{bot.name}</span>
          </div>
        ))}
        {bots.length > 0 && entries.length > 0 && <div className="mention-section" role="presentation">Files</div>}
        {entries.map((entry, at) => {
          const index = bots.length + at
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
      <div className={onBot ? 'mention-foot summon' : 'mention-foot'}>
        {onBot ? (
          <>
            <Icon name="message-bubble" />
            <span>The bot answers this message in this chat</span>
          </>
        ) : (
          <>
            <Icon name="warning-triangle-outline" />
            <span>Named files are sent as trusted context</span>
          </>
        )}
        <span className="mention-keys">Tab to complete</span>
      </div>
    </div>
  )
}
