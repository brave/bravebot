import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from '../src/renderer/App'
import { setLocale } from '../src/renderer/i18n'
import '../src/renderer/nala-setup'
import '../src/renderer/styles.css'
import { install, onScreen, play, playground, route, stop } from './bridge'
import { Panel } from './panel'
import { SCENARIOS } from './scenarios'
import './panel.css'

// The bridge is in place before the first request, which `askLocale` makes.
install()
for (const scenario of SCENARIOS) route(scenario.id, scenario.run)

/** The same controls as the panel, for the console: `playground.play('tour')`, `playground.scroll('top')`. */
const scroller = (): HTMLElement | null => document.querySelector<HTMLElement>('.entries')
Object.assign(window, {
  playground: {
    play: (id: string) => {
      const scenario = SCENARIOS.find((each) => each.id === id)
      const session = onScreen()
      return scenario && session ? play(session, scenario.title, scenario.run) : false
    },
    stop: () => { const session = onScreen(); if (session) stop(session) },
    scroll: (to: 'top' | 'end' | number) => {
      const element = scroller()
      if (element) element.scrollTop = to === 'top' ? 0 : to === 'end' ? element.scrollHeight : to
      return element ? [element.scrollTop, element.scrollHeight, element.clientHeight] : null
    },
    settings: playground,
    scenarios: SCENARIOS.map((each) => each.id),
  },
})

document.documentElement.dataset.platform = 'other'
setLocale('en')

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')
createRoot(root).render(
  <StrictMode>
    <App />
    <Panel />
  </StrictMode>,
)

// Land in the conversation that shows the most, unless the welcome screen is what is wanted.
if (!new URLSearchParams(location.search).has('welcome')) {
  const started = Date.now()
  const timer = setInterval(() => {
    const row = [...document.querySelectorAll<HTMLElement>('button.session')].find((each) => each.textContent?.includes('Everything the conversation can show'))
    if (row) { clearInterval(timer); row.click() }
    else if (Date.now() - started > 8000) clearInterval(timer)
  }, 100)
}
