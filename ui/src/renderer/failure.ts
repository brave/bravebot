import type { CutOff } from '../shared/protocol'

/**
 * Stable agent categories, independent of backend wording or translated diagnostics.
 *
 * `accountMessage` is the bridge's sentence for a failure of a ChatGPT or an xAI account, where it was one: one
 * sentence in sentence case that says what happened and what to do. It is the description over the
 * category it arrives beside, under one title, so which failure it was is said once, in the bridge's
 * catalog, and this window keeps no table of the words (SHARE-001). The command it names arrives as
 * `accountCommand` and the card draws it as code. The title is one for both accounts, since a title that
 * named the service would be a table of service names here, and the sentence under it names the service.
 *
 * `failureMessage` is the bridge's sentence for any other failure it has one for: the agent's own line,
 * the one the terminal shows, which says the cause, the figure and the setting that changes it. It is
 * the description under the category's title, so these words too are said once, in the agent's catalog.
 * Where the line is for a request whose reply did not begin or went quiet, `failureCause` names that
 * cause and the title says it, since the category's title says the service could not be reached and
 * it was. The title is chosen from the closed name and never from the line.
 * The table below is what a failure with neither sentence is described by.
 */
export function failureSummary(category: string, cutOff?: CutOff | null, accountMessage?: string | null, failureMessage?: string | null, failureCause?: string | null): { title: string; description: string } {
  if (accountMessage) return { title: 'The account could not be used', description: accountMessage }
  const rows: Record<string, [string, string]> = {
    cancelled: ['Task stopped', 'Completed changes remain in the project. You can continue from here.'],
    unauthorized: ['Authentication failed', 'Check the model service credentials in Agent settings before continuing.'],
    'rate-limited': ['The provider is busy', 'Wait before retrying, or choose another available model.'],
    unavailable: ['The model service is unavailable', 'Try again when the service recovers, or choose another model.'],
    refused: ['The provider refused this request', 'Review the selected model and provider limits before continuing.'],
    transport: ['The model service could not be reached', 'Check the connection, proxy and certificate settings in Agent settings.'],
    incomplete: ['The model reply was interrupted', 'Your conversation is preserved. Review completed work before continuing.'],
    undecodable: ['The model returned an unusable reply', 'Try again or choose another model.'],
    unrecognised: ['The service reported a failure this version does not recognise', 'Choose another model, or run bravebot doctor to see the endpoint and the diagnostic log.'],
    'too-long': ['The reply reached its output limit', 'Ask for a smaller next step, or raise the output limit with BRAVEBOT_OUTPUT_BUDGET; the limit is on one reply and not on the context window, so compaction does not change it.'],
    'model-unconfigured': ['Choose a model from your configured service', 'The selected model uses a service that is not configured. Choose another model from your gateway or AWS Bedrock in the model picker. Your existing API key does not configure the Brave service.'],
    unconfigured: ['The selected model needs configuration', 'Check the selected model and its service credentials in Agent settings, or choose another available model.'],
    blocked: ['A policy blocked this request', 'Review the conversation permissions and managed settings.'],
    workspace: ['The project could not be accessed', 'Check that the project still exists and that the required files are accessible.'],
    internal: ['The turn could not finish', 'Your conversation is preserved. Review diagnostics before continuing.'],
    // Set by this window, not the bridge: a `/plan` line whose `session.mode` request failed (MODE-12).
    'plan-mode': ['Could not switch to Plan mode', 'Nothing was sent, so try again or choose Plan from the mode menu.'],
  }
  const [title, description] = rows[category] ?? ['The turn could not finish', 'Your conversation is preserved. Review the details before continuing.']
  const causes: Record<string, string> = {
    begin: 'The model reply did not begin in time',
    'idle-gap': 'The model reply went quiet',
  }
  if (failureMessage) return { title: (failureCause && causes[failureCause]) || title, description: failureMessage }
  if (category === 'too-long' && cutOff) return { title, description: ceilingStop(cutOff) }
  return { title, description }
}

/**
 * A reply that spent the limit on a call is asked for in parts, and one that spent it thinking is
 * not. The agent asks the model again once for each kind of stop before the turn fails (TURN-7), so
 * the setting is named too. Each says the limit is on one reply and not on the context window, as
 * the agent's line does (BACKEND-37).
 */
function ceilingStop({ ceiling, call, thought }: CutOff): string {
  const reached = `The model reached its limit of ${ceiling.toLocaleString('en-US')} tokens`
  const raise = 'raise the limit with BRAVEBOT_OUTPUT_BUDGET in the env block of ~/.bravebot/settings.json'
  const window = 'The limit is on one reply and not on the context window, so compaction does not change it.'
  if (call) {
    const what = call.tool === null ? 'a tool call' : `a call to ${call.tool}`
    return `${reached} part way through ${what}, so the call was not made. Ask for the work in smaller parts, or ${raise}. ${window}`
  }
  if (thought) return `${reached} while it was still thinking. Choose another model, or ${raise}. ${window}`
  return `${reached}. Ask for a smaller next step, or ${raise}. ${window}`
}
