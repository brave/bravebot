/**
 * The prefix on new seeds. Any string draws a face. A seed with this prefix is painted from the
 * sixteen paints. A seed made earlier keeps the eleven it was painted with.
 */
export const AVATAR_VERSION = 'v3:'
export const newAvatarSeed = (entropy: string): string => `${AVATAR_VERSION}${entropy}`
