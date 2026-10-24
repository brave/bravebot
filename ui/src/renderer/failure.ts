import type { CutOff } from '../shared/protocol'

const SIGN_IN = 'Run bravebot auth login chatgpt in a terminal to sign in'
const OTHER_SERVICE = 'Choose a model from another service to keep working.'
const NOT_RENEWED = 'The ChatGPT sign-in could not be renewed'

/**
 * Which failure of a ChatGPT account it was, by the fixed names the agent sends beside the category
 * (CHATGPT-8), each with its own title and sentence. The set is closed, so a name is looked up here
 * and never printed: a name this build does not know is not in the table and says nothing. A map and
 * not an object, so `constructor` is not a name that finds a row.
 *
 * The sign-in is a command typed in a terminal, never something this window starts (CHATGPT-1), so
 * the sentences that send the person to sign in name the command and say where to run it. A usage
 * limit and an account that may not be used this way are not cured by signing in again, so neither
 * says to.
 */
const ACCOUNT_FAILURES = new Map<string, [string, string]>([
  ['not-signed-in', ['Not signed in to ChatGPT', `${SIGN_IN}, then send the message again.`]],
  ['ended', ['The ChatGPT sign-in has ended', `${SIGN_IN} again.`]],
  ['token-refused', ['OpenAI refused the ChatGPT sign-in', `${SIGN_IN} again.`]],
  ['unusable', ['The stored ChatGPT sign-in could not be read', 'Run bravebot auth status chatgpt in a terminal to see why.']],
  ['usage-limit', ['OpenAI says this ChatGPT account has reached its usage limit', OTHER_SERVICE]],
  ['not-permitted', ['OpenAI does not allow this ChatGPT account to be used this way', OTHER_SERVICE]],
  ['renewal-refused', [NOT_RENEWED, 'The request to renew it was refused.']],
  ['renewal-rate-limited', [NOT_RENEWED, 'The request to renew it was rate-limited. Wait before trying again.']],
  ['renewal-unavailable', [NOT_RENEWED, 'The request to renew it was met with an unavailable service. Try again when it recovers.']],
  ['renewal-incomplete', [NOT_RENEWED, 'The reply to the request to renew it could not be read.']],
  ['renewal-transport', [NOT_RENEWED, 'The request to renew it was not completed. Check the connection and try again.']],
])

/**
 * Stable agent categories, independent of backend wording or translated diagnostics.
 *
 * `account` is the name of which failure of a ChatGPT account it was, where it was one, and it
 * decides the wording over the category it arrives beside.
 */
export function failureSummary(category: string, cutOff?: CutOff | null, account?: string | null): { title: string; description: string } {
  const named = account ? ACCOUNT_FAILURES.get(account) : undefined
  if (named) return { title: named[0], description: named[1] }
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
