import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './shadcn.css'
import './styles.css'
import './modern.css'

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
