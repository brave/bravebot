/**
 * What the page says to the main process when it fails (DIAG-10): where it failed and the name of
 * the error, and nothing else.
 *
 * The error's message and stack stay in the window. They are shown to the person, who can copy
 * them, and are sent nowhere: the diagnostic log holds only words the agent fixes, and a message
 * can carry whatever the page was drawing.
 */
export type PageFaultKind = 'render' | 'uncaught' | 'rejection'

export interface PageFault {
  kind: PageFaultKind
  name: string
}
