import { useEffect, useState } from 'react'
import type { BotServers as Answer } from '../../shared/bots'
import { STANDING_LABEL, standing, type Connector, type ConnectorList } from '../../shared/connectors'
import { useDelayedFlag } from '../hooks'
import { Alert, Checkbox, ControlItem, Label, ProgressRing, SegmentedControl } from '../nala'
import { call, invokeMessage } from '../request'

/**
 * Which connected servers a bot may use, as the `mcpServers:` line of its definition says.
 *
 * "Every connected server" removes the line; "Only these" writes the ticked aliases, which may be
 * none. A name the file lists that no connector declares stays ticked, so saving keeps it.
 */
export function BotServers({ slug }: { slug: string }): React.JSX.Element {
  const [answer, setAnswer] = useState<Answer | null>(null)
  const [connectors, setConnectors] = useState<Connector[]>([])
  const [problem, setProblem] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const showLoading = useDelayedFlag(loading)

  useEffect(() => {
    let gone = false
    void Promise.all([
      window.bravebot.readBotServers(slug),
      call<ConnectorList>('connectors.list').then((list) => list.connectors, () => [] as Connector[]),
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
    setAnswer({ ...answer, servers })
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

  const readOnly = answer.inline || answer.kind !== 'worker'
  const every = answer.servers === null && !answer.tools
  const chosen = answer.servers ?? []
  const declared = connectors.map((each) => each.alias)
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
    {readOnly ? (
      <p className="bot-servers-note" data-test="bot-servers-readonly">
        {answer.inline
          ? 'This bot’s definition declares its own servers, so they are changed in the file rather than here.'
          : 'This definition is not one the picker rewrites, so its servers are changed in the file.'}
      </p>
    ) : (
      <SegmentedControl value={every ? 'every' : 'only'} size="small" aria-label="Which servers this bot may use" data-test="bot-servers-mode"
        onChange={(detail) => {
          if (saving) return
          if (detail.value === 'every' && !every && !answer.tools) void save(null)
          if (detail.value === 'only' && every) void save(declared)
        }}>
        <ControlItem value="every">Every connected server</ControlItem>
        <ControlItem value="only">Only these</ControlItem>
      </SegmentedControl>
    )}
    {answer.tools && answer.servers === null && (
      <p className="bot-servers-note" data-test="bot-servers-tools">
        This bot’s definition lists the tools it may use, so it gets only the servers ticked here.
      </p>
    )}
    {!every && !answer.inline && (
      names.length === 0 ? (
        <p className="bot-servers-note" data-test="bot-servers-none">No servers are set up yet. Add one in Settings, under Connectors.</p>
      ) : (
        <ul className="bot-servers-list">
          {names.map((name) => {
            const connector = connectors.find((each) => each.alias === name)
            const stands = standing(connector)
            return <li key={name} className="bot-server" data-test="bot-server" data-alias={name}>
              <Checkbox checked={chosen.includes(name)} isDisabled={readOnly || saving} size="small"
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
