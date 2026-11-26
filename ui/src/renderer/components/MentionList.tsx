import { Fragment } from 'react'
import type { MentionEntry } from '../../shared/mentions'
import type { SummonedBot } from '../staging'
import { Hr, Icon } from '../nala'
import { BotFace } from './BotAvatar'
import { FileGlyph } from './FileGlyph'

/**
 * What a half-typed `@` name could become, drawn above the message box (NAME-4).
 *
 * The person's bots come first, then the definitions the session resolves, then the project's files
 * (MEMORY-14, ADDRESS-2). Picking a bot or a definition stages it beside the draft to answer this
 * message; picking a file writes its name.
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
  const onDefinition = !onBot && entries[active - bots.length]?.kind === 'definition'
  const definitions = entries.filter((entry) => entry.kind === 'definition').length
  return (
    <div className="mention-list" data-test="mention-list">
      <div className="mention-rows" role="listbox" aria-label={bots.length || definitions ? 'Bots, definitions and project files' : 'Project files'}>
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
        {entries.map((entry, at) => {
          const index = bots.length + at
          // The bridge lists the definitions ahead of the files (ADDRESS-2).
          const heading = at === 0 && entry.kind === 'definition' && bots.length > 0
            ? 'Definitions'
            : at === definitions && entry.kind !== 'definition' && (bots.length > 0 || definitions > 0) ? 'Files' : null
          const section = heading && <div className="mention-section" role="presentation">{heading}</div>
          if (entry.kind === 'definition') {
            // A definition is staged to answer the message and never written as an `@` name, so
            // the row says which kind of thing it is (ADDRESS-2).
            return (
              <Fragment key={`definition:${entry.path}`}>
                {section}
                <div role="option" aria-selected={index === active} data-test="mention-definition"
                  className={index === active ? 'mention-option definition active' : 'mention-option definition'}
                  ref={index === active ? (row) => row?.scrollIntoView({ block: 'nearest' }) : undefined}
                  onMouseEnter={() => onActive(index)}
                  onMouseDown={(event) => { event.preventDefault(); onChoose(entry) }}>
                  <Icon name="product-brave-leo" />
                  <span className="mention-definition-name">{entry.shownPath}</span>
                  {entry.description && <span className="mention-description">{entry.description}</span>}
                  <span className="mention-kind">Definition</span>
                </div>
              </Fragment>
            )
          }
          // `shownPath` never carries a trailing slash, marked or not (the bridge takes it off
          // before marking), so the directory's own slash is only ever the one added back below,
          // outside of whatever quoting marking the name added.
          const name = entry.shownPath
          const slash = name.lastIndexOf('/')
          return (
            <Fragment key={entry.path}>
              {section}
              <div role="option" aria-selected={index === active} data-test="mention-option"
                className={index === active ? 'mention-option active' : 'mention-option'}
                ref={index === active ? (row) => row?.scrollIntoView({ block: 'nearest' }) : undefined}
                onMouseEnter={() => onActive(index)}
                onMouseDown={(event) => { event.preventDefault(); onChoose(entry) }}>
                {/* `entry.path` and never `name`: a marked file's `shownPath` is quoted, and an
                    extension read off the quote closing it is not the file's own. */}
                {entry.directory ? <Icon name="folder" /> : <FileGlyph name={entry.path} />}
                <span className="mention-path">
                  {slash >= 0 && <span className="mention-parent">{name.slice(0, slash + 1)}</span>}
                  <span className="mention-name">{name.slice(slash + 1)}{entry.directory ? '/' : ''}</span>
                </span>
              </div>
            </Fragment>
          )
        })}
      </div>
      <Hr />
      <div className={onBot || onDefinition ? 'mention-foot summon' : 'mention-foot'}>
        {onBot ? (
          <>
            <Icon name="message-bubble" />
            <span>The bot answers this message in this chat</span>
          </>
        ) : onDefinition ? (
          <>
            <Icon name="message-bubble" />
            <span>The definition answers this message alone</span>
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
