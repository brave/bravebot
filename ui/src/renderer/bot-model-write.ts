import type { Bot } from '../shared/bots'

export async function settleBotModelWrite(
  written: Promise<Bot | null>,
  setProblem: (problem: string | null) => void,
  readBots: () => void | Promise<void>,
): Promise<void> {
  try {
    const saved = await written
    if (saved) {
      // Clear before the read: readBots may set a bots-file problem and does not clear.
      setProblem(null)
    } else {
      setProblem(String(new Error('Could not save the bot’s model.')))
    }
  } catch (error) {
    setProblem(String(error))
  }
  await readBots()
}
