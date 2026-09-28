/// <reference types="vite/client" />
import type { BravebotApi } from '../preload/index'
import type { BravebotExportApi } from '../preload/export'

declare global {
  interface Window {
    bravebot: BravebotApi
    /**
     * Optional because it exists only in the offscreen window that prints a PDF. The main
     * window must never assume it, which is what the `?` is here to enforce.
     */
    bravebotExport?: BravebotExportApi
  }
}

declare module 'react' {
  namespace JSX {
    interface IntrinsicElements {
      'leo-option': React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & { value?: string },
        HTMLElement
      >
      'leo-menu-item': React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & { value?: string },
        HTMLElement
      >
      'leo-menu-section': React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement>,
        HTMLElement
      >
    }
  }
}

export {}
