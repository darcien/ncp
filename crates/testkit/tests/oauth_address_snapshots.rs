//! Transcripts of the address policy of the OAuth requests: the stops of a
//! login and of a stored login, and the permission in the trust display.
//!
//! All servers are local. A stop happens before a request goes out, so no
//! test contacts a private or a link-local address.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use ncp_testkit::{
    Mode,
    auth_server::{AuthServer, AuthSettings, MetadataPlace, Registration},
};
use serde_json::json;

const LOGIN: [&str; 5] = ["auth", "login", "web", "--credential-store", "file"];

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

fn oauth_filters<'a>(home: &'a Home, issuer: &'a str) -> Vec<(&'a str, &'a str)> {
    // Name the authorization server before the standard loopback filter.
    let mut filters = vec![(issuer, "[ISSUER]")];
    filters.extend(home.filters());
    filters
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_registration_endpoint_stops_the_login_before_any_request() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        registration_endpoint: Some("https://[::ffff:169.254.169.254]/register".into()),
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("oauth-address-blocked", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let filters = oauth_filters(&home, &issuer);
    ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           https://[::ffff:a9fe:a9fe]/register
      cause: The host `[::ffff:a9fe:a9fe]` is an address in a blocked range. ncp sends no OAuth request to this range.
    ");
    let run = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "auth", "login", "web", "--credential-store", "file"]),
        @r#"
    exit_code: 4 (failure)
    ----- stderr -----
    {"error":{"causes":["The host `[::ffff:a9fe:a9fe]` is an address in a blocked range. ncp sends no OAuth request to this range."],"code":"auth","data":["https://[::ffff:a9fe:a9fe]/register"],"exit":4,"hints":[],"message":"ncp stopped an OAuth request for the server `web` on purpose:","server":"web"}}
    "#
    );
    run.error();
    let record = auth.record();
    assert_eq!((record.register_calls, record.authorize_calls), (0, 0));
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    let stops: Vec<serde_json::Value> = run
        .stderr()
        .iter()
        .filter_map(|line| line["trace"].get("oauthStop").cloned())
        .collect();
    assert_eq!(
        stops,
        [json!({
            "url": "https://[::ffff:a9fe:a9fe]/register",
            "reason": "The host [::ffff:a9fe:a9fe] is an address in a blocked range. \
                       ncp sends no OAuth request to this range."
        })]
    );
    assert_eq!(home.file_accounts(), [] as [String; 0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_private_token_endpoint_gets_the_conditional_hint_with_no_command() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        token_endpoint: Some("https://10.0.0.5/token".into()),
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("oauth-address-private", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let filters = oauth_filters(&home, &issuer);
    ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           https://10.0.0.5/token
      cause: The host `10.0.0.5` is a private address. The entry does not permit private addresses.

    hint: If this server needs private-network authorization, set `oauth.allowPrivateAddresses` to `true` in its entry
    ");
    let run = ncp_snapshot!(
        filters,
        home.ncp(["--output-format", "json", "auth", "login", "web", "--credential-store", "file"]),
        @r#"
    exit_code: 4 (failure)
    ----- stderr -----
    {"error":{"causes":["The host `10.0.0.5` is a private address. The entry does not permit private addresses."],"code":"auth","data":["https://10.0.0.5/token"],"exit":4,"hints":[{"message":"If this server needs private-network authorization, set `oauth.allowPrivateAddresses` to `true` in its entry"}],"message":"ncp stopped an OAuth request for the server `web` on purpose:","server":"web"}}
    "#
    );
    let error = run.error();
    assert_eq!(error["hints"][0].get("command"), None, "{error}");
    assert_eq!(auth.record().authorize_calls, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_registration_redirect_stops_the_login_with_no_second_request() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        registration: Registration::Moved,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("oauth-address-redirect", &resource.url);
    let issuer = host_pattern(&auth.issuer());
    let filters = oauth_filters(&home, &issuer);
    ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           http://[ISSUER]/register
      cause: The authorization server answered 307 with a redirect to `http://[ISSUER]/register-moved`. ncp does not follow this redirect.
    ");
    let record = auth.record();
    assert_eq!(record.register_calls, 1);
    assert_eq!(record.registrations, [] as [serde_json::Value; 0]);
    assert_eq!(record.authorize_calls, 0);
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert!(
        run.stderr_text().contains("stopped on purpose"),
        "{}",
        run.describe()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stored_login_stops_when_the_metadata_later_names_a_blocked_token_endpoint() {
    let (auth, resource) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("oauth-address-stored", &resource.url);
    assert_eq!(home.run(&LOGIN).code(), 0);
    auth.settings(|settings| {
        settings.token_endpoint = Some("https://169.254.169.254/token".into());
    });
    let issuer = host_pattern(&auth.issuer());
    let filters = oauth_filters(&home, &issuer);
    ncp_snapshot!(filters, home.ncp(["tool", "list", "web"]), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           https://169.254.169.254/token
      cause: The host `169.254.169.254` is an address in a blocked range. ncp sends no OAuth request to this range.
    ");
    assert_eq!(auth.record().refresh_grants, 0);
}

#[test]
fn the_trust_display_shows_the_private_permission_of_a_project_entry() {
    let home = Home::new("oauth-address-trust").project(
        "app",
        &json!({
            "corp": {"type": "http", "url": "https://mcp.corp.example/mcp",
                     "oauth": {"allowPrivateAddresses": true}},
            "public": {"type": "http", "url": "https://mcp.example.com/mcp"},
            "quoted": {"type": "http", "url": "https://mcp.example.com/mcp",
                       "oauth": {"allowPrivateAddresses": "true"}}
        }),
    );
    ncp_snapshot!(home.filters(), home.ncp(["trust", "--show"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (untrusted)
      corp    https://mcp.corp.example/mcp   oauth.allowPrivateAddresses: true
      public  https://mcp.example.com/mcp
      quoted  https://mcp.example.com/mcp   not valid: The field oauth.allowPrivateAddresses must be a boolean
    ");
    ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "trust", "--show"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"file":"[HOME]/app/.mcp.json","invalid":[{"causes":[],"cwd":null,"env":[],"error":"The field `oauth.allowPrivateAddresses` must be a boolean","file":"[HOME]/app/.mcp.json","name":"quoted","scope":"project","sends":[],"target":"https://mcp.example.com/mcp","transport":"http","trust":"untrusted"}],"servers":[{"allowPrivateAddresses":true,"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"corp","scope":"project","sends":[],"target":"https://mcp.corp.example/mcp","transport":"http","trust":"untrusted"},{"allowPrivateAddresses":false,"cwd":null,"env":[],"file":"[HOME]/app/.mcp.json","name":"public","scope":"project","sends":[],"target":"https://mcp.example.com/mcp","transport":"http","trust":"untrusted"}],"trust":"untrusted"}
    "#
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_proxy_gets_no_plain_http_oauth_request_for_a_loopback_host() {
    let (auth, resource) = oauth_fixture(AuthSettings::default()).await;
    let home = oauth_home("oauth-address-remote-proxy", &resource.url)
        .unset("NO_PROXY")
        .set("HTTP_PROXY", "http://proxy.invalid:3128");
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let mut filters = vec![(resource_host.as_str(), "[RESOURCE]")];
    filters.extend(oauth_filters(&home, &issuer));
    ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           http://[RESOURCE]/mcp
      cause: The URL uses `http`, and the proxy `http://proxy.invalid:3128/` from `HTTP_PROXY` is on another computer

    hint: To send requests to a loopback host directly, run `export NO_PROXY=localhost,127.0.0.1,::1`
    ");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_private_authorization_server_literal_keeps_the_limit_of_the_library() {
    // The library checks the URLs of the protected resource metadata
    // before the client gets them, and it refuses a private IP literal.
    // The entry permission does not change that check.
    let (auth, resource) = oauth_fixture(AuthSettings {
        resource_authorization_server: Some("https://10.0.0.5".into()),
        ..AuthSettings::default()
    })
    .await;
    let home = Home::new("oauth-address-sdk-literal").with_fake_keychain();
    home.configure(&json!({"web": {"type": "http", "url": resource.url,
        "oauth": {"allowPrivateAddresses": true}}}));
    let issuer = host_pattern(&auth.issuer());
    let resource_host = host_pattern(&resource.url);
    let mut filters = vec![(resource_host.as_str(), "[RESOURCE]")];
    filters.extend(oauth_filters(&home, &issuer));
    let mut login = home.ncp(["--timeout", "10"]);
    login.args(LOGIN);
    ncp_snapshot!(filters, login, @"
    exit_code: 3 (failure)
    ----- stderr -----
    error: The OAuth discovery for the server `web` failed
      cause: Protected resource metadata at http://[RESOURCE]/.well-known/oauth-protected-resource/mcp names authorization servers https://10.0.0.5, but none published usable metadata
    ");
    assert_eq!(auth.record().authorize_calls, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_same_origin_metadata_redirect_allows_login() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        metadata_place: MetadataPlace::Moved,
        ..AuthSettings::default()
    })
    .await;
    let home = oauth_home("oauth-address-moved-metadata", &resource.url);
    let run = home.run(&["-vv", "auth", "login", "web", "--credential-store", "file"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let metadata = format!("{}/.well-known/oauth-authorization-server", auth.issuer());
    let outcomes: Vec<(String, String)> = run
        .stderr()
        .iter()
        .filter_map(|line| line["trace"].get("http"))
        .filter(|http| {
            http["url"]
                .as_str()
                .is_some_and(|url| url.starts_with(&metadata))
        })
        .map(|http| (http["url"].to_string(), http["outcome"].to_string()))
        .collect();
    assert_eq!(
        outcomes,
        [
            (
                format!("\"{metadata}\""),
                "\"307 Temporary Redirect\"".to_string()
            ),
            (format!("\"{metadata}-moved\""), "\"200 OK\"".to_string()),
        ]
    );
    assert_eq!(auth.record().metadata_calls, 1);
}

#[test]
fn an_untrusted_project_cannot_grant_private_access_and_a_change_needs_trust_again() {
    let entry = |allow: bool| {
        json!({"web": {"type": "http", "url": "https://mcp.corp.example/mcp",
            "oauth": {"allowPrivateAddresses": allow}}})
    };
    let home = Home::new("oauth-address-project").project("app", &entry(true));
    ncp_snapshot!(home.filters(), home.ncp(["auth", "login", "web"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `web` is in `[HOME]/app/.mcp.json`, which you did not trust:
           web  https://mcp.corp.example/mcp   oauth.allowPrivateAddresses: true

    hint: To trust the file, run `ncp trust`
    ");
    let home = home.project("app", &entry(false));
    assert_eq!(home.run(&["trust"]).code(), 0);
    let home = home.project("app", &entry(true));
    ncp_snapshot!(home.filters(), home.ncp(["trust", "--show"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Project file: [HOME]/app/.mcp.json (untrusted)
      web  https://mcp.corp.example/mcp   oauth.allowPrivateAddresses: true
    ");
}

// The token endpoint of the authorization server under a localhost name,
// which a test answers with `NCP_TEST_HOSTS`.
fn named_token_endpoint(auth: &AuthServer) -> String {
    let port = url::Url::parse(&auth.issuer()).unwrap().port().unwrap();
    format!("http://tok.localhost:{port}/token")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_stops_before_the_browser_when_the_token_host_has_no_permitted_address() {
    let (auth, resource) = oauth_fixture(AuthSettings::default()).await;
    let token = named_token_endpoint(&auth);
    auth.settings(|settings| settings.token_endpoint = Some(token));
    let home = oauth_home("oauth-address-token-lookup", &resource.url)
        .set("NCP_TEST_HOSTS", "tok.localhost=93.184.216.34");
    let issuer = host_pattern(&auth.issuer());
    let mut filters = vec![(r"tok\.localhost:\d+", "tok.localhost:[PORT]")];
    filters.extend(oauth_filters(&home, &issuer));
    ncp_snapshot!(filters, home.ncp(LOGIN), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           http://tok.localhost:[PORT]/token
      cause: The host `tok.localhost` is a localhost name. It resolves to `93.184.216.34`, which is not a loopback address.
    ");
    let record = auth.record();
    assert_eq!((record.authorize_calls, record.code_grants), (0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_stops_at_the_connection_when_the_dns_answer_changes() {
    let (auth, resource) = oauth_fixture(AuthSettings {
        expires_in: 0,
        ..AuthSettings::default()
    })
    .await;
    let token = named_token_endpoint(&auth);
    auth.settings(|settings| settings.token_endpoint = Some(token));
    let home = oauth_home("oauth-address-refresh", &resource.url)
        .set("NCP_TEST_HOSTS", "tok.localhost=127.0.0.1");
    let run = home.run(&LOGIN);
    assert_eq!(run.code(), 0, "{}", run.describe());
    // The name now resolves away from the computer of the user. The
    // refresh looks it up again at its connection.
    let home = home.set("NCP_TEST_HOSTS", "tok.localhost=10.0.0.5");
    let issuer = host_pattern(&auth.issuer());
    let mut filters = vec![(r"tok\.localhost:\d+", "tok.localhost:[PORT]")];
    filters.extend(oauth_filters(&home, &issuer));
    ncp_snapshot!(filters, home.ncp(["tool", "list", "web"]), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           http://tok.localhost:[PORT]/token
      cause: The host `tok.localhost` is a localhost name. It resolves to `10.0.0.5`, which is not a loopback address.
    ");
    assert_eq!(auth.record().refresh_grants, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_that_stops_during_a_request_gives_the_stop() {
    // rmcp refreshes a token when less than 30 s of it remain, in whole
    // seconds. The token lasts 33 s, and the handshake takes 4 s: the
    // connect needs no refresh, and the first request after it does.
    let auth = ncp_testkit::auth_server::serve_auth(AuthSettings {
        expires_in: 33,
        ..AuthSettings::default()
    })
    .await
    .unwrap();
    let resource = ncp_testkit::serve_http_with_oauth(
        Mode::SlowModernOnly(std::time::Duration::from_secs(4)),
        &auth,
        None,
    )
    .await
    .unwrap();
    let token = named_token_endpoint(&auth);
    auth.settings(|settings| settings.token_endpoint = Some(token));
    let home = oauth_home("oauth-address-request-refresh", &resource.url)
        .set("NCP_TEST_HOSTS", "tok.localhost=127.0.0.1");
    let run = home.run(&LOGIN);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let home = home.set("NCP_TEST_HOSTS", "tok.localhost=10.0.0.5");
    let issuer = host_pattern(&auth.issuer());
    let mut filters = vec![(r"tok\.localhost:\d+", "tok.localhost:[PORT]")];
    filters.extend(oauth_filters(&home, &issuer));
    ncp_snapshot!(filters, home.ncp(["tool", "list", "web"]), @"
    exit_code: 4 (failure)
    ----- stderr -----
    error: ncp stopped an OAuth request for the server `web` on purpose:
           http://tok.localhost:[PORT]/token
      cause: The host `tok.localhost` is a localhost name. It resolves to `10.0.0.5`, which is not a loopback address.
    ");
    assert_eq!(auth.record().refresh_grants, 0);
}
