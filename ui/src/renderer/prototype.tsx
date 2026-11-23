/**
 * The page of the prototype window: the desktop as a client of `bravebot-host` (HOSTCLIENT-1).
 *
 * It has no `window.bravebot`. The only way out is `window.prototypeHost`, which its preload
 * exposes, and the ordinary window's preload is not loaded here.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import type { PrototypeApi } from './prototype/PrototypeApp'
import { PrototypeApp } from './prototype/PrototypeApp'
import './nala-setup'
import './styles.css'
import './styles/prototype.css'

declare global {
  interface Window { prototypeHost?: PrototypeApi }
}

const root = document.getElementById('root')
if (!root) throw new Error('no #root to mount into')
const api = window.prototypeHost
if (!api) throw new Error('this page is the prototype window and needs its preload')
createRoot(root).render(
  <StrictMode>
    <PrototypeApp api={api} />
  </StrictMode>,
)
