import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './nala-setup'
import './styles.css'

// Read before the first paint, so the titlebar is laid out for the platform it is on.
document.documentElement.dataset.platform = /Mac/.test(navigator.userAgent) ? 'mac' : 'other'

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
