/** The prefix on new seeds. Any string draws a face; the prefix only marks when a seed was made. */
export const AVATAR_VERSION = 'v2:'
export const newAvatarSeed = (entropy: string): string => `${AVATAR_VERSION}${entropy}`
