/** Stable agent categories, independent of backend wording or translated diagnostics. */
export function failureSummary(category: string): { title: string; description: string } {
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
  return { title, description }
}
