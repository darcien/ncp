//! The fixes for errors of the configuration. The error types say only
//! what is wrong. The command line gives the fix as a hint, as
//! `hints_for_error` of uv does.

use super::{outcome::Hint, render::shell_word};
use crate::{config::EntryError, server::UrlError, store::StoreError};

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

/// The hints for a store error.
#[must_use]
pub fn store_hints(error: &StoreError) -> Vec<Hint> {
    match error {
        StoreError::OpenPermissions(path) => vec![Hint::run_text(
            "close the file to other users",
            &format!("chmod 600 {}", shell_word(&path.display().to_string())),
        )],
        // A locked store waits for its unlock prompt.
        StoreError::NoAnswer(_) => vec![Hint::text(
            "If the store shows an unlock prompt, answer it. Then run the command again.",
        )],
        StoreError::Locked(_) => vec![Hint::text("To list the locked items, unlock the keyring")],
        _ => Vec::new(),
    }
}

/// The hint for a name that is not a secret name.
#[must_use]
pub fn secret_name_hint() -> Hint {
    Hint::text("Use letters, digits and `_`, and do not start with a digit")
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
