import { useEffect, useRef, useState } from 'react'
import {
  parseJob, parsePluginList, parsePluginQuestion, parsePluginsEnded,
  type PluginList, type PluginOutcome, type PluginQuestion as Question, type PluginRow,
} from '../../shared/plugins'
import type { BridgeEvent } from '../../shared/protocol'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, Input, Label, Toggle } from '../nala'
import { call } from '../request'
import { PluginQuestion } from './PluginQuestion'
import { SettingsGroup } from './SettingsGroup'
import { Skeleton } from './Skeleton'

/**
 * The plugins page of the settings: what is installed, with each plugin's record, a switch that
 * turns it on or off, Update and Remove buttons, and a form that installs one (docs/specs/plugins.md
 * PLUGIN-1, PLUGIN-4, PLUGIN-9, PLUGIN-11).
 *
 * The bridge does every check and puts every question; this page sends what the person typed or
 * picked, draws each `plugins.question` and answers it. Only the yes button of a question sends
 * `yes: true`. Esc, closing it, the no button and leaving the page each answer no, and leaving the
 * page while a job runs cancels it. Every field but `enabled` is text the plugin or the person
 * wrote, drawn as plain text.
 */
export function Plugins(): React.JSX.Element {
  const [list, setList] = useState<PluginList | null>(null)
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState<{ tone: 'success' | 'info' | 'error'; text: string } | null>(null)
  const [busy, setBusy] = useState(false)
  const [source, setSource] = useState('')
  /** The job this page started and is waiting on, or `null`. */
  const [job, setJob] = useState<string | null>(null)
  const [question, setQuestion] = useState<Question | null>(null)
  const showLoading = useDelayedFlag(busy && !list)

  // What leaving the page answers, read at unmount.
  const open = useRef<{ job: string | null; question: Question | null }>({ job: null, question: null })
  open.current = { job, question }
  // The jobs that have ended. The bridge starts a job before it answers the call that started it,
  // so `plugins.ended` can arrive before that answer, and `started` must not wait on it then.
  const ended = useRef(new Set<string>())

  /** Run one bridge call and draw the list it answers with. */
  const run = async (method: string, params: Record<string, unknown>, done?: string) => {
    setBusy(true)
    setProblem('')
    setStatus(null)
    try {
      const parsed = parsePluginList(await call<unknown>(method, params))
      if (!parsed) throw new Error('This agent answered with a plugin list this window cannot read. Rebuild or reinstall the app, then retry.')
      setList(parsed)
      if (done) setStatus({ tone: 'success', text: done })
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }
  const load = () => run('plugins.list', {})

  useEffect(() => {
    void load()
    const stop = window.bravebot.onEvent((message: BridgeEvent) => {
      if (message.event === 'plugins.question') {
        const parsed = parsePluginQuestion(message.data)
        if (parsed) {
          setJob(parsed.job)
          setQuestion(parsed)
          return
        }
        // A question this window cannot draw whole is answered no, never left open.
        const raw = message.data as { job?: unknown; question?: unknown } | null
        if (raw && typeof raw.job === 'string' && typeof raw.question === 'number') {
          void call('plugins.reply', { job: raw.job, question: raw.question, yes: false }).catch(() => undefined)
        }
        setProblem('This agent asked a plugin question this window cannot read, so it was answered no. Rebuild or reinstall the app, then retry.')
      } else if (message.event === 'plugins.ended') {
        const done = parsePluginsEnded(message.data)
        if (!done) return
        ended.current.add(done.job)
        setQuestion(null)
        setJob(null)
        setList((old) => (old ? { ...old, plugins: done.plugins } : old))
        setStatus({ tone: TONE[done.outcome], text: done.sentence })
        if (done.outcome === 'installed') setSource('')
      }
    })
    return () => {
      stop()
      // Leaving the page answers no, and stops a job that has not asked yet.
      const { job: running, question: asked } = open.current
      if (asked) void call('plugins.reply', { job: asked.job, question: asked.question, yes: false }).catch(() => undefined)
      else if (running) void call('plugins.cancel', { job: running }).catch(() => undefined)
    }
  }, [])

  /** Start a job from the answer to `plugins.add` or `plugins.update`. */
  const started = (answer: unknown, waiting: string) => {
    const id = parseJob(answer)
    if (!id) throw new Error('This agent answered with a job this window cannot read. Rebuild or reinstall the app, then retry.')
    if (ended.current.has(id)) return
    setJob(id)
    setStatus({ tone: 'info', text: waiting })
  }
  const begin = async (method: 'plugins.add' | 'plugins.update', params: Record<string, unknown>, waiting: string) => {
    setProblem('')
    setStatus(null)
    try {
      started(await call<unknown>(method, params), waiting)
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    }
  }
  const add = () => {
    const typed = source.trim()
    if (typed) void begin('plugins.add', { source: typed }, `Reading ${typed}…`)
  }
  const chooseFolder = async () => {
    setProblem('')
    setStatus(null)
    try {
      const answer = await window.bravebot.choosePluginFolder()
      if (answer === null) return
      if ('error' in answer) throw new Error(answer.error.message)
      started(answer.ok, 'Reading the folder…')
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    }
  }
  const update = (plugin: PluginRow) => void begin('plugins.update', { name: plugin.name }, `Reading ${plugin.source}…`)
  const answer = (yes: boolean) => {
    const asked = question
    if (!asked) return
    setQuestion(null)
    void call('plugins.reply', { job: asked.job, question: asked.question, yes }).catch((error: unknown) => {
      setProblem(error instanceof Error ? error.message : String(error))
    })
  }

  const toggle = (plugin: PluginRow, enabled: boolean) => run(enabled ? 'plugins.enable' : 'plugins.disable', { name: plugin.name },
    enabled ? `${plugin.name} is on. New conversations offer what it holds.` : `${plugin.name} is off. It stays installed, and new conversations offer nothing from it.`)
  const remove = (plugin: PluginRow) => {
    if (!window.confirm(`Remove ${plugin.name}? Its directory and its record are deleted.`)) return
    void run('plugins.remove', { name: plugin.name }, `${plugin.name} was removed.`)
  }

  const locked = !!list && !list.writable
  const working = busy || job !== null
  return (
    <div className="settings-body plugins" data-test="plugins">
      {(problem || status || locked) && (
        <div className="plugins-alerts">
          {problem && (
            <div className="settings-error">
              <Alert type="error" role="alert" data-test="plugins-error">{problem}</Alert>
              {!list && <Button size="small" kind="outline" isDisabled={busy} onClick={() => void load()}>Retry</Button>}
            </div>
          )}
          {status && <Alert type={status.tone} size="small" role="status" data-test="plugins-status">{status.text}</Alert>}
          {locked && <Alert type="warning" role="alert">Nothing is being written in this session, so plugins cannot be changed.</Alert>}
        </div>
      )}
      {busy && !list && <Skeleton label="Loading plugins…" kind="rows" count={2} delayed={!showLoading} />}
      {list && (
        <SettingsGroup title="Installed">
          {list.plugins.length === 0 ? (
            <div className="settings-block">
              <p className="plugins-empty" data-test="plugins-empty">No plugins are installed. Add one below.</p>
            </div>
          ) : (
            <ul className="plugin-rows" data-test="plugins-list">
              {list.plugins.map((plugin) => (
                <li key={plugin.name} className="settings-block plugin-row" data-test={`plugin-${plugin.name}`}>
                  <div className="plugin-head">
                    <span className="plugin-name" data-test="plugin-name">{plugin.name}</span>
                    <Label mode="outline" color="neutral" className="plugin-version" data-test="plugin-version">{plugin.version}</Label>
                    <span className="plugin-actions">
                      <Toggle size="small" checked={plugin.enabled} disabled={working || locked}
                        aria-label={`Use ${plugin.name}`} data-test="plugin-enabled"
                        onChange={({ checked }) => { if (checked !== plugin.enabled) void toggle(plugin, checked) }} />
                      <Button size="small" kind="outline" isDisabled={working || locked} onClick={() => update(plugin)} data-test="plugin-update">Update</Button>
                      <Button size="small" kind="plain-faint" isDisabled={working || locked} onClick={() => remove(plugin)} data-test="plugin-remove">Remove</Button>
                    </span>
                  </div>
                  <dl className="plugin-record">
                    <dt>Source</dt><dd><code data-test="plugin-source">{plugin.source}</code></dd>
                    {plugin.requestedRef !== null && <><dt>Ref</dt><dd><code data-test="plugin-ref">{plugin.requestedRef}</code></dd></>}
                    {plugin.commit !== null && <><dt>Commit</dt><dd><code data-test="plugin-commit">{plugin.commit}</code></dd></>}
                    <dt>Digest</dt><dd><code data-test="plugin-digest">{plugin.digest}</code></dd>
                  </dl>
                </li>
              ))}
            </ul>
          )}
        </SettingsGroup>
      )}
      {list && (
        <SettingsGroup title="Install a plugin">
          <form className="settings-block plugin-add" data-test="plugin-add"
            onSubmit={(event) => { event.preventDefault(); if (!working && !locked) add() }}>
            <Input value={source} placeholder="/path/to/plugin or https://example.com/plugin.git#v1.0" disabled={working || locked}
              data-test="plugin-add-source" onInput={(event) => { const value = fieldText(event); if (value !== null) setSource(value) }}>
              Source
            </Input>
            <p className="plugin-add-help">
              The full path of a folder on this computer, or a git address with an optional <code>#ref</code>. Everything the
              plugin holds is shown to you before anything is installed.
            </p>
            <div className="plugin-add-actions">
              <Button size="small" kind="outline" isDisabled={working || locked} onClick={() => void chooseFolder()} data-test="plugin-choose-folder">Choose folder…</Button>
              <Button size="small" kind="filled" isDisabled={working || locked || source.trim() === ''} onClick={add} data-test="plugin-add-submit">Install</Button>
            </div>
          </form>
        </SettingsGroup>
      )}
      {list && (
        <p className="plugins-foot">
          A plugin that is on offers its skills, agent definitions, hooks and servers to new conversations. Plugins are kept
          in <code>{list.state}/plugins</code>, the same place the terminal&rsquo;s <code>/plugin</code> writes.
        </p>
      )}
      {question && <PluginQuestion key={`${question.job}.${question.question}`} question={question} onAnswer={answer} />}
    </div>
  )
}

const TONE: Record<PluginOutcome, 'success' | 'info' | 'error'> = {
  installed: 'success', updated: 'success', unchanged: 'info', declined: 'info', nobody: 'info', refused: 'error',
}

const fieldText = (event: { value?: unknown; target?: EventTarget | null }): string | null => {
  if (typeof event.value === 'string') return event.value
  const target = event.target
  if (target && typeof target === 'object' && 'value' in target && typeof target.value === 'string') return target.value
  return null
}
