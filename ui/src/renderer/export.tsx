/**
 * The page a PDF is photographed from.
 *
 * It renders one document and then says so. Everything about why this exists as a second
 * entry point rather than a string of HTML is in `src/main/export.ts`.
 */

import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { ExportView } from './components/ExportView'
import type { ExportDocument } from '../shared/export'
import './styles.css'
import './export.css'

function Page(): React.JSX.Element | null {
  const [ready, setReady] = useState<{ document: ExportDocument; at: number } | null>(null)

  useEffect(() => {
    window.bravebotExport?.onDocument((document, at) => setReady({ document, at }))
  }, [])

  // Said after the browser has painted this pass and the fonts it used have loaded. Both
  // matter: a print taken before either would come out with the wrong metrics or the wrong
  // typeface, and neither failure announces itself in the file.
  useEffect(() => {
    if (!ready) return
    let cancelled = false
    void document.fonts.ready.then(() => {
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          if (!cancelled) window.bravebotExport?.ready()
        }),
      )
    })
    return () => {
      cancelled = true
    }
  }, [ready])

  if (!ready) return null
  return <ExportView document={ready.document} at={ready.at} />
}

const root = document.getElementById('root')
if (root) createRoot(root).render(<Page />)
