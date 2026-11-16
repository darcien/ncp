//! Transcripts of login, logout and stored login status.

#![allow(clippy::unwrap_used)]

mod common;

use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt as _,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use common::{Home, Run, ncp_snapshot};
use ncp_testkit::{
    Mode,
    auth_server::{AuthServer, AuthSettings, Decision},
};
use serde_json::{Value, json};

const LOGIN: [&str; 5] = ["auth", "login", "web", "--credential-store", "file"];
const JSON_LOGIN: [&str; 7] = [
    "--output-format",
    "json",
    "auth",
    "login",
    "web",
    "--credential-store",
    "file",
];

async fn oauth_fixture(settings: AuthSettings) -> (AuthServer, ncp_testkit::HttpFixture) {
    let auth = ncp_testkit::auth_server::serve_auth(settings)
        .await
        .unwrap();
    let resource = ncp_testkit::serve_http_with_oauth(Mode::Modern, &auth, None)
        .await
        .unwrap();
    (auth, resource)
}

fn oauth_home(name: &str, resource: &str) -> Home {
    let home = Home::new(name)
        .with_fake_keychain()
        .set("NCP_TEST_BROWSER", "follow");
    home.configure(&json!({"web": {"type": "http", "url": resource}}));
    home
}

fn host_pattern(url: &str) -> String {
    let url = url::Url::parse(url).unwrap();
    format!(r"127\.0\.0\.1(?::|%3A){}\b", url.port().unwrap())
}

