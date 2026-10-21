//! The stored OAuth items of a server.
//!
//! Each server has two items: the tokens
//! (`oauth/<server>/tokens`) and the client credentials
//! (`oauth/<server>/client`). The tokens item is the JSON of rmcp
//! `StoredCredentials`, because rmcp reads it back. The client item records
//! the issuer that registered the client. A client of one authorization
//! server is not valid at another one, so a new issuer causes a new
//! registration.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::credential::Secret;

/// The field of the tokens item that keeps the scopes of a `403
/// insufficient_scope` answer until the next login. rmcp ignores it.
pub const REQUIRED_SCOPES: &str = "ncp_required_scopes";

/// The field of the tokens item that keeps the scopes that the login
/// requested, for the union of a step-up. rmcp ignores it.
pub const REQUESTED_SCOPES: &str = "ncp_requested_scopes";

pub const SCOPE_FIELDS: [&str; 2] = [REQUIRED_SCOPES, REQUESTED_SCOPES];

/// The field of the tokens item that keeps the binding of the tokens. rmcp
/// ignores it.
pub const BINDING: &str = "ncp_binding";

/// The server that a token set is for: the canonical URL of the MCP server
/// (the RFC 8707 resource) and the issuer of the tokens. The server name is
/// only the storage key, so an entry that changes its URL keeps its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBinding {
    pub resource: String,
    pub issuer: String,
}

/// Why `ncp` does not send a stored token. It holds no stored value: a
/// stored URL or issuer can hold an old `${NAME}` value that `ncp` cannot
/// hide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingMismatch {
    /// The tokens have no binding, for example from an older `ncp`.
    Missing,
    Resource,
    /// The tokens are from another issuer than `current`.
    Issuer {
        current: String,
    },
}

/// Compares the binding of the stored tokens with the current entry and
/// metadata. A token for another server must not go to this one.
///
/// # Errors
///
/// Returns the first value that differs, or `Missing`.
pub fn check_binding(
    stored: Option<&TokenBinding>,
    current: &TokenBinding,
) -> Result<(), BindingMismatch> {
    let stored = stored.ok_or(BindingMismatch::Missing)?;
    if stored.resource != current.resource {
        return Err(BindingMismatch::Resource);
    }
    if stored.issuer != current.issuer {
        return Err(BindingMismatch::Issuer {
            current: current.issuer.clone(),
        });
    }
    Ok(())
}

/// The client credentials that an authorization server gave to `ncp`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientItem {
    pub issuer: String,
    pub client_id: String,
    /// rmcp keeps a client secret of the registration only in memory
    /// (rmcp issue #784). `ncp` keeps it here, so that a refresh can use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

impl fmt::Debug for ClientItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientItem")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<hidden>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("The stored {item} item is not valid")]
pub struct ItemError {
    pub item: &'static str,
    #[source]
    pub reason: ItemReason,
}

/// The kind of the problem of an item. It has no text of the JSON parser,
/// because the parser quotes a string value, and an item holds secrets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ItemReason(&'static str);

impl ItemReason {
    /// The problem of an item that `ncp` reads.
    #[must_use]
    pub fn of(error: &serde_json::Error) -> Self {
        Self(match error.classify() {
            serde_json::error::Category::Syntax => "The item is not valid JSON",
            serde_json::error::Category::Eof => "The JSON of the item is not complete",
            serde_json::error::Category::Data => "The JSON does not have the fields of the item",
            serde_json::error::Category::Io => "ncp cannot read the JSON of the item",
        })
    }

    /// The problem of an item that `ncp` writes.
    const WRITE: Self = Self("ncp cannot write the item as JSON");
}

impl ClientItem {
    /// The item as a secret: compact JSON, because the macOS Keychain
    /// through `security` keeps a long value in more items.
    ///
    /// # Errors
    ///
    /// Returns an error when `serde_json` cannot write the item.
    pub fn to_secret(&self) -> Result<Secret, ItemError> {
        to_secret("client", self)
    }

    /// # Errors
    ///
    /// Returns an error when the secret is not a client item.
    pub fn from_secret(secret: &Secret) -> Result<Self, ItemError> {
        serde_json::from_str(secret.expose()).map_err(|error| ItemError {
            item: "client",
            reason: ItemReason::of(&error),
        })
    }

    /// True when this client is for the issuer. A client of another issuer
    /// is not valid there.
    #[must_use]
    pub fn is_for(&self, issuer: &str) -> bool {
        self.issuer == issuer
    }
}

/// Compact JSON of a value as a secret.
///
/// # Errors
///
/// Returns an error when `serde_json` cannot write the value.
pub fn to_secret(item: &'static str, value: &impl Serialize) -> Result<Secret, ItemError> {
    serde_json::to_string(value)
        .map(Secret::new)
        .map_err(|_| ItemError {
            item,
            reason: ItemReason::WRITE,
        })
}

