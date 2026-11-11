import { useEffect, useState } from 'react'
import type { Bot } from '../../shared/bots'
import type { RetrievalStatus } from '../../shared/protocol'
import { t } from '../i18n'
import { ProgressBar } from '../nala'
import { building, recallLine } from '../recall-status'

const POLL_MS = 2000

/**
 * How far a bot's conversation can be recalled from. Hidden while the setting is off, and asked
 * again while the index builds and whenever a turn of the bot's ends.
 */
export function BotRecall({ bot, fellBack, running }: { bot: Bot; fellBack: string | null; running: boolean }): React.JSX.Element | null {
  const [status, setStatus] = useState<RetrievalStatus | null>(null)
  const id = bot.session
  const directory = bot.conversations.find((each) => each.id === id)?.directory ?? bot.home
  useEffect(() => {
    setStatus(null)
    if (!id) return
    let gone = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const ask = async (): Promise<void> => {
      const answer = await window.bravebot.request<RetrievalStatus>('retrieval.status', { directory, id }).catch(() => null)
      if (gone) return
      const next = answer?.ok ?? null
      setStatus(next)
      if (building(next)) timer = setTimeout(() => void ask(), POLL_MS)
    }
    if (!running) void ask()
    return () => { gone = true; clearTimeout(timer) }
  }, [id, directory, running])
  const line = recallLine(status, fellBack)
  if (!line) return null
  return <section className="bot-recall" data-test="bot-recall" aria-labelledby={`recall-${bot.slug}`}>
    <span className="bot-field-label" id={`recall-${bot.slug}`}>{t.uiRecallHeading()}</span>
    <span className="bot-recall-line" data-test="bot-recall-line">{line.text}</span>
    {line.progress !== undefined && <ProgressBar progress={line.progress} />}
    {line.until && <span className="bot-recall-until">{line.until}</span>}
  </section>
}
