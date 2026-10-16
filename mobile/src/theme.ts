import { Platform, useColorScheme } from 'react-native'

// Values are Leo's resolved tokens (@brave/leo tokens/css/variables.css, light and dark), under
// the same role names the desktop app uses (ui/src/renderer/styles/tokens.css), so a role means
// the same thing on both. Nothing here is a value the desktop does not have, except the type sizes
// noted below.

export interface Theme {
  scheme: 'light' | 'dark'
  surface: { app: string; panel: string; raised: string; sunken: string; hover: string; selected: string; scrim: string }
  border: { hairline: string; subtle: string; strong: string; focus: string }
  ink: { primary: string; secondary: string; tertiary: string; disabled: string; accent: string; onAccent: string }
  status: {
    success: string
    successBg: string
    warning: string
    warningBg: string
    error: string
    errorBg: string
    info: string
    infoBg: string
  }
}

const light: Theme = {
  scheme: 'light',
  surface: { app: '#FAFAFB', panel: '#FFFFFF', raised: '#FFFFFF', sunken: '#F2F2F3', hover: '#F2F2F3', selected: '#EEF2FF', scrim: 'rgba(0,0,0,0.5)' },
  border: { hairline: '#F2F2F3', subtle: '#E4E4E5', strong: '#C9C9CA', focus: '#BCC6F3' },
  ink: { primary: '#1C1C1D', secondary: '#464649', tertiary: '#78787C', disabled: 'rgba(28,28,29,0.5)', accent: '#434FCF', onAccent: '#FFFFFF' },
  status: {
    success: '#1C713D', successBg: '#E8F6EC', warning: '#745C00', warningBg: '#FCF2D4',
    error: '#B4020E', errorBg: '#FFEDEB', info: '#0154CC', infoBg: '#E9F3FF',
  },
}

const dark: Theme = {
  scheme: 'dark',
  surface: { app: '#141415', panel: '#1C1C1D', raised: '#1C1C1D', sunken: '#252527', hover: '#252527', selected: '#1A1F38', scrim: 'rgba(0,0,0,0.75)' },
  border: { hairline: '#303032', subtle: '#303032', strong: '#464649', focus: '#303A68' },
  ink: { primary: '#E4E4E5', secondary: '#C9C9CA', tertiary: '#909093', disabled: 'rgba(228,228,229,0.5)', accent: '#BCC6F3', onAccent: '#212848' },
  status: {
    success: '#38C971', successBg: '#0A2716', warning: '#CFA608', warningBg: '#2C2000',
    error: '#FF7F72', errorBg: '#391816', info: '#78ABFF', infoBg: '#112238',
  },
}

export const space = { s: 4, m: 8, l: 12, xl: 16, xxl: 24 } as const
// Leo radius: s 4, m 8, l 12, xl 16, xxl 24, full.
export const radius = { chip: 4, control: 8, card: 12, field: 16, bubble: 24, pill: 999 } as const
/** The ground showing around a card. */
export const cardGap = 8
export const titlebar = 44
export const hit = 44
export const readingWidth = 720

const mono = Platform.select({ ios: 'Menlo', default: 'monospace' })

// Leo font roles: page title 28/34, title 16/24, large 16/24, default 14/22, small 12/18,
// x-small 11/16, monospace small 12. The transcript reads at the large role.
export const type = {
  hero: { fontSize: 28, lineHeight: 34, fontWeight: '600' as const },
  title: { fontSize: 16, lineHeight: 24, fontWeight: '600' as const },
  body: { fontSize: 16, lineHeight: 24 },
  ui: { fontSize: 14, lineHeight: 22 },
  uiStrong: { fontSize: 14, lineHeight: 22, fontWeight: '600' as const },
  meta: { fontSize: 12, lineHeight: 18 },
  metaStrong: { fontSize: 12, lineHeight: 18, fontWeight: '600' as const },
  caption: { fontSize: 11, lineHeight: 16 },
  code: { fontSize: 12, lineHeight: 18, fontFamily: mono },
}

export function useTheme(): Theme {
  return useColorScheme() === 'dark' ? dark : light
}

export const themes = { light, dark }
