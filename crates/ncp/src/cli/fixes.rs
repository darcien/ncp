//! The fixes for errors of the configuration. The error types say only
//! what is wrong. The command line gives the fix as a hint, as
//! `hints_for_error` of uv does.

use super::outcome::Hint;
use crate::{config::EntryError, server::UrlError};

/// The hints for an entry that is not valid, in the order to try them.
#[must_use]
pub fn entry_hints(error: &EntryError) -> Vec<Hint> {
    let hint = match error {
        EntryError::UrlWithoutType => {
            "Add `\"type\": \"http\"` or `\"type\": \"sse\"` to the entry"
        }
        EntryError::Url(UrlError::PlainHttp) => "Use an `https` URL",
        EntryError::Url(UrlError::Credentials) => {
            "Remove the user name and the password from the URL, and keep the secret in the credential store"
        }
        EntryError::SecretWithoutIssuer { .. } => {
            "Set `oauth.issuer` to the issuer of the authorization server where you registered the client"
        }
        _ => return Vec::new(),
    };
    vec![Hint::text(hint)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fix_that_the_error_text_gave_before_is_a_hint() {
        let hints = entry_hints(&EntryError::Url(UrlError::PlainHttp));
        assert_eq!(hints, [Hint::text("Use an `https` URL")]);
        assert_eq!(entry_hints(&EntryError::NoTransport), []);
    }
}
