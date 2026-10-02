import type { RunRecord as Saved } from '../../shared/protocol'
import { ago } from './Sessions'

/**
 * A saved manifest run, read only.
 *
 * A run has no conversation, so there is nothing to continue and no composer is drawn. The
 * view shows what the record holds: the task, the goal as the planner understood it, the plan,
 * the steps that ran, and why the run stopped where it did.
 *
 * Everything here is drawn as plain text. The task is what the person typed, and the rest came
 * from a planner that was shown the task and nothing else, or from the agent's own account.
 */
export function RunRecord({ run, onNew }: { run: Saved; onNew: (directory?: string) => void }): React.JSX.Element {
  const { record, manifest, model } = run
  const front = record.front === 'terminal' ? 'the terminal' : record.front === 'desktop' ? 'the desktop app' : null
  return (
    <div className="run-record-body">
      <div className="run-record-notice" role="note">
        <strong>Plan run · read only</strong>
        <p>
          A plan run has no conversation, so it cannot be continued. Start a chat in this
          project to ask again.
        </p>
        <button onClick={() => onNew(record.directory)}>New chat here</button>
      </div>

      <section>
        <h2>Task</h2>
        <pre className="plan-text">{record.title}</pre>
      </section>

      <section className={`run-record-outcome ${manifest.failure ? 'stopped' : 'finished'}`}>
        <h2>Outcome</h2>
        {manifest.failure ? (
          <>
            <p><strong>The run stopped.</strong></p>
            <pre className="plan-text">{manifest.failure}</pre>
          </>
        ) : (
          <p><strong>The run finished.</strong> What it showed at the end was not saved.</p>
        )}
      </section>

      {manifest.goal && (
        <section>
          <h2>Goal, as the planner understood it</h2>
          <pre className="plan-text">{manifest.goal}</pre>
        </section>
      )}

      {manifest.plan ? (
        <section>
          <h2>Plan</h2>
          <pre className="plan-text">{manifest.plan}</pre>
        </section>
      ) : manifest.proposed ? (
        <section>
          <h2>What the planner proposed, which could not be used</h2>
          <pre className="plan-text">{manifest.proposed}</pre>
        </section>
      ) : null}

      <section>
        <h2>Steps that ran</h2>
        {manifest.steps.length ? <pre className="plan-text">{manifest.steps.join('\n')}</pre> : <p>No step ran.</p>}
      </section>

      <p className="run-record-meta">
        {[
          `Run ${record.id}`,
          `saved ${ago(record.updated)}`,
          front && `started from ${front}`,
          model && `model ${model}`,
          `${record.tokens.toLocaleString()} tokens`,
        ].filter(Boolean).join(' · ')}
      </p>
    </div>
  )
}
