//! Hidden values in diagnostic output.
//!
//! Verbose output must not show a secret, because logs and screens are often
//! shared. It hides each value that `ncp` knows is a secret (design B43). It
//! does not guess from a name, because a guess hides some secrets and not
//! others, and the user cannot know which. As in uv, a hidden value shows as
//! `****`, always with the same length. An empty value shows as empty. Thus the
//! user can see if a value exists or is empty, but never its length.

use std::{
    cmp::Reverse,
    fmt::Write as _,
    sync::{Arc, PoisonError, RwLock},
};

use serde_json::Value;

pub const HIDDEN: &str = "****";

/// Header fields that RFC 9110 defines as credentials: `Authorization`
/// (§11.6.2) and `Proxy-Authorization` (§11.7.2). No other name: a longer
/// list is a guess.
const SECRET_HEADERS: [&str; 2] = ["authorization", "proxy-authorization"];

/// OAuth fields and form parameters that hold a token, a code or a client
/// secret. The names are standard (RFC 6749, 7591, 7636, 8693), so the list is
/// exact. A part such as `token` also hides `token_type` and the endpoints of
/// the metadata.
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

/// True for a header field that RFC 9110 defines as credentials. Field
/// names have no case (RFC 9110 §5.1).
#[must_use]
pub fn is_credential_header(name: &str) -> bool {
    SECRET_HEADERS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

/// A credential header value, and its credentials after the scheme
/// (RFC 9110 §11.4), for example the token of `Bearer TOKEN`. A server can
/// echo the token alone.
#[must_use]
pub fn credentials(value: &str) -> Vec<String> {
    let after_scheme = value
        .split_once(' ')
        .map(|(_, credentials)| credentials.trim().to_string());
    std::iter::once(value.to_string())
        .chain(after_scheme)
        .collect()
}

/// The header value as it shows. `sensitive` is true for a value that came
/// from `${NAME}`.
#[must_use]
pub fn header(name: &str, value: &str, sensitive: bool) -> String {
    if sensitive || is_credential_header(name) {
        hide(value).to_string()
    } else {
        value.to_string()
    }
}

/// The URL with its user information and its OAuth codes and tokens hidden. The
/// rest of the text stays as it is, also a `${NAME}` of the configuration, so
/// that the URL shows as the user wrote it. The function hides a text that is
/// not a URL completely.
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

// The query with the value of each OAuth secret parameter hidden, for
// example the `code` of a redirect.
fn hide_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => {
                let decoded: String = url::form_urlencoded::parse(name.as_bytes())
                    .map(|(name, _)| name.into_owned())
                    .collect();
                if is_oauth_secret(&decoded) {
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

/// The known secret values of one server in one run, in the forms that
/// output hides (design B81).
///
/// The list grows during the run, and each clone sees each new value:
/// rmcp reads and refreshes the OAuth tokens inside the connection, after
/// the trace of the connection starts. The list never gets smaller, so a
/// text that was hidden once stays hidden.
#[derive(Clone, Default)]
pub struct Known(Arc<RwLock<Vec<String>>>);

impl Known {
    pub fn add(&self, values: impl IntoIterator<Item = String>) {
        let mut forms = self.0.write().unwrap_or_else(PoisonError::into_inner);
        forms.extend(
            values
                .into_iter()
                .filter(|value| !value.is_empty())
                .flat_map(|value| forms_of(&value)),
        );
        // The longest first, so that a value inside another one cannot
        // leave a part of it.
        forms.sort_by(|a, b| (Reverse(a.len()), a).cmp(&(Reverse(b.len()), b)));
        forms.dedup();
    }

    /// The text with each form of each known value hidden.
    #[must_use]
    pub fn hide(&self, text: &str) -> String {
        let forms = self.0.read().unwrap_or_else(PoisonError::into_inner);
        forms
            .iter()
            .fold(text.to_string(), |text, form| text.replace(form, HIDDEN))
    }
}

impl std::fmt::Debug for Known {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Known(<hidden>)")
    }
}

/// Hides each value, also in its encoded forms. A URL that a library makes
/// from the server URL can hold a value of `${NAME}` in another place or
/// encoding.
#[must_use]
pub fn values(text: &str, secrets: &[String]) -> String {
    let known = Known::default();
    known.add(secrets.iter().cloned());
    known.hide(text)
}

// The forms of one value that output hides (design B81). Each form comes
// from an encoder that `ncp`, its libraries or a common server uses. A
// form encoding of each form covers a URL that holds the value and goes
// into a form again, for example the `resource` of OAuth.
fn forms_of(value: &str) -> Vec<String> {
    let percent = percent_encoded(value);
    let quoted = serde_json::to_string(value).unwrap_or_default();
    let json = quoted
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(value);
    let base = [
        value.to_string(),
        // A URL parser lowercases a host.
        value.to_lowercase(),
        // A server that reads a query as a form makes a `+` a space.
        value.replace('+', " "),
        percent.to_lowercase(),
        percent,
        // PHP `json_encode` writes `/` as `\/`.
        json.replace('/', "\\/"),
        json.to_string(),
    ]
    .into_iter()
    .chain(url_forms(value));
    base.flat_map(|form| {
        let encoded = url::form_urlencoded::byte_serialize(form.as_bytes()).collect();
        [form, encoded]
    })
    .collect()
}

// The value as the `url` crate writes it in a path, a query and a
// fragment. Each part keeps other characters, for example `/` in a path.
// The setters encode `?` and `#`, which a parse of a whole URL would take
// as the start of the next part.
fn url_forms(value: &str) -> Vec<String> {
    let Ok(mut url) = url::Url::parse("https://h.invalid/") else {
        return Vec::new();
    };
    url.set_path(value);
    url.set_query(Some(value));
    url.set_fragment(Some(value));
    [
        url.path().trim_start_matches('/'),
        url.query().unwrap_or_default(),
        url.fragment().unwrap_or_default(),
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// The tokens and client secrets in an OAuth message, at any depth. Output
/// hides them where a server echoes them.
#[must_use]
pub fn oauth_values(value: &Value) -> Vec<String> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(name, field)| match field {
                Value::String(text) if is_oauth_secret(name) => vec![text.clone()],
                other => oauth_values(other),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(oauth_values).collect(),
        _ => Vec::new(),
    }
}

/// The JSON value with each string hidden by `hide`. The JSON stays valid,
/// so a trace of a JSON-RPC message stays a JSON object.
#[must_use]
pub fn json_strings(value: &Value, hide: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::String(text) => Value::String(hide(text)),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, field)| (name.clone(), json_strings(field, hide)))
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| json_strings(item, hide)).collect())
        }
        other => other.clone(),
    }
}

