import type { CutOff } from '../shared/protocol'

/** Stable agent categories, independent of backend wording or translated diagnostics. */
export function failureSummary(category: string, cutOff?: CutOff | null): { title: string; description: string } {
  const rows: Record<string, [string, string]> = {
    cancelled: ['Task stopped', 'Completed changes remain in the project. You can continue from here.'],
    unauthorized: ['Authentication failed', 'Check the model service credentials in Agent settings before continuing.'],
    'rate-limited': ['The provider is busy', 'Wait before retrying, or choose another available model.'],
    unavailable: ['The model service is unavailable', 'Try again when the service recovers, or choose another model.'],
    refused: ['The provider refused this request', 'Review the selected model and provider limits before continuing.'],
    transport: ['The model service could not be reached', 'Check the connection, proxy and certificate settings in Agent settings.'],
    incomplete: ['The model reply was interrupted', 'Your conversation is preserved. Review completed work before continuing.'],
    undecodable: ['The model returned an unusable reply', 'Try again or choose another model.'],
    'too-long': ['The reply reached its output limit', 'Ask for a smaller next step or choose another model.'],
    'model-unconfigured': ['Choose a model from your configured service', 'The selected model uses a service that is not configured. Choose another model from your gateway or AWS Bedrock in the model picker. Your existing API key does not configure the Brave service.'],
    unconfigured: ['The selected model needs configuration', 'Check the selected model and its service credentials in Agent settings, or choose another available model.'],
    blocked: ['A policy blocked this request', 'Review the conversation permissions and managed settings.'],
    workspace: ['The project could not be accessed', 'Check that the project still exists and that the required files are accessible.'],
    internal: ['The turn could not finish', 'Your conversation is preserved. Review diagnostics before continuing.'],
  }
  const [title, description] = rows[category] ?? ['The turn could not finish', 'Your conversation is preserved. Review the details before continuing.']
  if (category === 'too-long' && cutOff) return { title, description: ceilingStop(cutOff) }
  return { title, description }
}

/**
 * A reply that spent the limit on a call is asked for in parts, and one that spent it thinking is
 * not. The agent has already asked the model for smaller parts once, so the setting is named too.
 */
function ceilingStop({ ceiling, call, thought }: CutOff): string {
  const reached = `The model reached its limit of ${ceiling.toLocaleString('en-US')} tokens`
  const raise = 'raise the limit with BRAVEBOT_OUTPUT_BUDGET in the env block of ~/.bravebot/settings.json'
  if (call) {
    const what = call.tool === null ? 'a tool call' : `a call to ${call.tool}`
    return `${reached} part way through ${what}, so the call was not made. Ask for the work in smaller parts, or ${raise}.`
  }
  if (thought) return `${reached} while it was still thinking. Choose another model, or ${raise}.`
  return `${reached}. Ask for a smaller next step, or ${raise}.`
}
