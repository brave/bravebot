//! Replacing an access token before it expires.
//!
//! The stored sign-in is read, and where its access token is within a minute of its expiry the
//! refresh token is presented to the token endpoint under the claim on the file, the reply is
//! written, and only then is the new access token handed out. Every process that signs in or
//! renews takes the same claim and reads the file again once it holds it, so two of them never
//! present one refresh token, which a server that replaces it on use would kill for the second.
//!
//! The file is written here whatever the session is doing: this crate knows no mode that declines
//! a write, and a refresh that was not written would end the sign-in at the next process.

use super::encoding::{now, same};
use super::{AccountError, Class, Kind, Refused, claim, load_at, path, remove_held, write_held};
use crate::Secret;
use std::path::PathBuf;

/// How close to its expiry an access token may be and still be sent.
const MARGIN: u64 = 60;

/// Why no access token could be had. Each is a step and, for a failed request, a class, and none
/// holds text from a reply.
#[derive(Debug)]
pub enum TokenError {
    /// Nothing is stored, and this renewer has not found a sign-in end.
    NotSignedIn,
    /// The authorization server says the refresh token is no longer valid. The stored copy was
    /// removed where it was the one refused. Every later ask of the same renewer says so as well,
    /// until a sign-in is stored again.
    Ended,
    /// The refresh failed in another way and no token in hand could be sent.
    Refresh(Class),
    /// The file could not be read, or the claim on it could not be taken.
    Store(AccountError),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSignedIn => f.write_str("not signed in"),
            Self::Ended => f.write_str("sign-in ended"),
            Self::Refresh(class) => write!(f, "refresh {class}"),
            Self::Store(failure) => write!(f, "store {failure}"),
        }
    }
}

impl std::error::Error for TokenError {}

/// An access token to send, and what could not be stored of the renewal that made it.
///
/// No equality, which the token does not have: see [`Secret`].
#[derive(Debug)]
pub struct Access {
    pub token: Secret,
    /// Set where a renewed token could not be written. The token is good, and this process holds
    /// the only copy of the refresh token that renews it until a later write succeeds.
    pub unwritten: Option<AccountError>,
}

/// Tokens a renewal made and the file could not take.
struct Unwritten<A> {
    /// The refresh token the file held when it was last read: the one this process replaced. The
    /// memory is written over the file only while the file still holds it.
    replaced: Secret,
    account: A,
}

/// Hands out access tokens for the stored sign-in of `K`, replacing one that is about to expire.
pub struct Renewer<K: Kind> {
    hosts: K::Hosts,
    agent: ureq::Agent,
    path: PathBuf,
    unwritten: Option<Unwritten<K::Account>>,
    /// A refresh was refused as the refresh token no longer being valid and the file was removed.
    /// A missing file does not say why it is missing, so this is what tells a later ask that the
    /// sign-in ended, where the file alone would say none was made. Forgotten where a sign-in is
    /// found stored again.
    ended: bool,
    /// Seconds since 1970 where an expiry is made and compared, which is 0 for a clock set before
    /// that. The system clock outside a test.
    now: fn() -> u64,
}

impl<K: Kind> Renewer<K> {
    /// A renewer for the sign-in kept where [`path`] says. `transport` is what the caller has
    /// decided its clients trust and go through.
    pub fn new(
        hosts: K::Hosts,
        transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    ) -> Result<Self, AccountError> {
        Ok(Self::at(hosts, transport, path::<K>()?))
    }

    pub(crate) fn at(
        hosts: K::Hosts,
        transport: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
        path: PathBuf,
    ) -> Self {
        Self {
            hosts,
            agent: super::agent(transport),
            path,
            unwritten: None,
            ended: false,
            now,
        }
    }

    /// This renewer reading the instant from `now`, which a test states.
    #[cfg(test)]
    pub(crate) fn clocked(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }

    /// Whether tokens a renewal made are held in memory because the file could not take them.
    #[cfg(test)]
    pub(crate) fn holds_unwritten_tokens(&self) -> bool {
        self.unwritten.is_some()
    }

    /// The access token to send a request with: the stored one where its expiry is more than a
    /// minute away, and otherwise a new one.
    pub fn access_token(&mut self) -> Result<Access, TokenError> {
        self.obtain(None)
    }

    /// A token other than `refused`, which the serving host turned away before its expiry. It is
    /// the stored one where another process already replaced it, and a new one otherwise. The
    /// caller sends the request once more with it, and does not call this a second time for the
    /// same request.
    pub fn replace_refused(&mut self, refused: &Secret) -> Result<Access, TokenError> {
        self.obtain(Some(refused))
    }

    /// The stored sign-in, read from the file. One found there is not the sign-in that ended, so
    /// finding it forgets the end.
    fn read(&mut self) -> Result<Option<K::Account>, TokenError> {
        let stored = load_at::<K>(&self.path).map_err(TokenError::Store)?;
        if stored.is_some() {
            self.ended = false;
        }
        Ok(stored)
    }

    /// What a file holding no sign-in means: that this process found it end, or that none is stored.
    fn missing(&self) -> TokenError {
        if self.ended {
            TokenError::Ended
        } else {
            TokenError::NotSignedIn
        }
    }

    /// Write what a refresh gave and hand out its access token. Where the file cannot take it, the tokens
    /// are held in memory with the refresh token this process replaced, and the failure is handed out
    /// with the token.
    fn keep(
        &mut self,
        current: &K::Account,
        stored: Option<K::Account>,
        next: K::Account,
    ) -> Access {
        match write_held::<K>(&self.path, &next) {
            Ok(()) => {
                self.unwritten = None;
                Access {
                    token: K::access(&next).clone(),
                    unwritten: None,
                }
            }
            Err(failure) => {
                let replaced = match (self.unwritten.take(), stored) {
                    (Some(held), _) => held.replaced,
                    (None, Some(file)) => K::refresh(&file).clone(),
                    (None, None) => K::refresh(current).clone(),
                };
                let token = K::access(&next).clone();
                self.unwritten = Some(Unwritten {
                    replaced,
                    account: next,
                });
                Access {
                    token,
                    unwritten: Some(failure),
                }
            }
        }
    }

