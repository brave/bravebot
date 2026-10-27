import { useEffect, useState } from 'react'
import type { BotServers as Answer } from '../../shared/bots'
import { STANDING_LABEL, standing, type Connector, type ConnectorList } from '../../shared/connectors'
import { useDelayedFlag } from '../hooks'
import { Alert, Checkbox, Dropdown, Label, ProgressRing } from '../nala'
import { call, invokeMessage } from '../request'

/**
 * Which connected servers a bot may use, as the `mcpServers:` line of its definition says.
 *
 * "Every connected server" removes the line; "Only these" writes the ticked aliases, which may be
 * none. A name the file lists that no connector declares stays ticked, so saving keeps it. What the
 * line means and whether it may be rewritten are the agent's answers, never worked out here.
 */
export function BotServers({ slug }: { slug: string }): React.JSX.Element {
  const [answer, setAnswer] = useState<Answer | null>(null)
  const [connectors, setConnectors] = useState<Connector[] | null>(null)
  const [problem, setProblem] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const showLoading = useDelayedFlag(loading)

  useEffect(() => {
    let gone = false
    void Promise.all([
      window.bravebot.readBotServers(slug),
      call<ConnectorList>('connectors.list').then((list) => list.connectors, () => null),
    ]).then(([read, declared]) => {
      if (gone) return
      setAnswer(read)
      setConnectors(declared)
      if (!read) setProblem('This bot’s servers could not be read.')
    }).catch((error) => { if (!gone) setProblem(invokeMessage(error)) })
      .finally(() => { if (!gone) setLoading(false) })
    return () => { gone = true }
  }, [slug])

  const save = async (servers: string[] | null) => {
    if (!answer) return
    const before = answer
    setAnswer({ ...answer, servers, every: servers === null && answer.everyPossible })
    setProblem('')
    setSaving(true)
    try {
      const saved = await window.bravebot.writeBotServers(slug, servers)
      if (!saved) throw new Error('The agent did not save this bot’s servers.')
      setAnswer(saved)
    } catch (error) {
      setAnswer(before)
      setProblem(invokeMessage(error))
    } finally { setSaving(false) }
  }

  const heading = <span className="bot-field-label" id={`servers-${slug}`}>Servers</span>
  if (!answer) {
    return <section className="bot-servers" data-test="bot-servers" aria-labelledby={`servers-${slug}`}>
      {heading}
      {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
      {loading && <p role="status" className="bot-servers-loading" data-delayed={!showLoading || undefined}><ProgressRing mode="indeterminate" /> Loading…</p>}
    </section>
  }

  // Without the list, "Only these" would offer nothing to tick and save a line that selects none.
  const unlisted = connectors === null
  const editable = answer.editable && !unlisted
  const chosen = answer.servers ?? []
  const declared = (connectors ?? []).map((each) => each.alias)
  const names = [...declared, ...chosen.filter((name) => !declared.includes(name))]
  const pick = (alias: string, on: boolean) => {
    // Leo reports a `checked` the window set as well as a click, so only a change is saved.
    if (saving || on === chosen.includes(alias)) return
    const next = on ? [...chosen, alias] : chosen.filter((name) => name !== alias)
    void save(names.filter((name) => next.includes(name)))
  }

  return <section className="bot-servers" data-test="bot-servers" aria-labelledby={`servers-${slug}`}>
    {heading}
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    {!answer.editable && (
      <p className="bot-servers-note" data-test="bot-servers-readonly">
        {answer.inline
          ? 'This bot’s definition declares its own servers, so they are changed in the file rather than here.'
          : 'This definition is not one the picker rewrites, so its servers are changed in the file.'}
      </p>
    )}
    {answer.editable && unlisted && (
      <Alert type="warning" size="small" data-test="bot-servers-unlisted">
        The connected servers could not be listed, so this bot’s servers cannot be changed right now.
      </Alert>
    )}
    {editable && answer.everyPossible && (
      <Dropdown value={answer.every ? 'every' : 'only'} size="small" disabled={saving} aria-label="Which servers this bot may use" data-test="bot-servers-mode"
        onChange={(detail) => {
          if (saving) return
          if (detail.value === 'every' && !answer.every) void save(null)
          if (detail.value === 'only' && answer.every) void save(declared)
        }}>
        <leo-option value="every">Every connected server</leo-option>
        <leo-option value="only">Only these</leo-option>
      </Dropdown>
    )}
    {editable && !answer.everyPossible && (
      <p className="bot-servers-note" data-test="bot-servers-tools">
        This bot’s definition lists the tools it may use, so it gets only the servers ticked here.
      </p>
    )}
    {!answer.every && !answer.inline && !unlisted && (
      names.length === 0 ? (
        <p className="bot-servers-note" data-test="bot-servers-none">No servers are set up yet. Add one in Settings, under Connectors.</p>
      ) : (
        <ul className="bot-servers-list">
          {names.map((name) => {
            const connector = connectors?.find((each) => each.alias === name)
            const stands = standing(connector)
            return <li key={name} className="bot-server" data-test="bot-server" data-alias={name}>
              <Checkbox checked={chosen.includes(name)} isDisabled={!editable || saving} size="small"
                onChange={({ checked }) => pick(name, checked)}>
                {name}
              </Checkbox>
              <Label mode={stands === 'connected' ? 'loud' : 'outline'} color={stands === 'connected' ? 'green' : stands === 'attention' ? 'yellow' : 'neutral'}>
                {STANDING_LABEL[stands]}
              </Label>
            </li>
          })}
        </ul>
      )
    )}
    <small className="bot-servers-note">Applies from this bot’s next turn, in open conversations too.</small>
  </section>
}
