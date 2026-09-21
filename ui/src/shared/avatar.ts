/** Versioned seeds opt new bots into new geometry without changing existing faces. */
export const AVATAR_VERSION = 'v2:'
export const newAvatarSeed = (entropy: string): string => `${AVATAR_VERSION}${entropy}`
