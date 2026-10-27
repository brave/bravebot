//! The codes of the closed list of CHATGPT-8 that tests name, as the clause writes them and not as
//! the table in the crate holds them. A test that names a code the table does not hold is not one
//! that reaches the refusal it is about.
#![allow(dead_code, reason = "each file that includes this one uses some of it")]

/// The code that says the account has reached its usage limit.
pub const USAGE_LIMIT: &str = "subscription_sharing_usage_limit_exceeded";
/// The code that says the account has reached the usage limit a workspace administrator set for its
/// user, which other programs' sources and a pull request of theirs state and the OpenAI pages that
/// were read do not.
pub const WORKPLACE_USAGE_LIMIT: &str =
    "subscription_sharing_workplace_individual_usage_limit_exceeded";
/// One of the codes that say the account may not be used this way.
pub const NOT_ELIGIBLE: &str = "subscription_sharing_user_not_eligible";
/// Another of the six.
pub const INVALID_USER: &str = "subscription_sharing_invalid_user";

/// The code the serving host gives a model the account does not offer, which is not one of the
/// listed codes: it refuses the model and says nothing of the account.
pub const MODEL_NOT_FOUND: &str = "model_not_found";
