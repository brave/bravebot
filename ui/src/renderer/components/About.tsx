import { useState } from 'react'
import { Modal } from './Modal'
import { Button, Collapse, Icon, Link } from '../nala'
import { updateLine, updateState, type UpdateStatus } from '../../shared/update'

export interface AboutInfo {
  version: string
  build: string
  home: string | null
}

const project = 'https://github.com/brave/bravebot'

export function About({ info, update, onDownload, onReportBug, onClose }: {
  info: AboutInfo
  /** What the last update check found (UPDATE-12), or nothing where it could not be read. */
  update: UpdateStatus | null
  onDownload: () => void
  onReportBug: () => void
  onClose: () => void
}): React.JSX.Element {
  const newer = updateState(update) === 'newer'
  // The pinned agent stamps its build as "version (commit)"; info.version is the bridge version.
  const agentVersion = info.build.split(' ')[0]
  const [copyStatus, setCopyStatus] = useState('')

  async function copyBuildInfo() {
    try {
      await navigator.clipboard.writeText(`Brave Bot\nInterface: ${info.version}\nAgent: ${info.build}`)
      setCopyStatus('Build info copied')
    } catch {
      setCopyStatus('Could not copy build info. Please select and copy the details below.')
    }
  }

  return <Modal title="About Brave Bot" size="sm" onClose={onClose} className="about">
    <div className="about-hero">
      <h2>Brave Bot</h2>
      <span className="about-version">Version {agentVersion}</span>
    </div>
    <div className={`about-update${newer ? ' newer' : ''}`} role="status" data-test="about-update" data-state={updateState(update)}>
      <span>{updateLine(update)}</span>
      {newer && <Button kind="filled" size="small" onClick={onDownload} data-test="about-download">
        <Icon name="download" slot="icon-before" />Download
      </Button>}
    </div>
    <nav className="about-links" aria-label="Project resources">
      <Link href={project} target="_blank" rel="noreferrer">GitHub<Icon name="launch" slot="icon-after" /></Link>
      <Link href={`${project}/releases`} target="_blank" rel="noreferrer">Release notes<Icon name="launch" slot="icon-after" /></Link>
      <Button kind="plain" size="small" className="about-report" onClick={onReportBug} data-test="about-report-bug">Report a bug…</Button>
    </nav>
    <Collapse className="about-details" title="Build & storage details" isOpen={undefined} data-test="about-details">
      <dl>
        <div><dt>Interface</dt><dd>{info.version}</dd></div>
        <div><dt>Agent</dt><dd>{info.build}</dd></div>
        <div><dt>Sessions</dt><dd>{info.home ?? 'Session folder unavailable'}</dd></div>
      </dl>
      <div className="about-copy">
        <Button size="small" kind="outline" onClick={() => void copyBuildInfo()} data-test="about-copy">
          <Icon name="copy" slot="icon-before" />
          Copy build info
        </Button>
        <span role="status">{copyStatus}</span>
      </div>
    </Collapse>
    <footer className="about-footer">
      <span>Built with <Link href={project} target="_blank" rel="noreferrer">bravebot</Link></span>
      <span aria-hidden="true">·</span>
      <Link href={`${project}/blob/main/LICENSE`} target="_blank" rel="noreferrer">MPL-2.0</Link>
    </footer>
  </Modal>
}
