/**
 * Start the app again, unless a turn is running in any session. Quitting disposes of the bridge,
 * which refuses every approval a turn is waiting on and ends the turn, and a restart offered for a
 * language is not worth that. Answers whether the app is restarting.
 */
export function relaunchUnlessRunning(
  running: ReadonlySet<string>,
  app: { relaunch(): void; quit(): void },
): boolean {
  if (running.size > 0) return false
  app.relaunch()
  app.quit()
  return true
}
