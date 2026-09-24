import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { TooltipProvider } from './components/ui/tooltip'
import './shadcn.css'
import './styles.css'
import './modern.css'

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')
document.documentElement.classList.toggle('dark', window.matchMedia('(prefers-color-scheme: dark)').matches)

createRoot(root).render(
  <StrictMode>
    <TooltipProvider delayDuration={350}>
      <App />
    </TooltipProvider>
  </StrictMode>,
)
