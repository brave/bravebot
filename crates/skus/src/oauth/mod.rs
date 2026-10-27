//! What the sign-ins that keep tokens in a file have in common: the file, the claim a write is made
//! under, the requests to a token endpoint, and the replacing of an access token that is about to
//! expire.
//!
//! The sign-ins are [`crate::chatgpt`] and [`crate::xai`]. Each states its own file, its own
//! destinations and what a refresh presents, as a [`Kind`], and this module is the rest. It holds no
//! label and no workspace content, for the reason [the crate](crate) gives: a person types the
//! command at a shell, so no planner exists, and what is read from a reply is compared and parsed
//! here and handed to nothing a turn sees. What leaves it is a token that is never read, a number of
//! seconds, and a value from the closed set of [`Class`], [`AccountError`] and [`TokenError`].

mod file;
mod renew;
mod wire;

pub(crate) mod encoding;
#[cfg(test)]
pub(crate) mod stand_in;

pub use file::{AccountError, Defect};
pub use renew::{Access, Renewer, TokenError};
pub use wire::{Lifetime, Refused, Renewed};

#[cfg(test)]
pub(crate) use file::ABANDONED_AFTER;
pub(crate) use file::{claim, forget_at, load_at, path, remove_held, save_at, write_held};
pub(crate) use wire::{REPLY_LIMIT, agent, class_of, ends_the_sign_in, error_code, form, send};

use crate::Secret;

/// How a request to the authorization server failed, in the categories the backends use for any
/// other service's failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The server refused the request, or answered it with a redirect.
    Refused,
    /// The server is limiting requests.
    RateLimited,
    /// The server is busy or unavailable.
    Unavailable,
    /// The reply was not the shape the exchange records.
    Incomplete,
    /// The request did not complete.
    Transport,
}

/// A short name for the class, for a log or a test.
impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Refused => "refused",
            Self::RateLimited => "rate-limited",
            Self::Unavailable => "unavailable",
            Self::Incomplete => "incomplete",
            Self::Transport => "transport",
        })
    }
}

/// One sign-in whose tokens a file keeps and a [`Renewer`] replaces.
///
/// A sign-in states what is stored, where, and what presenting its refresh token to its token
/// endpoint looks like. The file, the claim it is written under, the memory of a renewal the file
/// could not take and the memory of a sign-in that ended are the same for every kind, so a second
/// sign-in adds a kind and not a copy of them.
pub trait Kind {
    /// What the file holds: the two tokens, the instant the access token expires, and whatever else
    /// the sign-in needs to present them.
    type Account: Clone;
    /// The destinations a refresh goes to.
    type Hosts;
    /// The file the sign-in is kept in, in the state directory.
    const FILE: &'static str;
    /// The claim taken while the file is written or removed, in the same directory.
    const CLAIM: &'static str;

    /// The text of the file for `account`. It holds both tokens, so it is a secret of its own.
    fn encode(account: &Self::Account) -> Secret;
    /// The account the text of a file stands for.
    ///
    /// A text that is not what this version writes is an error that names the defect and quotes
    /// none of it.
    fn decode(raw: &str) -> Result<Self::Account, AccountError>;
    /// What a request is sent with.
    fn access(account: &Self::Account) -> &Secret;
    /// What mints another access token.
    fn refresh(account: &Self::Account) -> &Secret;
    /// The instant the access token expires, in seconds since 1970-01-01.
    fn expires_at(account: &Self::Account) -> u64;
    /// `current` with the tokens a refresh gave. `refresh` is `None` where the server did not
    /// replace the refresh token, which keeps the one `current` holds.
    fn replace(
        current: &Self::Account,
        access: Secret,
        refresh: Option<Secret>,
        expires_at: u64,
    ) -> Self::Account;
    /// Present the refresh token of `account` to the token endpoint of `hosts`.
    fn renew(
        agent: &ureq::Agent,
        hosts: &Self::Hosts,
        account: &Self::Account,
    ) -> Result<Renewed, Refused>;
}
