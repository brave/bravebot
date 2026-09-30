import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './nala-setup'
import './styles.css'

// Read before the first paint, so the titlebar is laid out for the platform it is on.
// Only a platform known not to be macOS gives the room back: if this cannot be told, the traffic
// lights stay clear, which is the mistake that costs nothing.
const { platform } = window.bravebot
if (typeof platform === 'string' && platform !== 'darwin') document.documentElement.dataset.platform = 'other'

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
