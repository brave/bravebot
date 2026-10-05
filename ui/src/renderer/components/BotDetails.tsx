import { useEffect, useRef, useState } from 'react'
import { newAvatarSeed } from '../../shared/avatar'
import { botFolders, type Bot } from '../../shared/bots'
import { projectLabel } from '../../shared/recents'
import { useFitTextArea } from '../hooks'
import { Dropdown, Input, TextArea } from '../nala'
import { BotAvatar } from './BotAvatar'
import { BotMemory } from './BotMemory'
import type { BotFormValue } from './Bots'
import { IconButton } from './IconButton'
import { IconMenu } from './IconMenu'

/**
 * A bot's details in the right column: its face, its name and purpose, and its memory.
 *
 * Name and purpose are saved when a field is left. A field left empty goes back to what is stored. The
 * memory is the one kept in a folder this bot has worked in; its home folder first.
 */
export function BotDetails({ bot, onSave, onArchive }: {
  bot: Bot
  onSave: (bot: BotFormValue) => Promise<boolean>
  onArchive: () => void
}): React.JSX.Element {
  const [name, setName] = useState(bot.name)
  const [purpose, setPurpose] = useState(bot.purpose)
  const [folder, setFolder] = useState(bot.home)
  // One field at a time, so saving one does not overwrite what is being typed in the other.
  useEffect(() => { setName(bot.name) }, [bot.slug, bot.name])
  useEffect(() => { setPurpose(bot.purpose) }, [bot.slug, bot.purpose])
  useEffect(() => { setFolder(bot.home) }, [bot.slug, bot.home])
  const folders = botFolders(bot)
  const purposeField = useRef<HTMLElement>(null)
  useFitTextArea(purposeField, purpose, 3, 8)

  const commit = (next: { name?: string; purpose?: string }) => {
    const value = { name: (next.name ?? name).trim(), purpose: (next.purpose ?? purpose).trim() }
    // An emptied field is not saved, and shows what is stored again rather than a blank.
    if (!value.name) setName(bot.name)
    if (!value.purpose) setPurpose(bot.purpose)
    if (!value.name || !value.purpose) return
    // Only what changed is sent, so this save cannot put back a field another one is changing.
    const edit: BotFormValue = { slug: bot.slug }
    if (value.name !== bot.name) edit.name = value.name
    if (value.purpose !== bot.purpose) edit.purpose = value.purpose
    if (edit.name === undefined && edit.purpose === undefined) return
    void onSave(edit)
  }

  return (
    <div className="bot-details" data-test="bot-details">
      <div className="context-head">
        <div className="inspector-title">
          <strong>Bot details</strong>
          <IconMenu icon="more-vertical" label={`Actions for ${bot.name}`} tooltip={false} className="bot-details-more">
            <leo-menu-item onClick={onArchive}>
              <span className="menu-icon-row">Archive bot</span>
            </leo-menu-item>
          </IconMenu>
        </div>
      </div>
      <div className="bot-details-body">
        <div className="bot-details-face">
          <BotAvatar seed={bot.avatar} size={80} doing="open" />
          <IconButton icon="refresh" label="New face" tooltip="Try a new face" kind="filled" size="tiny"
            className="bot-avatar-refresh"
            onClick={() => void onSave({ slug: bot.slug, avatar: newAvatarSeed(crypto.randomUUID()) })} />
        </div>
        <div className="bot-field">
          <Input value={name} onInput={({ value }) => setName(value)} onChange={({ value }) => { setName(value); commit({ name: value }) }}>
            Bot name
          </Input>
        </div>
        <div className="bot-field">
          <TextArea ref={purposeField} value={purpose} minRows={3} maxRows={8} onInput={({ value }) => setPurpose(value)}
            onChange={({ value }) => { setPurpose(value); commit({ purpose: value }) }}>
            Purpose
          </TextArea>
        </div>
        {folders.length > 1 && (
          <Dropdown value={folder} size="small" data-test="memory-folder"
            onChange={(detail) => { if (typeof detail.value === 'string' && folders.includes(detail.value)) setFolder(detail.value) }}>
            <span slot="label">Memory for</span>
            {folders.map((each) => (
              <leo-option key={each} value={each}>{each === bot.home ? 'No project' : projectLabel(each)}</leo-option>
            ))}
          </Dropdown>
        )}
        <BotMemory key={`${bot.slug}:${folder}`} slug={bot.slug} directory={folder} />
      </div>
    </div>
  )
}