fn oauth_filters<'a>(
    home: &'a Home,
    issuer: &'a str,
    resource: &'a str,
) -> Vec<(&'a str, &'a str)> {
    // Name both servers before the standard loopback filter merges them.
    let mut filters = vec![
        (issuer, "[ISSUER]"),
        (resource, "[RESOURCE]"),
        (r"127\.0\.0\.1%3A\d+", "[CALLBACK]"),
        (r#"(state=)[^&\s"]+"#, "${1}[STATE]"),
        (r#"(code_challenge=)[^&\s"]+"#, "${1}[PKCE_CHALLENGE]"),
        (r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", "[EXPIRY]"),
    ];
    filters.extend(home.filters());
    filters
}

fn credential_filters<'a>(filters: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    // Use token filters only on the credential file. On command output,
    // they hide a leaked token.
    let mut filters = filters.to_vec();
    filters.extend([
        (r#"(token_received_at\\":)\d+"#, "${1}[RECEIVED_AT]"),
        (r#"(access_token\\":\\")[^"\\]+"#, "${1}[ACCESS_TOKEN]"),
        (r#"(refresh_token\\":\\")[^"\\]+"#, "${1}[REFRESH_TOKEN]"),
    ]);
    filters
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn assert_authorization(
    run: &Run,
    json_output: bool,
    auth: &AuthServer,
    resource: &str,
    grants: usize,
) {
    let url = if json_output {
        let login = run
            .stderr()
            .into_iter()
            .find_map(|line| line.get("login").cloned())
            .unwrap();
        assert_eq!(login["server"], "web");
        url::Url::parse(login["url"].as_str().unwrap()).unwrap()
    } else {
        let stderr = run.stderr_text();
        let url = stderr
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("http://"))
            .unwrap();
        url::Url::parse(url).unwrap()
    };
    assert_eq!(url.origin().ascii_serialization(), auth.issuer());
    assert_eq!(url.path(), "/authorize");
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["client_id"], "client-1");
    assert_eq!(query["scope"], "a b");
    assert_eq!(query["resource"], resource);
    assert_ne!(query["state"], "");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&query["code_challenge"])
            .unwrap()
            .len(),
        32
    );
    let callback = url::Url::parse(&query["redirect_uri"]).unwrap();
    assert_eq!(callback.scheme(), "http");
    assert_eq!(callback.host_str(), Some("127.0.0.1"));
    assert!(callback.port().is_some_and(|port| port != 0));
    assert_eq!(callback.path(), "/callback");
    assert!(callback.query().is_none());

    let record = auth.record();
    assert_eq!(
        (
            record.register_calls,
            record.authorize_calls,
            record.code_grants
        ),
        (1, 1, grants)
    );
    assert_eq!(record.authorize_resource.as_deref(), Some(resource));
    assert_eq!(record.authorize_scope.as_deref(), Some("a b"));
    assert_eq!(record.registrations.len(), 1);
    let registration = &record.registrations[0];
    assert_eq!(registration["client_name"], "ncp");
    assert_eq!(
        registration["redirect_uris"],
        json!([query["redirect_uri"]])
    );
    assert_eq!(registration["scope"], "a b");
    assert_eq!(
        registration["grant_types"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(registration["response_types"], json!(["code"]));
    assert_eq!(registration["token_endpoint_auth_method"], "none");
    if grants == 1 {
        assert_eq!(record.token_resource.as_deref(), Some(resource));
        assert_eq!(record.token_client_id.as_deref(), Some("client-1"));
        assert!(record.token_secret.is_none());
        assert!(!record.token_basic);
    } else {
        assert!(record.token_resource.is_none());
    }
}

fn assert_credential_mode(home: &Home) {
    let path = home.root.join("state/ncp/credentials.json");
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

fn assert_stored_login(home: &Home, issuer: &str, resource: &str, started: u64) -> Value {
    assert_credential_mode(home);
    let file = home.read("state/ncp/credentials.json");
    let file: Value = serde_json::from_str(&file).unwrap();
    let tokens: Value =
        serde_json::from_str(file["secrets"]["oauth/web/tokens"].as_str().unwrap()).unwrap();
    let client: Value =
        serde_json::from_str(file["secrets"]["oauth/web/client"].as_str().unwrap()).unwrap();
    assert_eq!(client, json!({"issuer": issuer, "clientId": "client-1"}));
    assert_eq!(tokens["issuer"], issuer);
    assert_eq!(
        tokens["ncp_binding"],
        json!({"issuer": issuer, "resource": resource})
    );
    assert_eq!(tokens["client_id"], "client-1");
    assert_eq!(tokens["ncp_requested_scopes"], json!(["a", "b"]));
    assert_eq!(tokens["granted_scopes"], json!(["a", "b"]));
    assert_eq!(tokens["token_response"]["expires_in"], 3600);
    let received = tokens["token_received_at"].as_u64().unwrap();
    assert!((started..=now()).contains(&received));
    for key in ["access_token", "refresh_token"] {
        assert_ne!(tokens["token_response"][key].as_str().unwrap(), "");
    }
    tokens
}

fn assert_no_tokens(run: &Run, tokens: &Value, json_output: bool) {
    // The login URL is not protected output. Random URL data can resemble a token.
    let protected_stderr = if json_output {
        run.stderr()
            .into_iter()
            .filter(|line| line.get("login").is_none())
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        run.stderr_text()
            .lines()
            .filter(|line| !line.starts_with("  http://"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for key in ["access_token", "refresh_token"] {
        let token = tokens["token_response"][key].as_str().unwrap();
        assert!(!run.stdout_text().contains(token), "{key} shows on stdout");
        assert!(!protected_stderr.contains(token), "{key} shows on stderr");
    }
}

fn expiry(tokens: &Value) -> String {
    ncp::oauth::items::rfc3339(tokens["token_received_at"].as_u64().unwrap() + 3600)
}

#[tokio::test(flavor = "multi_thread")]
async fn text_browser_login_status_and_logout_show_the_stored_login() {
    let (auth, resource) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("snap-auth-text-lifecycle", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let filters = oauth_filters(&home, &issuer, &resource_host);
    let started = now();

    let login = ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 0 (success)
    ----- stdout -----
    Logged in to web at http://[ISSUER]. The tokens are in the credential file.

    ----- stderr -----
    Open this URL in a browser to log in to web:

      http://[ISSUER]/authorize?response_type=code&client_id=client-1&state=[STATE]&code_challenge=[PKCE_CHALLENGE]&code_challenge_method=S256&redirect_uri=http%3A%2F%2F[CALLBACK]%2Fcallback&scope=a+b&resource=http%3A%2F%2F[RESOURCE]%2Fmcp
    ");
    assert_authorization(&login, false, &auth, &resource.url, 1);
    let tokens = assert_stored_login(&home, &auth.issuer(), &resource.url, started);
    assert_no_tokens(&login, &tokens, false);
    assert!(login.stdout_text().contains(&auth.issuer()));
    insta::with_settings!({filters => credential_filters(&filters)}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}",
            "oauth/web/tokens": "{\"client_id\":\"client-1\",\"granted_scopes\":[\"a\",\"b\"],\"issuer\":\"http://[ISSUER]\",\"ncp_binding\":{\"issuer\":\"http://[ISSUER]\",\"resource\":\"http://[RESOURCE]/mcp\"},\"ncp_requested_scopes\":[\"a\",\"b\"],\"token_received_at\":[RECEIVED_AT],\"token_response\":{\"access_token\":\"[ACCESS_TOKEN]\",\"expires_in\":3600,\"refresh_token\":\"[REFRESH_TOKEN]\",\"scope\":\"a b\",\"token_type\":\"bearer\"}}"
          }
        }
        "#);
    });

    let status = ncp_snapshot!(filters, home.ncp(["auth", "status", "web"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    web  logged in  http://[ISSUER]  a b  [EXPIRY]
    ");
    assert_no_tokens(&status, &tokens, false);
    assert!(status.stdout_text().contains(&auth.issuer()));
    assert!(status.stdout_text().contains(&expiry(&tokens)));

    let logout = ncp_snapshot!(filters, home.ncp(["auth", "logout", "web"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Deleted the tokens of web from the credential file.
    ");
    assert_no_tokens(&logout, &tokens, false);
    assert_eq!(home.file_accounts(), ["oauth/web/client"]);
    assert_credential_mode(&home);
    insta::with_settings!({filters => filters.clone()}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}"
          }
        }
        "#);
    });
    let status = ncp_snapshot!(filters, home.ncp(["auth", "status", "web"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    web  not logged in
    ");
    assert_no_tokens(&status, &tokens, false);
}

#[tokio::test(flavor = "multi_thread")]
async fn json_browser_login_status_and_logout_show_the_stored_login() {
    let (auth, resource) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("snap-auth-json-lifecycle", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let filters = oauth_filters(&home, &issuer, &resource_host);
    let started = now();

    let login = ncp_snapshot!(filters, home.ncp(JSON_LOGIN), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"credentialStore":"file","issuer":"http://[ISSUER]","scopes":["a","b"],"server":"web"}

    ----- stderr -----
    {"login":{"server":"web","url":"http://[ISSUER]/authorize?response_type=code&client_id=client-1&state=[STATE]&code_challenge=[PKCE_CHALLENGE]&code_challenge_method=S256&redirect_uri=http%3A%2F%2F[CALLBACK]%2Fcallback&scope=a+b&resource=http%3A%2F%2F[RESOURCE]%2Fmcp"}}
    "#);
    assert_eq!(login.warnings(), [] as [serde_json::Value; 0]);
    assert_authorization(&login, true, &auth, &resource.url, 1);
    assert_eq!(login.stdout()["issuer"], auth.issuer());
    let tokens = assert_stored_login(&home, &auth.issuer(), &resource.url, started);
    assert_no_tokens(&login, &tokens, true);
    insta::with_settings!({filters => credential_filters(&filters)}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}",
            "oauth/web/tokens": "{\"client_id\":\"client-1\",\"granted_scopes\":[\"a\",\"b\"],\"issuer\":\"http://[ISSUER]\",\"ncp_binding\":{\"issuer\":\"http://[ISSUER]\",\"resource\":\"http://[RESOURCE]/mcp\"},\"ncp_requested_scopes\":[\"a\",\"b\"],\"token_received_at\":[RECEIVED_AT],\"token_response\":{\"access_token\":\"[ACCESS_TOKEN]\",\"expires_in\":3600,\"refresh_token\":\"[REFRESH_TOKEN]\",\"scope\":\"a b\",\"token_type\":\"bearer\"}}"
          }
        }
        "#);
    });

    let status = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "auth", "status", "web"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"servers":[{"expiresAt":"[EXPIRY]","issuer":"http://[ISSUER]","loggedIn":true,"name":"web","scopes":["a","b"]}]}
    "#
    );
    assert_eq!(status.warnings(), [] as [serde_json::Value; 0]);
    assert_no_tokens(&status, &tokens, true);
    assert_eq!(status.stdout()["servers"][0]["issuer"], auth.issuer());
    assert_eq!(status.stdout()["servers"][0]["expiresAt"], expiry(&tokens));

    let logout = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "auth", "logout", "web"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"credentialStores":["file"],"server":"web"}
    "#
    );
    assert_eq!(logout.warnings(), [] as [serde_json::Value; 0]);
    assert_no_tokens(&logout, &tokens, true);
    assert_eq!(home.file_accounts(), ["oauth/web/client"]);
    assert_credential_mode(&home);
    insta::with_settings!({filters => filters.clone()}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}"
          }
        }
        "#);
    });
    let status = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "auth", "status", "web"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"servers":[{"expiresAt":null,"issuer":null,"loggedIn":false,"name":"web","scopes":[]}]}
    "#
    );
    assert_eq!(status.warnings(), [] as [serde_json::Value; 0]);
    assert_no_tokens(&status, &tokens, true);
    assert_eq!(status.stdout()["servers"][0]["loggedIn"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn text_browser_login_reports_a_refusal_without_storing_tokens() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        decision: Decision::Deny,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("snap-auth-text-denied", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let filters = oauth_filters(&home, &issuer, &resource_host);
    let run = ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    Open this URL in a browser to log in to web:

      http://[ISSUER]/authorize?response_type=code&client_id=client-1&state=[STATE]&code_challenge=[PKCE_CHALLENGE]&code_challenge_method=S256&redirect_uri=http%3A%2F%2F[CALLBACK]%2Fcallback&scope=a+b&resource=http%3A%2F%2F[RESOURCE]%2Fmcp

    error: The login to the server `web` failed
      cause: The authorization server refused the login with the error `access_denied`
    ");
    assert_authorization(&run, false, &auth, &resource.url, 0);
    assert_eq!(home.file_accounts(), ["oauth/web/client"]);
    assert_credential_mode(&home);
    insta::with_settings!({filters => filters.clone()}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}"
          }
        }
        "#);
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn json_browser_login_reports_a_refusal_without_storing_tokens() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        decision: Decision::Deny,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("snap-auth-json-denied", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let filters = oauth_filters(&home, &issuer, &resource_host);
    let run = ncp_snapshot!(filters, home.ncp(JSON_LOGIN), @r#"
    exit_code: 4 (failure)
    ----- stderr -----
    {"login":{"server":"web","url":"http://[ISSUER]/authorize?response_type=code&client_id=client-1&state=[STATE]&code_challenge=[PKCE_CHALLENGE]&code_challenge_method=S256&redirect_uri=http%3A%2F%2F[CALLBACK]%2Fcallback&scope=a+b&resource=http%3A%2F%2F[RESOURCE]%2Fmcp"}}
    {"error":{"causes":["The authorization server refused the login with the error `access_denied`"],"code":"auth","data":[],"exit":4,"hints":[],"message":"The login to the server `web` failed","server":"web"}}
    "#);
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert_authorization(&run, true, &auth, &resource.url, 0);
    assert_eq!(home.file_accounts(), ["oauth/web/client"]);
    assert_credential_mode(&home);
    insta::with_settings!({filters => filters.clone()}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/client": "{\"issuer\":\"http://[ISSUER]\",\"clientId\":\"client-1\"}"
          }
        }
        "#);
    });
}