// Each byte that is not an unreserved URL character (RFC 3986 §2.3) as
// `%XX`.
fn percent_encoded(value: &str) -> String {
    value.bytes().fold(String::new(), |mut text, byte| {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            text.push(char::from(byte));
        } else {
            let _ = write!(text, "%{byte:02X}");
        }
        text
    })
}

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
    fn rfc_credential_headers_and_values_from_variables_are_hidden() {
        assert_eq!(header("Authorization", "Bearer t", false), "****");
        assert_eq!(header("proxy-authorization", "Basic x", false), "****");
        assert_eq!(header("Cookie", "a=b", false), "a=b");
        assert_eq!(header("X-Api", "k", true), "****");
        assert_eq!(header("X-Org", "acme", false), "acme");
        assert_eq!(header("X-Api", "", true), "");
    }

    #[test]
    fn url_hides_user_information_and_oauth_secrets_and_guesses_nothing_from_a_name() {
        assert_eq!(
            url("https://user:pass@example.com/mcp?api_key=k1&page=2&Token=t&code=c"),
            "https://****@example.com/mcp?api_key=k1&page=2&Token=t&code=****"
        );
        assert_eq!(url("http://:pass@p.test:3128"), "http://****@p.test:3128");
        assert_eq!(
            url("http://h:1/mcp/${KEY}?q=a%20b&key=${KEY}#f"),
            "http://h:1/mcp/${KEY}?q=a%20b&key=${KEY}#f"
        );
        assert_eq!(
            url("https://example.com/mcp?q=a"),
            "https://example.com/mcp?q=a"
        );
        assert_eq!(
            url("https://example.com/cb?access_token=&state=s"),
            "https://example.com/cb?access_token=&state=s"
        );
        assert_eq!(url("not a url"), "****");
    }

    #[test]
    fn values_hide_each_value_also_when_percent_encoded() {
        let secrets = vec!["k 1/x".to_string(), String::new()];
        assert_eq!(
            values("a=k 1/x b=k+1%2Fx c=k%201%2Fx d", &secrets),
            "a=**** b=**** c=**** d"
        );
        let host = vec!["AcmeX".to_string()];
        assert_eq!(
            values("https://acmex.example.com", &host),
            "https://****.example.com"
        );
        let path = vec!["a b".to_string()];
        assert_eq!(
            values("resource=https%3A%2F%2Fh%2Fa%2520b", &path),
            "resource=https%3A%2F%2Fh%2F****"
        );
    }

    #[test]
    fn values_hide_each_form_of_the_url_encoders_and_of_json() {
        let hidden = |value: &str, text: &str| values(text, &[value.to_string()]);
        // The `url` crate keeps `/` in a path and encodes the space.
        assert_eq!(
            hidden("SEC RET/KEY", "404 for /SEC%20RET/KEY"),
            "404 for /****"
        );
        // The same path in the `resource` of an OAuth form.
        assert_eq!(
            hidden("SEC RET/KEY", "resource=h%2FSEC%2520RET%2FKEY"),
            "resource=h%2F****"
        );
        assert_eq!(hidden("SEC'RET", "q?k=SEC%27RET"), "q?k=****");
        assert_eq!(hidden("ü/x", "/%C3%BC/x"), "/****");
        assert_eq!(hidden("a?b#c", "/a%3Fb%23c"), "/****");
        // A server that reads the query as a form.
        assert_eq!(hidden("ab+c/d==", "bad key ab c/d=="), "bad key ****");
        assert_eq!(hidden("a/b\"c", r#"{"e":"a\/b\"c"}"#), r#"{"e":"****"}"#);
        assert_eq!(hidden("a\nb", r#"{"e":"a\nb"}"#), r#"{"e":"****"}"#);
    }

    #[test]
    fn values_do_not_hide_forms_outside_the_list() {
        // Base64 of `k3y-value`, an HTML entity and a part of the value.
        let secrets = ["k3y-value".to_string()];
        let text = "azN5LXZhbHVl k3y&#45;value k3y-val";
        assert_eq!(values(text, &secrets), text);
    }

    #[test]
    fn known_values_reach_each_clone_and_debug_shows_none() {
        let known = Known::default();
        let copy = known.clone();
        assert_eq!(copy.hide("a k1 b"), "a k1 b");
        known.add(["k1".to_string(), String::new()]);
        assert_eq!(copy.hide("a k1 b"), "a **** b");
        assert_eq!(copy.hide(""), "");
        assert_eq!(format!("{known:?}"), "Known(<hidden>)");
    }

    #[test]
    fn a_credential_header_gives_its_value_and_its_credentials() {
        assert_eq!(credentials("Bearer t0k"), ["Bearer t0k", "t0k"]);
        assert_eq!(credentials("t0k"), ["t0k"]);
    }

    #[test]
    fn oauth_values_are_the_tokens_and_secrets_at_any_depth() {
        let message = json!({
            "client_id": "id", "token_response": {"access_token": "a", "token_type": "Bearer",
            "refresh_token": "r"}, "list": [{"client_secret": "s"}], "expires_in": 3600
        });
        let mut values = oauth_values(&message);
        values.sort();
        assert_eq!(values, ["a", "r", "s"]);
    }

    #[test]
    fn json_strings_hides_each_string_and_keeps_the_shape() {
        let message = json!({"id": 1, "result": {"text": "k1", "list": ["k1", true]}});
        let shown = json_strings(&message, &|text| text.replace("k1", HIDDEN));
        assert_eq!(
            shown,
            json!({"id": 1, "result": {"text": "****", "list": ["****", true]}})
        );
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