/// What `auth status` shows of the tokens of one server. No token value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub logged_in: bool,
    pub issuer: Option<String>,
    pub scopes: Vec<String>,
    /// Seconds since the Unix epoch.
    pub expires_at: Option<u64>,
}

/// The status of the tokens item, as JSON of rmcp `StoredCredentials`.
#[must_use]
pub fn status(tokens: Option<&Value>) -> Status {
    let Some(tokens) = tokens else {
        return Status {
            logged_in: false,
            issuer: None,
            scopes: Vec::new(),
            expires_at: None,
        };
    };
    let response = &tokens["token_response"];
    let granted: Vec<String> = tokens["granted_scopes"]
        .as_array()
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| scope.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let scopes = if granted.is_empty() {
        response["scope"]
            .as_str()
            .map(|scope| scope.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    } else {
        granted
    };
    let expires_at = tokens["token_received_at"]
        .as_u64()
        .zip(response["expires_in"].as_u64())
        .map(|(received, lifetime)| received + lifetime);
    Status {
        logged_in: response["access_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()),
        issuer: tokens["issuer"].as_str().map(str::to_string),
        scopes,
        expires_at,
    }
}

/// The time as RFC 3339 in UTC, for example `2026-10-08T12:00:00Z`.
#[must_use]
pub fn rfc3339(seconds: u64) -> String {
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

// The date of a day count since 1970-01-01, for the proleptic Gregorian
// calendar (the `civil_from_days` algorithm of Howard Hinnant).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn error_texts_follow_the_message_rules() {
        let error = ItemError {
            item: "tokens",
            reason: ItemReason("x"),
        };
        let parse = |text: &str| ItemReason::of(&serde_json::from_str::<Value>(text).unwrap_err());
        let reasons = [parse("{"), parse("x"), ItemReason::WRITE];
        let data = serde_json::from_str::<ClientItem>("{}").unwrap_err();
        let texts: Vec<String> = std::iter::once(error.to_string())
            .chain(reasons.iter().map(ToString::to_string))
            .chain([ItemReason::of(&data).to_string()])
            .collect();
        crate::style::assert_style(&texts);
    }

    #[test]
    fn client_item_round_trips_as_compact_json_and_hides_the_secret_in_debug() {
        let item = ClientItem {
            issuer: "https://as.test".into(),
            client_id: "c1".into(),
            client_secret: Some("s3".into()),
        };
        let secret = item.to_secret().unwrap();
        assert_eq!(
            secret.expose(),
            r#"{"issuer":"https://as.test","clientId":"c1","clientSecret":"s3"}"#
        );
        assert_eq!(ClientItem::from_secret(&secret).unwrap(), item);
        assert!(!format!("{item:?}").contains("s3"));
        assert!(item.is_for("https://as.test") && !item.is_for("https://other.test"));
    }

    #[test]
    fn binding_check_names_the_value_that_differs_and_refuses_no_binding() {
        let binding = |resource: &str, issuer: &str| TokenBinding {
            resource: resource.into(),
            issuer: issuer.into(),
        };
        let current = binding("https://a/mcp", "https://as");
        assert_eq!(check_binding(Some(&current), &current), Ok(()));
        assert_eq!(check_binding(None, &current), Err(BindingMismatch::Missing));
        assert_eq!(
            check_binding(Some(&binding("https://b/mcp", "https://as")), &current),
            Err(BindingMismatch::Resource)
        );
        assert_eq!(
            check_binding(Some(&binding("https://a/mcp", "https://other")), &current),
            Err(BindingMismatch::Issuer {
                current: "https://as".into()
            })
        );
    }

    #[test]
    fn client_item_that_is_not_json_names_the_item() {
        let error = ClientItem::from_secret(&Secret::new("x".into())).unwrap_err();
        assert_eq!(error.item, "client");
    }

    #[test]
    fn status_shows_issuer_scopes_and_expiry_and_no_token() {
        let tokens = json!({
            "client_id": "c1",
            "token_response": {"access_token": "at", "token_type": "bearer",
                "expires_in": 3600, "refresh_token": "rt", "scope": "a b"},
            "granted_scopes": [],
            "token_received_at": 1_790_000_000_u64,
            "issuer": "https://as.test"
        });
        assert_eq!(
            status(Some(&tokens)),
            Status {
                logged_in: true,
                issuer: Some("https://as.test".into()),
                scopes: vec!["a".into(), "b".into()],
                expires_at: Some(1_790_003_600),
            }
        );
        assert!(!status(None).logged_in);
    }

    #[test]
    fn granted_scopes_come_before_the_scope_of_the_response() {
        let tokens = json!({"token_response": {"access_token": "at", "scope": "a"},
            "granted_scopes": ["a", "b"]});
        assert_eq!(status(Some(&tokens)).scopes, ["a", "b"]);
        assert_eq!(status(Some(&tokens)).expires_at, None);
    }

    #[test]
    fn rfc3339_gives_the_utc_date_and_time() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_790_003_661), "2026-09-21T15:14:21Z");
    }
}
