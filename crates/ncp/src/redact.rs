//! Hidden values in diagnostic output.
//!
//! Verbose output must not show a secret (requirements S3 and I6). As uv
//! does, a hidden value shows as `****`, always with the same length. An
//! empty value shows as empty. Thus the user can see if a value is set or
//! empty, but never its length.

use serde_json::Value;

/// The text of a hidden value.
pub const HIDDEN: &str = "****";

/// Header names that always hold a credential.
const SECRET_HEADERS: [&str; 4] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
];

/// Parts of a query parameter name that show a secret value, for example
/// `token`, `api_key` or `client_secret`. A name that holds one of them is
/// hidden. To hide too much is safe; to show a key is not.
const SECRET_NAME_PARTS: [&str; 7] = [
    "token",
    "secret",
    "password",
    "passwd",
    "key",
    "signature",
    "credential",
];

/// Query parameter names that are secret only as a full name. As a part,
/// they occur in too many common names.
const SECRET_NAMES: [&str; 4] = ["code", "sig", "pwd", "auth"];

/// OAuth fields that hold a token or a client secret.
const OAUTH_SECRET_FIELDS: [&str; 8] = [
    "access_token",
    "refresh_token",
    "id_token",
    "client_secret",
    "code",
    "code_verifier",
    "registration_access_token",
    "client_assertion",
];

/// The value as it shows: `****`, or empty when it is empty.
#[must_use]
pub fn hide(value: &str) -> &'static str {
    if value.is_empty() { "" } else { HIDDEN }
}

/// True when the query parameter name shows a secret value.
#[must_use]
pub fn is_secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_NAMES.contains(&name.as_str())
        || SECRET_NAME_PARTS.iter().any(|part| name.contains(part))
}

/// The header value as it shows. `sensitive` is true for a value that came
/// from `${NAME}`.
#[must_use]
pub fn header(name: &str, value: &str, sensitive: bool) -> String {
    if sensitive || SECRET_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
        hide(value).to_string()
    } else {
        value.to_string()
    }
}

/// The URL with its user information and its secret query values hidden.
/// A text that is not a URL is hidden as a whole.
#[must_use]
pub fn url(text: &str) -> String {
    let Ok(mut url) = url::Url::parse(text) else {
        return hide(text).to_string();
    };
    if !url.username().is_empty() || url.password().is_some() {
        // Both calls fail only for a URL with no host, which has no user
        // information.
        let _ = url.set_password(None);
        let _ = url.set_username(HIDDEN);
    }
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    if pairs.iter().any(|(name, _)| is_secret_name(name)) {
        url.query_pairs_mut()
            .clear()
            .extend_pairs(pairs.iter().map(|(name, value)| {
                let shown = if is_secret_name(name) {
                    hide(value)
                } else {
                    value.as_str()
                };
                (name.as_str(), shown)
            }));
    }
    url.to_string()
}

/// The OAuth message with each token and client secret hidden, at any
/// depth.
#[must_use]
pub fn oauth_json(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, field)| {
                    let shown = match field {
                        Value::String(text) if OAUTH_SECRET_FIELDS.contains(&name.as_str()) => {
                            Value::String(hide(text).to_string())
                        }
                        other => oauth_json(other),
                    };
                    (name.clone(), shown)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(oauth_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hidden_value_has_one_length_and_empty_stays_empty() {
        assert_eq!(hide("a"), "****");
        assert_eq!(hide("a much longer secret value"), "****");
        assert_eq!(hide(""), "");
    }

    #[test]
    fn credential_headers_and_values_from_variables_are_hidden() {
        assert_eq!(header("Authorization", "Bearer t", false), "****");
        assert_eq!(header("proxy-authorization", "Basic x", false), "****");
        assert_eq!(header("Cookie", "a=b", false), "****");
        assert_eq!(header("X-Api", "k", true), "****");
        assert_eq!(header("X-Org", "acme", false), "acme");
        assert_eq!(header("X-Api", "", true), "");
    }

    #[test]
    fn url_hides_user_information_and_secret_query_values() {
        assert_eq!(
            url("https://user:pass@example.com/mcp?api_key=k1&page=2&Token=t&code=c"),
            "https://****@example.com/mcp?api_key=****&page=2&Token=****&code=****"
        );
        assert_eq!(url("http://:pass@p.test:3128"), "http://****@p.test:3128/");
        assert_eq!(
            url("https://example.com/mcp?q=a"),
            "https://example.com/mcp?q=a"
        );
        assert_eq!(
            url("https://example.com/mcp?token="),
            "https://example.com/mcp?token="
        );
        assert_eq!(url("not a url"), "****");
    }

    #[test]
    fn oauth_json_hides_tokens_and_client_secrets_at_any_depth() {
        let shown = oauth_json(&json!({
            "access_token": "a", "token_type": "Bearer", "refresh_token": "",
            "client": {"client_id": "id", "client_secret": "s"},
            "list": [{"id_token": "i"}], "expires_in": 3600
        }));
        assert_eq!(
            shown,
            json!({
                "access_token": "****", "token_type": "Bearer", "refresh_token": "",
                "client": {"client_id": "id", "client_secret": "****"},
                "list": [{"id_token": "****"}], "expires_in": 3600
            })
        );
    }
}
