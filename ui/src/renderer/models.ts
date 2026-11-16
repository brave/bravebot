/**
 * Model choices belong to this UI's conversations, not the CLI's global preference.
 *
 * Only a model the person picked is stored here. A model the backend reported as the default is
 * not a choice, and keeping it would go on requesting that model after the settings that named it
 * stopped applying (BACKEND-33). The key carries `picked` for that reason: entries written under
 * `bravebot.conversation-model:` recorded every model a conversation showed, picked or not, and
 * cannot be told apart from picks, so they are not read.
 */
export function modelKey(directory: string, id: string): string {
  return `bravebot.picked-model:${JSON.stringify([directory, id])}`
}

/** The model the person picked for this conversation, or `null` where they picked none. */
export function pickedModel(directory: string, id: string): string | null {
  try {
    return localStorage.getItem(modelKey(directory, id)) || null
  } catch {
    return null
  }
}

export function rememberModel(directory: string, id: string, model: string | null): void {
  try {
    if (model) localStorage.setItem(modelKey(directory, id), model)
    else localStorage.removeItem(modelKey(directory, id))
  } catch {
    // A full or unavailable preference store must not prevent sending a prompt.
  }
}

/** The model a conversation shows and whether the person picked it (BACKEND-33). */
export interface ShownModel {
  model: string | null
  modelPicked?: boolean
}

/**
 * Store the model a conversation shows only where the person picked it. A default the backend
 * reported is not stored, so it is not requested again after the settings that named it stop
 * applying.
 */
export function rememberPickedModel(directory: string, id: string, shown: ShownModel): void {
  if (shown.modelPicked) rememberModel(directory, id, shown.model)
}

/**
 * The model a reopened conversation shows: a bot's own, else the person's stored pick, else the
 * one the backend reported. Only the stored pick counts as picked; a bot's model and the backend's
 * default do not, so reopening never stores either.
 */
export function reopenedModel(
  botModel: string | null | undefined,
  picked: string | null,
  reported: string | null,
): ShownModel {
  return { model: botModel ?? picked ?? reported, modelPicked: picked !== null }
}

/** What a conversation shows once the person picks `model`, the one way `modelPicked` becomes true. */
export function pickModel<T extends ShownModel>(shown: T, model: string | null): T {
  return { ...shown, model, modelPicked: true }
}
