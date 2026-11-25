import { useEffect, useState } from 'react'
import { parsePluginList, type PluginList, type PluginRow } from '../../shared/plugins'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, Label, Toggle } from '../nala'
import { call } from '../request'
import { SettingsGroup } from './SettingsGroup'
import { Skeleton } from './Skeleton'

/**
 * The plugins page of the settings: what is installed, with each plugin's record, a switch that
 * turns it on or off and a button that removes it (docs/specs/plugins.md PLUGIN-11).
 *
 * The bridge does every check; this page sends the name and draws the list the bridge answers
 * with. Every field but `enabled` is text the plugin or the person wrote, drawn as plain text.
 */
export function Plugins(): React.JSX.Element {
  const [list, setList] = useState<PluginList | null>(null)
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)
  const showLoading = useDelayedFlag(busy && !list)

  /** Run one bridge call and draw the list it answers with. */
  const run = async (method: string, params: Record<string, unknown>, done?: string) => {
    setBusy(true)
    setProblem('')
    setStatus('')
    try {
      const parsed = parsePluginList(await call<unknown>(method, params))
      if (!parsed) throw new Error('This agent answered with a plugin list this window cannot read. Rebuild or reinstall the app, then retry.')
      setList(parsed)
      if (done) setStatus(done)
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }
  const load = () => run('plugins.list', {})
  useEffect(() => { void load() }, [])

  const toggle = (plugin: PluginRow, enabled: boolean) => run(enabled ? 'plugins.enable' : 'plugins.disable', { name: plugin.name },
    enabled ? `${plugin.name} is on. New conversations offer what it holds.` : `${plugin.name} is off. It stays installed, and new conversations offer nothing from it.`)
  const remove = (plugin: PluginRow) => {
    if (!window.confirm(`Remove ${plugin.name}? Its directory and its record are deleted.`)) return
    void run('plugins.remove', { name: plugin.name }, `${plugin.name} was removed.`)
  }

  const locked = !!list && !list.writable
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
          {status && <Alert type="success" size="small" role="status" data-test="plugins-status">{status}</Alert>}
          {locked && <Alert type="warning" role="alert">Nothing is being written in this session, so plugins cannot be changed.</Alert>}
        </div>
      )}
      {busy && !list && <Skeleton label="Loading plugins…" kind="rows" count={2} delayed={!showLoading} />}
      {list && (
        <SettingsGroup title="Installed">
          {list.plugins.length === 0 ? (
            <div className="settings-block">
              <p className="plugins-empty" data-test="plugins-empty">No plugins are installed. Install one in the terminal with <code>/plugin add</code>.</p>
            </div>
          ) : (
            <ul className="plugin-rows" data-test="plugins-list">
              {list.plugins.map((plugin) => (
                <li key={plugin.name} className="settings-block plugin-row" data-test={`plugin-${plugin.name}`}>
                  <div className="plugin-head">
                    <span className="plugin-name" data-test="plugin-name">{plugin.name}</span>
                    <Label mode="outline" color="neutral" className="plugin-version" data-test="plugin-version">{plugin.version}</Label>
                    <span className="plugin-actions">
                      <Toggle size="small" checked={plugin.enabled} disabled={busy || locked}
                        aria-label={`Use ${plugin.name}`} data-test="plugin-enabled"
                        onChange={({ checked }) => { if (checked !== plugin.enabled) void toggle(plugin, checked) }} />
                      <Button size="small" kind="plain-faint" isDisabled={busy || locked} onClick={() => remove(plugin)} data-test="plugin-remove">Remove</Button>
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
        <p className="plugins-foot">
          A plugin that is on offers its skills, agent definitions, hooks and servers to new conversations. Plugins are kept
          in <code>{list.state}/plugins</code>, the same place the terminal&rsquo;s <code>/plugin</code> writes.
        </p>
      )}
    </div>
  )
}
