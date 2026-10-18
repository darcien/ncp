//! The fixes for errors of the configuration. The error types say only
//! what is wrong. The command line gives the fix as a hint, as
//! `hints_for_error` of uv does.

use super::{
    outcome::{Failure, Hint},
    render::shell_word,
};
use crate::{
    config::EntryError,
    expand::ExpandError,
    server::UrlError,
    store::{StoreError, StoreFix},
};

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
        EntryError::Variable {
            error: ExpandError::BadReference(_),
            ..
        } => "ncp supports only `${NAME}` and `${env:NAME}`",
        // The text of these errors names the field and the problem, and
        // the fix is clear from it.
        EntryError::Name(_)
        | EntryError::NotObject
        | EntryError::NoTransport
        | EntryError::UnknownType(_)
        | EntryError::Missing(_)
        | EntryError::WrongType { .. }
        | EntryError::Url(UrlError::Invalid(_) | UrlError::Scheme | UrlError::NoHost)
        | EntryError::Variable { .. }
        | EntryError::WithoutClientId(_)
        | EntryError::IssuersDisagree
        | EntryError::BadClientSecret => return Vec::new(),
    };
    vec![Hint::text(hint)]
}

/// The failure with the hints for its entry error added.
#[must_use]
pub fn with_entry_hints(failure: Failure, error: &EntryError) -> Failure {
    entry_hints(error).into_iter().fold(failure, Failure::hint)
}

/// The hints for a store error.
#[must_use]
pub fn store_hints(error: &StoreError) -> Vec<Hint> {
    error.fix().iter().map(fix_hint).collect()
}

/// The hint for the fix of a store error.
#[must_use]
pub fn fix_hint(fix: &StoreFix) -> Hint {
    match fix {
        StoreFix::CloseFile(path) => Hint::run_text(
            "close the file to other users",
            &format!("chmod 600 {}", shell_word(&path.display().to_string())),
        ),
        StoreFix::AnswerPrompt => Hint::text(
            "If the store shows an unlock prompt, answer it. Then run the command again.",
        ),
        StoreFix::UnlockKeyring => Hint::text("To list the locked items, unlock the keyring"),
        StoreFix::RemoveCopies => {
            Hint::text("Remove the copies of the item in the keyring, and keep one")
        }
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

    #[test]
    fn a_bad_reference_gives_the_forms_that_ncp_supports() {
        let error = EntryError::Variable {
            field: "headers.Authorization".into(),
            error: ExpandError::BadReference(crate::expand::BadReference::Prefix),
        };
        assert_eq!(
            entry_hints(&error),
            [Hint::text("ncp supports only `${NAME}` and `${env:NAME}`")]
        );
    }
}
