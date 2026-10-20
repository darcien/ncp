//! The scopes of a login request, from MCP 2026-07-28,
//! basic/authorization:
//! - "Scope Selection Strategy" gives the initial selection.
//! - "Step-Up Authorization Flow" adds the scopes of the last request and
//!   of a `403 insufficient_scope`.
//!
//! The granted scopes of the old tokens never go into a request. On the
//! consent page, the user can grant more than the request. A new request
//! must not ask for all of it again by default.

use rmcp::transport::WWWAuthenticateParams;
use serde_json::Value;

const OFFLINE_ACCESS: &str = "offline_access";

/// The `scope` of a `WWW-Authenticate` challenge, or `None` when the
/// challenge has no scope. The spec makes it authoritative for the current
/// operation.
#[must_use]
pub fn challenged(challenge: &str, server_url: &str) -> Option<Vec<String>> {
    let base = url::Url::parse(server_url).ok()?;
    let scopes: Vec<String> = WWWAuthenticateParams::parse(challenge, &base)
        .scope?
        .split_whitespace()
        .map(str::to_string)
        .collect();
    (!scopes.is_empty()).then_some(scopes)
}

/// Adds `offline_access` when the authorization server lists it, so that
/// the server can give a refresh token (MCP 2026-07-28,
/// basic/authorization, "Refresh Tokens").
/// A request with no scopes stays with no scopes.
#[must_use]
pub fn with_offline_access(scopes: Vec<String>, supported: Option<&[String]>) -> Vec<String> {
    let listed = supported.is_some_and(|supported| supported.iter().any(|s| s == OFFLINE_ACCESS));
    if listed && !scopes.is_empty() && !scopes.iter().any(|s| s == OFFLINE_ACCESS) {
        return scopes.into_iter().chain([OFFLINE_ACCESS.into()]).collect();
    }
    scopes
}

/// The scopes of a login: the initial selection, the scopes that the last
/// login requested, and the scopes of a `403` since then. A new login is a
/// re-authorization, and the spec keeps the earlier scopes in it. Thus the
/// user does not lose a permission that a step-up gave.
#[must_use]
pub fn for_login(
    initial: &[String],
    last_requested: &[String],
    required: &[String],
) -> Vec<String> {
    union(&union(initial, last_requested), required)
}

/// The scopes of a JSON array of the tokens item. Other values give none.
#[must_use]
pub fn list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| scope.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The scopes of both lists, each one once, in the order of the lists.
#[must_use]
pub fn union(first: &[String], second: &[String]) -> Vec<String> {
    first
        .iter()
        .chain(second)
        .fold(Vec::new(), |mut all, scope| {
            if !all.contains(scope) {
                all.push(scope.clone());
            }
            all
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scopes(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn reads_the_scope_of_a_challenge() {
        let url = "https://mcp.test/mcp";
        let challenge = r#"Bearer resource_metadata="https://mcp.test/.well-known/oauth-protected-resource/mcp", scope="user:read account:read""#;
        assert_eq!(
            challenged(challenge, url),
            Some(scopes("user:read account:read"))
        );
        assert_eq!(challenged(r#"Bearer realm="OAuth""#, url), None);
        assert_eq!(challenged(r#"Bearer scope="""#, url), None);
    }

    #[test]
    fn adds_offline_access_only_when_the_server_lists_it() {
        let supported = scopes("offline_access a b");
        assert_eq!(
            with_offline_access(scopes("a"), Some(&supported)),
            scopes("a offline_access")
        );
        assert_eq!(with_offline_access(scopes("a"), None), scopes("a"));
        assert_eq!(
            with_offline_access(scopes("offline_access a"), Some(&supported)),
            scopes("offline_access a")
        );
        assert_eq!(
            with_offline_access(Vec::new(), Some(&supported)),
            scopes("")
        );
    }

    #[test]
    fn a_second_login_asks_again_for_the_same_scopes() {
        let initial = scopes("user:read account:read offline_access");
        assert_eq!(for_login(&initial, &initial, &[]), initial);
        assert_eq!(for_login(&initial, &[], &[]), initial);
    }

    #[test]
    fn a_login_keeps_the_last_request_and_adds_the_scopes_of_a_403() {
        let initial = scopes("a offline_access");
        assert_eq!(
            for_login(&initial, &scopes("a b"), &scopes("c b")),
            scopes("a offline_access b c")
        );
        assert_eq!(
            for_login(&initial, &[], &scopes("c")),
            scopes("a offline_access c")
        );
    }

    #[test]
    fn reads_a_scope_list_and_ignores_other_values() {
        assert_eq!(list(Some(&json!(["a", 1, "b"]))), scopes("a b"));
        assert_eq!(list(Some(&json!("a b"))), scopes(""));
        assert_eq!(list(None), scopes(""));
    }
}
