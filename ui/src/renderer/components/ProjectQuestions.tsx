import { useRef } from 'react'
import { Modal } from './Modal'
import { Answers, Approve } from './Approval'
import { Button, Icon } from '../nala'
import { t } from '../i18n'
import type { FolderQuestion, RulesQuestion } from '../../shared/protocol'

/**
 * The rules in the order the bridge listed them, each run from one file under that file's name,
 * so a long list does not repeat a long path under every rule. Every rule is still its own row.
 */
function byFile(rules: RulesQuestion['rules']): { file: string; rules: string[] }[] {
  const groups: { file: string; rules: string[] }[] = []
  for (const rule of rules) {
    const last = groups[groups.length - 1]
    if (last && last.file === rule.shownFile) last.rules.push(rule.shownRule)
    else groups.push({ file: rule.shownFile, rules: [rule.shownRule] })
  }
  return groups
}

/**
 * Whether to grant the `allow` rules this checkout's settings propose, asked once its trust
 * question is answered (PERM-15).
 *
 * Modal, as the trust question is, with no way past it but an answer: no default and no dismiss.
 * The grant takes no press until every rule listed has been on screen (PROMPT-4, issue #954), so
 * nobody grants a rule they were never shown; a list that fits counts as read at once, and a long
 * one scrolls inside its own box. Keep asking me answers at every draw.
 *
 * The answer is a yes or a no to the question the bridge put, by its request id. The window names
 * no rule back: what a yes grants is what the bridge listed.
 */
export function RulesPrompt({ question, onAnswer }: {
  question: RulesQuestion
  onAnswer: (granted: boolean) => void
}): React.JSX.Element {
  const rows = useRef<HTMLDivElement>(null)
  const count = question.rules.length
  return (
    <Modal title={t.uiProjectRulesTitle()} size="md" className="project-question" subtitle={t.uiProjectRulesSubtitle()} subtitleId="project-rules-title" actions={
      <Answers rows={rows} className="trust-answers">
        <Button kind="plain-faint" onClick={() => onAnswer(false)} data-test="rules-decline">
          {t.uiProjectRulesDecline()}
        </Button>
        <Approve size="medium" onClick={() => onAnswer(true)} data-test="rules-grant">
          {t.uiProjectRulesGrant({ count })}
        </Approve>
      </Answers>
    }>
      <div ref={rows} className="trust-rows">
        <p className="project-question-about">{t.uiProjectRulesAbout()}</p>
        <div className="project-rules" tabIndex={0} role="group" aria-label={t.uiProjectRulesList()} data-test="rules-list">
          {byFile(question.rules).map((group, at) => (
            <section key={at} className="project-rules-file">
              <h4 className="project-rule-file" data-deciding="all" data-test="rules-file">{t.uiProjectRulesFrom({ file: group.file })}</h4>
              <ul>
                {group.rules.map((rule, index) => (
                  <li key={index} data-deciding="all" data-test="rules-row"><code>{rule}</code></li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        {question.grantedBefore.length > 0 && (
          <div className="project-rules-before" data-test="rules-granted-before">
            <p>{t.uiProjectRulesGrantedBefore({ count: question.grantedBefore.length })}</p>
            <ul>
              {question.grantedBefore.map((rule, index) => (
                <li key={index}><code>{rule.shownRule}</code> <span className="project-rule-file">{t.uiProjectRulesFrom({ file: rule.shownFile })}</span></li>
              ))}
            </ul>
          </div>
        )}
        <ul className="trust-options">
          <li data-deciding="all">
            <Icon name="pin" />
            <p data-test="rules-keeping">
              {question.shownKeeping ? t.uiProjectRulesKeeping({ path: question.shownKeeping }) : t.uiProjectRulesThisChat()}
            </p>
          </li>
          <li>
            <Icon name="shield-done" />
            <p>{t.uiProjectRulesDeclineAbout()}</p>
          </li>
        </ul>
      </div>
    </Modal>
  )
}

/**
 * Whether to open one directory this checkout's settings name in `additionalDirectories`, beside
 * the project, as `/add-dir` opens one (TRUST-9). One question per directory, asked after the
 * rules question; the bridge says in each answer which comes next.
 */
export function FolderPrompt({ question, onAnswer }: {
  question: FolderQuestion
  onAnswer: (open: boolean) => void
}): React.JSX.Element {
  const rows = useRef<HTMLDivElement>(null)
  return (
    <Modal title={t.uiProjectFolderTitle()} size="sm" className="project-question" subtitle={t.uiProjectFolderSubtitle()} subtitleId="project-folder-title" actions={
      <Answers rows={rows} className="trust-answers">
        <Button kind="plain-faint" onClick={() => onAnswer(false)} data-test="folder-decline">
          {t.uiProjectFolderDecline()}
        </Button>
        <Approve size="medium" onClick={() => onAnswer(true)} data-test="folder-open">
          {t.uiProjectFolderOpen()}
        </Approve>
      </Answers>
    }>
      <div ref={rows} className="trust-rows">
        <code className="path" tabIndex={0} aria-label={t.uiProjectFolderTitle()} data-deciding="all" data-test="folder-path">{question.shownDirectory}</code>
        {question.shownNamed.length > 0 && (
          <p className="project-folder-named" data-test="folder-named">
            {t.uiProjectFolderNamed()}{' '}
            {question.shownNamed.map((name, index) => (
              <span key={index}>{index > 0 && ', '}<code>{name}</code></span>
            ))}
          </p>
        )}
        <ul className="trust-options">
          <li data-deciding="all">
            <Icon name="folder-open" />
            <p>{t.uiProjectFolderOpenAbout()}</p>
          </li>
          <li>
            <Icon name="eye-off" />
            <p>{t.uiProjectFolderDeclineAbout()}</p>
          </li>
        </ul>
        {question.remaining > 0 && (
          <p className="aside" data-test="folder-remaining">{t.uiProjectFolderRemaining({ count: question.remaining })}</p>
        )}
      </div>
    </Modal>
  )
}
