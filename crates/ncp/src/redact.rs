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

/// OAuth fields and form parameters that hold a token, a code or a client
/// secret. The names are standard (RFC 6749, 7591, 7636, 8693), so the list
/// is exact: a part such as `token` would also hide `token_type` and the
/// endpoints of the metadata.
const OAUTH_SECRET_FIELDS: [&str; 12] = [
    "access_token",
    "refresh_token",
    "id_token",
    "client_secret",
    "code",
    "code_verifier",
    "registration_access_token",
    "client_assertion",
    "assertion",
    "subject_token",
    "actor_token",
    "password",
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
/// The rest of the text stays as it is, also a `${NAME}` of the
/// configuration, so that the URL shows as the user wrote it. A text that
/// is not a URL is hidden as a whole.
#[must_use]
pub fn url(text: &str) -> String {
    let Some((scheme, rest)) = text.split_once("://") else {
        return hide(text).to_string();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let authority = match authority.rsplit_once('@') {
        Some((_, host)) => format!("{HIDDEN}@{host}"),
        None => authority.to_string(),
    };
    let (before_fragment, fragment) = match tail.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (tail, None),
    };
    let tail = match before_fragment.split_once('?') {
        Some((path, query)) => format!("{path}?{}", hide_query(query)),
        None => before_fragment.to_string(),
    };
    let fragment = fragment.map(|f| format!("#{f}")).unwrap_or_default();
    format!("{scheme}://{authority}{tail}{fragment}")
}

/// The query with the value of each secret parameter hidden.
fn hide_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => {
                let decoded: String = url::form_urlencoded::parse(name.as_bytes())
                    .map(|(name, _)| name.into_owned())
                    .collect();
                if is_secret_name(&decoded) {
                    format!("{name}={}", hide(value))
                } else {
                    pair.to_string()
                }
            }
            None => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// True when the OAuth field or form parameter holds a secret.
#[must_use]
pub fn is_oauth_secret(name: &str) -> bool {
    OAUTH_SECRET_FIELDS.contains(&name)
}

/// The form body (`application/x-www-form-urlencoded`) with each secret
/// value hidden.
#[must_use]
pub fn form(body: &str) -> String {
    url::form_urlencoded::parse(body.as_bytes())
        .fold(
            url::form_urlencoded::Serializer::new(String::new()),
            |mut form, (name, value)| {
                let shown = if is_oauth_secret(&name) {
                    hide(&value)
                } else {
                    &value
                };
                form.append_pair(&name, shown);
                form
            },
        )
        .finish()
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
                        Value::String(text) if is_oauth_secret(name) => {
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
        assert_eq!(url("http://:pass@p.test:3128"), "http://****@p.test:3128");
        assert_eq!(
            url("http://h:1/mcp/${KEY}?q=a%20b&key=${KEY}#f"),
            "http://h:1/mcp/${KEY}?q=a%20b&key=****#f"
        );
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
    fn form_hides_codes_verifiers_tokens_and_client_secrets() {
        assert_eq!(
            form(
                "grant_type=authorization_code&code=c&code_verifier=v&client_id=id&client_secret=s&redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcallback"
            ),
            "grant_type=authorization_code&code=****&code_verifier=****&client_id=id&client_secret=****&redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcallback"
        );
        assert_eq!(
            form("refresh_token=r&resource=https%3A%2F%2Fa"),
            "refresh_token=****&resource=https%3A%2F%2Fa"
        );
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
