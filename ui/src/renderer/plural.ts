/** `1 chat`, `2 chats`: a count and a noun whose plural adds an s. */
export const plural = (count: number, one: string): string => `${count} ${one}${count === 1 ? '' : 's'}`