    fn obtain(&mut self, refused: Option<&Secret>) -> Result<Access, TokenError> {
        // Nothing in memory and nothing to replace: no claim is needed to read a good token.
        if self.unwritten.is_none() {
            let stored = self.read()?.ok_or_else(|| self.missing())?;
            if !needs_replacing::<K>(&stored, refused, (self.now)()) {
                return Ok(Access {
                    token: K::access(&stored).clone(),
                    unwritten: None,
                });
            }
        }

        let claimed = claim::<K>(&self.path).map_err(TokenError::Store)?;
        // Read again with the claim held: another process may have replaced the token meanwhile.
        let mut stored = self.read()?;
        let mut failed_write = None;
        if let Some(held) = self.unwritten.take() {
            let still_ours = stored
                .as_ref()
                .is_some_and(|file| same_secret(K::refresh(file), &held.replaced));
            if still_ours {
                match write_held::<K>(&self.path, &held.account) {
                    Ok(()) => stored = Some(held.account.clone()),
                    Err(failure) => failed_write = Some(failure),
                }
                if failed_write.is_some() {
                    self.unwritten = Some(held);
                }
            }
            // Where the file holds another token, another process replaced it or the sign-in is
            // gone, and what memory holds is dropped with `held`.
        }
        let current = match (&self.unwritten, stored.as_ref()) {
            (Some(held), _) => held.account.clone(),
            (None, Some(file)) => file.clone(),
            (None, None) => return Err(self.missing()),
        };
        if !needs_replacing::<K>(&current, refused, (self.now)()) {
            return Ok(Access {
                token: K::access(&current).clone(),
                unwritten: failed_write,
            });
        }

        let mut current = current;
        // What the refresh was refused for once, so a sign-in another process keeps replacing is not
        // chased for ever.
        let mut retried = false;
        loop {
            match K::renew(&self.agent, &self.hosts, &current) {
                Ok(renewed) => {
                    let next = K::replace(
                        &current,
                        renewed.access,
                        renewed.refresh,
                        renewed.lifetime.expiry((self.now)()),
                    );
                    return Ok(self.keep(&current, stored, next));
                }
                Err(Refused::Ended) => {
                    // Memory holds a token that was just refused, so it goes.
                    let replaced = self.unwritten.take().map(|held| held.replaced);
                    // The file is read again immediately before it is removed. A claim is judged
                    // abandoned by its age, so a holder that is slow and alive can have its claim
                    // broken, and the process that broke it presents a refresh token the holder has
                    // already spent. The refusal then says nothing about what the holder wrote, and
                    // removing the file would remove that. The file is removed only while it holds a
                    // refresh token this process knows to be spent: the one just refused, or the one
                    // it replaced before it, which the server killed when it gave the refused one.
                    let found = load_at::<K>(&self.path);
                    if let Ok(Some(file)) = found
                        && !same_secret(K::refresh(&file), K::refresh(&current))
                        && !replaced
                            .as_ref()
                            .is_some_and(|spent| same_secret(K::refresh(&file), spent))
                    {
                        // Another process renewed or signed in, so this is not the sign-in that
                        // ended. Its token is used, and replaced once if it too is due.
                        self.ended = false;
                        if !needs_replacing::<K>(&file, refused, (self.now)()) {
                            return Ok(Access {
                                token: K::access(&file).clone(),
                                unwritten: None,
                            });
                        }
                        if retried {
                            return Err(TokenError::Refresh(Class::Refused));
                        }
                        retried = true;
                        stored = Some(file.clone());
                        current = file;
                        // The first request may have waited as long as a request may, and this one
                        // may too, so the claim is made young again before it. Otherwise it could be
                        // judged abandoned while this request waits, and a third process could
                        // present the token it is about to spend.
                        claimed.touch();
                        continue;
                    }
                    // The sign-in has ended whether or not the file could be removed. A failed
                    // removal is not the answer: the next request meets the same refusal and tries
                    // again.
                    let _ = remove_held(&self.path);
                    // Kept for the rest of the process: the file is gone now, and the next ask,
                    // which may be the turn's after a picker's listing found the end, must not take
                    // that for a sign-in nobody made.
                    self.ended = true;
                    return Err(TokenError::Ended);
                }
                Err(Refused::Failed(class)) => {
                    // A token the host refused is not sent again. One that has merely not been
                    // replaced, or that another process made after the refusal, is good until it
                    // expires.
                    let refused_now =
                        refused.is_some_and(|refused| same_secret(K::access(&current), refused));
                    return if !refused_now && K::expires_at(&current) > (self.now)() {
                        Ok(Access {
                            token: K::access(&current).clone(),
                            unwritten: failed_write,
                        })
                    } else {
                        Err(TokenError::Refresh(class))
                    };
                }
            }
        }
    }
}

/// Whether a request cannot be built from this account's access token: it is within the margin of
/// its expiry, or it is the one the serving host refused.
fn needs_replacing<K: Kind>(account: &K::Account, refused: Option<&Secret>, now: u64) -> bool {
    refused.is_some_and(|refused| same_secret(K::access(account), refused))
        || K::expires_at(account) <= now.saturating_add(MARGIN)
}

fn same_secret(a: &Secret, b: &Secret) -> bool {
    same(a.expose().as_bytes(), b.expose().as_bytes())
}
