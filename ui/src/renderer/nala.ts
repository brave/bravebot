/**
 * One place the renderer reaches Leo (Nala) React wrappers from.
 *
 * Call sites import components here rather than deep paths into `@brave/leo`.
 * The token stylesheet and icon base path are set from `nala-setup.ts` so Node
 * tests that load React components through esbuild do not have to emit CSS.
 *
 * Button, Input, Navigation, NavigationItem, TextArea, Checkbox, RadioButton and Toggle are wrapped by `withShadowAttrs`
 * (see `nala-a11y.tsx`): Leo's React wrappers drop `aria-*` props, so an icon-only button would
 * have no accessible name. ButtonMenu is wrapped so its anchor is not a second button around the
 * Button inside it. Everything else is Leo's own export.
 *
 * Fonts stay system: Leo's `--leo-font-*` tokens already name `system-ui` / SF /
 * Segoe, and this app does not load Inter or Poppins.
 */

import LeoButton from '@brave/leo/react/button'
import LeoButtonMenu from '@brave/leo/react/buttonMenu'
import LeoCheckbox from '@brave/leo/react/checkbox'
import LeoInput from '@brave/leo/react/input'
import LeoNavigation from '@brave/leo/react/navigation'
import LeoNavigationItem from '@brave/leo/react/navigationItem'
import LeoRadioButton from '@brave/leo/react/radioButton'
import LeoTextArea from '@brave/leo/react/textarea'
import LeoToggle from '@brave/leo/react/toggle'
import { withPlainMenuAnchor, withShadowAttrs } from './nala-a11y'

export const Button = withShadowAttrs(LeoButton, 'button, a')
export const ButtonMenu = withPlainMenuAnchor(LeoButtonMenu)
export const Checkbox = withShadowAttrs(LeoCheckbox, 'input')
export const Input = withShadowAttrs(LeoInput, 'input', { flattenTabindex: true })
export const Navigation = withShadowAttrs(LeoNavigation, 'nav')
export const NavigationItem = withShadowAttrs(LeoNavigationItem, 'button, a')
export const RadioButton = withShadowAttrs(LeoRadioButton, 'input')
export const TextArea = withShadowAttrs(LeoTextArea, 'textarea', { flattenTabindex: true })
export const Toggle = withShadowAttrs(LeoToggle, 'button')

export { default as Alert } from '@brave/leo/react/alert'
export { default as Collapse } from '@brave/leo/react/collapse'
export { default as Dialog } from '@brave/leo/react/dialog'
export { default as Dropdown } from '@brave/leo/react/dropdown'
export { default as FormItem } from '@brave/leo/react/formItem'
export { default as Hr } from '@brave/leo/react/hr'
export { default as Icon } from '@brave/leo/react/icon'
export { default as Label } from '@brave/leo/react/label'
export { default as Link } from '@brave/leo/react/link'
export { default as Menu } from '@brave/leo/react/menu'
export { default as ProgressBar } from '@brave/leo/react/progressBar'
export { default as ProgressRing } from '@brave/leo/react/progressRing'
export { default as SegmentedControl } from '@brave/leo/react/segmentedControl'
export { default as ControlItem } from '@brave/leo/react/segmentedControlItem'
export { default as Tabs } from '@brave/leo/react/tabs'
export { default as TabItem } from '@brave/leo/react/tabItem'
export { default as Tooltip } from '@brave/leo/react/tooltip'
export type { IconName } from '@brave/leo/icons/meta'
export { setIconBasePath } from '@brave/leo/react/icon'
