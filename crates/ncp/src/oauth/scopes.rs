//! The scopes of a login request (design B79), as MCP 2026-07-28,
//! basic/authorization, selects them: "Scope Selection Strategy" for a new
//! login, and "Step-Up Authorization Flow" after a `403 insufficient_scope`.
//!
//! The granted scopes of the old tokens never go into a request: the user
//! can grant more than the request on the consent page, and a new request
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
/// the server can give a refresh token (MCP 2026-07-28, "Refresh Tokens").
/// A request with no scopes stays with no scopes.
#[must_use]
pub fn with_offline_access(scopes: Vec<String>, supported: Option<&[String]>) -> Vec<String> {
    let listed = supported.is_some_and(|supported| supported.iter().any(|s| s == OFFLINE_ACCESS));
    if listed && !scopes.is_empty() && !scopes.iter().any(|s| s == OFFLINE_ACCESS) {
        union(&scopes, &[OFFLINE_ACCESS.to_string()])
    } else {
        scopes
    }
}

/// The scopes of a login. With no scopes of a `403`, the initial selection.
/// With them, a step-up: the union of the scopes that the last login
/// requested and the scopes of the `403`. Tokens from before the requested
/// scopes were kept use the initial selection in place of the last request.
#[must_use]
pub fn for_login(
    initial: Vec<String>,
    last_requested: &[String],
    required: &[String],
) -> Vec<String> {
    match (required, last_requested) {
        ([], _) => initial,
        (required, []) => union(&initial, required),
        (required, last_requested) => union(last_requested, required),
    }
}

/// The scopes of a JSON array of the tokens item. Other values give none.
#[must_use]
pub fn list(value: &Value) -> Vec<String> {
    value
        .as_array()
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
    fn a_new_login_asks_for_the_initial_selection_only() {
        let initial = scopes("user:read account:read offline_access");
        assert_eq!(
            for_login(initial.clone(), &scopes("user:read account:read"), &[]),
            initial
        );
    }

    #[test]
    fn a_step_up_asks_for_the_last_request_and_the_scopes_of_the_403() {
        let initial = scopes("a offline_access");
        assert_eq!(
            for_login(initial.clone(), &scopes("a b"), &scopes("c b")),
            scopes("a b c")
        );
        assert_eq!(
            for_login(initial, &[], &scopes("c")),
            scopes("a offline_access c")
        );
    }

    #[test]
    fn reads_a_scope_list_and_ignores_other_values() {
        assert_eq!(list(&json!(["a", 1, "b"])), scopes("a b"));
        assert_eq!(list(&json!("a b")), scopes(""));
        assert_eq!(list(&Value::Null), scopes(""));
    }
}
