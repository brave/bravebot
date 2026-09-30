import { useEffect, useState } from 'react'
import { IconButton } from './IconButton'
import { showToast } from '../toasts'

/**
 * Put some text on the clipboard, and say so on the button for a moment and in a toast.
 *
 * The text is read when the button is pressed rather than when it is drawn, so a long reply
 * is not flattened to a string on every render of the row it sits in.
 */
export function CopyButton({ text, label, className, size = 'tiny', 'data-test': dataTest }: {
  text: () => string
  label: string
  className?: string
  size?: 'tiny' | 'small'
  'data-test'?: string
}): React.JSX.Element {
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle')
  useEffect(() => {
    if (state === 'idle') return
    const timer = setTimeout(() => setState('idle'), state === 'copied' ? 1500 : 4000)
    return () => clearTimeout(timer)
  }, [state])
  return (
    <>
      <IconButton
        icon={state === 'copied' ? 'check-normal' : 'copy'}
        label={state === 'copied' ? 'Copied' : label}
        tooltip={state === 'failed' ? 'Could not copy. Select the text and copy it manually.' : state === 'copied' ? 'Copied' : label}
        size={size}
        className={`copy-button${state === 'copied' ? ' copied' : ''}${className ? ` ${className}` : ''}`}
        data-test={dataTest}
        onClick={() => {
          void navigator.clipboard.writeText(text()).then(() => { setState('copied'); showToast('Copied to clipboard') }, () => setState('failed'))
        }}
      />
      <span className="offscreen" role="status">{state === 'failed' ? 'Could not copy' : ''}</span>
    </>
  )
}
