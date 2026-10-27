import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { setLocale } from './i18n'
import type { LanguageReport } from '../shared/protocol'
import './nala-setup'
import './styles.css'

// Read before the first paint, so the titlebar is laid out for the platform it is on.
// Only a platform known not to be macOS gives the room back: if this cannot be told, the traffic
// lights stay clear, which is the mistake that costs nothing.
const { platform } = window.bravebot
if (typeof platform === 'string' && platform !== 'darwin') document.documentElement.dataset.platform = 'other'

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')

// The catalog is chosen once, before anything is drawn (LOCALE-7). A bridge that cannot say, or
// does not say soon, leaves the reference rather than a blank window.
const LANGUAGE_WAIT_MS = 3000

async function chooseLocale(): Promise<void> {
  const late = new Promise<null>((resolve) => setTimeout(() => resolve(null), LANGUAGE_WAIT_MS))
  try {
    const answer = await Promise.race([window.bravebot.request<LanguageReport>('language.get'), late])
    setLocale(typeof answer?.ok?.inUse === 'string' ? answer.ok.inUse : 'en-US')
  } catch {
    setLocale('en-US')
  }
}

void chooseLocale().then(() => {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
})