#[test]
fn status_reports_a_corrupt_stored_login_without_its_value() {
    let home = Home::new("snap-auth-status-corrupt").with_fake_keychain();
    home.configure(&json!({"web": {"type": "http", "url": "https://mcp.example.test/mcp"}}));
    home.store_in_file("oauth/web/tokens", "private-token-not-json");
    let text = ncp_snapshot!(home.filters(), home.ncp(["auth", "status", "web"]), @"
    exit_code: 3 (failure)
    ----- stderr -----
    error: The stored OAuth tokens of the server `web` are not JSON
      cause: The item is not valid JSON
    ");
    assert!(!text.describe().contains("private-token-not-json"));
    let json = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "auth", "status", "web"]),
        @r#"
    exit_code: 3 (failure)
    ----- stderr -----
    {"error":{"causes":["The item is not valid JSON"],"code":"protocol","data":[],"exit":3,"hints":[],"message":"The stored OAuth tokens of the server `web` are not JSON","server":"web"}}
    "#
    );
    json.error();
    assert_eq!(json.warnings(), [] as [serde_json::Value; 0]);
    assert!(!json.describe().contains("private-token-not-json"));
    let mut filters = vec![("private-token-not-json", "[SECRET]")];
    filters.extend(home.filters());
    insta::with_settings!({filters => filters}, {
        insta::assert_snapshot!(home.read("state/ncp/credentials.json"), @r#"
        {
          "version": 1,
          "secrets": {
            "oauth/web/tokens": "[SECRET]"
          }
        }
        "#);
    });
}

#[test]
fn logout_rejects_a_stdio_server() {
    let home = Home::new("snap-auth-logout-stdio").with_fake_keychain();
    home.configure(&json!({"fx": {"command": "unused"}}));
    ncp_snapshot!(home.filters(), home.ncp(["auth", "logout", "fx"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `fx` is a stdio server, and OAuth is only for HTTP servers

    hint: To give credentials to a stdio server, use the `env` field of its entry
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "auth", "logout", "fx"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"message":"To give credentials to a stdio server, use the `env` field of its entry"}],"message":"The server `fx` is a stdio server, and OAuth is only for HTTP servers","server":"fx"}}
    "#
    );
    run.error();
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
    assert!(!home.root.join("state/ncp/credentials.json").exists());
}
