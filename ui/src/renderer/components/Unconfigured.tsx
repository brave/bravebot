import { Modal } from './Modal'
import { t } from '../i18n'
import { Button, Collapse } from '../nala'

export function Unconfigured({ detail, onClose, onSettings }: { detail: string; onClose: () => void; onSettings: () => void }): React.JSX.Element {
  return <Modal title={t.uiUnconfiguredTitle()} size="md" className="unconfigured" onClose={onClose}
    subtitle={t.uiUnconfiguredSubtitle()}
    actions={<>
      <Button kind="plain-faint" onClick={onClose} data-test="unconfigured-continue">{t.uiUnconfiguredContinue()}</Button>
      <Button kind="filled" onClick={onSettings} data-test="unconfigured-settings">{t.uiUnconfiguredOpenSettings()}</Button>
    </>}>
    <p>{t.uiUnconfiguredBody()}</p>
    <Collapse className="dialog-collapse" title={t.uiUnconfiguredDetails()} isOpen={undefined} data-test="unconfigured-details">
      <pre>{detail}</pre>
    </Collapse>
  </Modal>
}
