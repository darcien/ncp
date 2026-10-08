//! Tests of the local authorization server fixture and the OAuth mode of
//! the HTTP fixture, with plain HTTP requests.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use base64::Engine as _;
use ncp_testkit::{
    Mode, StepUp,
    auth_server::{AuthServer, AuthSettings, CallbackIss, Decision, serve_auth},
    metadata_url, serve_http_with_oauth,
};
use serde_json::{Value, json};

const REDIRECT: &str = "http://127.0.0.1:4567/callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

fn http() -> reqwest::Client {
    ncp::client::http::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn challenge_of(verifier: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

async fn json_of(response: reqwest::Response) -> (u16, Value) {
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn register(server: &AuthServer) -> Value {
    let response = http()
        .post(format!("{}/register", origin(server)))
        .body(
            json!({"redirect_uris": ["http://127.0.0.1/callback"], "client_name": "ncp"})
                .to_string(),
        )
        .send()
        .await
        .unwrap();
    let (status, body) = json_of(response).await;
    assert_eq!(status, 201, "{body}");
    body
}

fn origin(server: &AuthServer) -> String {
    let issuer = url::Url::parse(&server.issuer()).unwrap();
    issuer.origin().ascii_serialization()
}

/// The query of the redirect after an authorization request.
async fn authorize(server: &AuthServer, client_id: &str, scope: &str) -> HashMap<String, String> {
    let challenge = challenge_of(VERIFIER);
    let query = form(&[
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", REDIRECT),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("state", "st-1"),
        ("resource", "http://127.0.0.1:1/mcp"),
        ("scope", scope),
    ]);
    let response = http()
        .get(format!("{}/authorize?{query}", origin(server)))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 302);
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert!(location.starts_with(REDIRECT), "{location}");
    url::Url::parse(&location)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

async fn token(server: &AuthServer, pairs: &[(&str, &str)]) -> (u16, Value) {
    let response = http()
        .post(format!("{}/token", origin(server)))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form(pairs))
        .send()
        .await
        .unwrap();
    json_of(response).await
}

async fn exchange(server: &AuthServer, client_id: &str, code: &str) -> (u16, Value) {
    token(
        server,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("code_verifier", VERIFIER),
            ("client_id", client_id),
            ("resource", "http://127.0.0.1:1/mcp"),
        ],
    )
    .await
}

/// A valid access token with the scope.
async fn login(server: &AuthServer, scope: &str) -> Value {
    let client = register(server).await;
    let client_id = client["client_id"].as_str().unwrap();
    let callback = authorize(server, client_id, scope).await;
    let (status, tokens) = exchange(server, client_id, &callback["code"]).await;
    assert_eq!(status, 200, "{tokens}");
    tokens
}

#[tokio::test]
async fn metadata_names_the_endpoints_and_the_issuer() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let response = http()
        .get(format!(
            "{}/.well-known/oauth-authorization-server",
            server.issuer()
        ))
        .send()
        .await
        .unwrap();
    let (status, metadata) = json_of(response).await;
    assert_eq!(status, 200);
    assert_eq!(metadata["issuer"], server.issuer());
    assert_eq!(
        metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    assert_eq!(
        metadata["authorization_response_iss_parameter_supported"],
        true
    );
    assert!(
        metadata["registration_endpoint"]
            .as_str()
            .unwrap()
            .ends_with("/register")
    );
    assert_eq!(server.record().metadata_calls, 1);
}

#[tokio::test]
async fn code_flow_with_pkce_gives_tokens_and_records_the_resource() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let client = register(&server).await;
    assert_eq!(client["token_endpoint_auth_method"], "none");
    assert!(client.get("client_secret").is_none());
    let client_id = client["client_id"].as_str().unwrap();
    let callback = authorize(&server, client_id, "a").await;
    assert_eq!(callback["state"], "st-1");
    assert_eq!(callback["iss"], server.issuer());
    let (status, tokens) = exchange(&server, client_id, &callback["code"]).await;
    assert_eq!(status, 200, "{tokens}");
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["scope"], "a");
    assert!(tokens["access_token"].as_str().unwrap().starts_with("at-"));
    assert!(tokens["refresh_token"].as_str().unwrap().starts_with("rt-"));
    let record = server.record();
    assert_eq!(
        record.authorize_resource.as_deref(),
        Some("http://127.0.0.1:1/mcp")
    );
    assert_eq!(
        record.token_resource.as_deref(),
        Some("http://127.0.0.1:1/mcp")
    );
    assert_eq!(record.authorize_scope.as_deref(), Some("a"));
    assert_eq!((record.register_calls, record.code_grants), (1, 1));
    assert_eq!(record.registrations[0]["client_name"], "ncp");
    let (status, error) = exchange(&server, client_id, &callback["code"]).await;
    assert_eq!((status, &error["error"]), (400, &json!("invalid_grant")));
}

#[tokio::test]
async fn token_endpoint_rejects_a_wrong_verifier() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    server.add_client("pre", None);
    let callback = authorize(&server, "pre", "").await;
    let (status, error) = token(
        &server,
        &[
            ("grant_type", "authorization_code"),
            ("code", &callback["code"]),
            ("redirect_uri", REDIRECT),
            (
                "code_verifier",
                "wrong-verifier-wrong-verifier-wrong-verifier",
            ),
            ("client_id", "pre"),
            ("resource", "http://127.0.0.1:1/mcp"),
        ],
    )
    .await;
    assert_eq!((status, &error["error"]), (400, &json!("invalid_grant")));
}

#[tokio::test]
async fn authorize_rejects_an_unknown_client_and_can_deny() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let response = http()
        .get(format!(
            "{}/authorize?client_id=nobody&redirect_uri={REDIRECT}",
            origin(&server)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    server.add_client("pre", None);
    server.settings(|settings| settings.decision = Decision::Deny);
    let callback = authorize(&server, "pre", "").await;
    assert_eq!(callback["error"], "access_denied");
    assert_eq!(callback["state"], "st-1");
    assert!(!callback.contains_key("code"));
}

#[tokio::test]
async fn callback_iss_can_be_absent_or_wrong() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    server.add_client("pre", None);
    server.settings(|settings| settings.callback_iss = CallbackIss::Absent);
    assert!(!authorize(&server, "pre", "").await.contains_key("iss"));
    server.settings(|settings| settings.callback_iss = CallbackIss::Wrong);
    assert_ne!(authorize(&server, "pre", "").await["iss"], server.issuer());
}

#[tokio::test]
async fn refresh_rotates_the_tokens_unless_rejected() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let tokens = login(&server, "a b").await;
    let client_id = server.record().token_client_id.unwrap();
    let refresh = tokens["refresh_token"].as_str().unwrap();
    let grant = |refresh: String| {
        let client_id = client_id.clone();
        let server = &server;
        async move {
            token(
                server,
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &refresh),
                    ("client_id", &client_id),
                ],
            )
            .await
        }
    };
    let (status, new) = grant(refresh.to_string()).await;
    assert_eq!(status, 200, "{new}");
    assert_ne!(new["access_token"], tokens["access_token"]);
    assert_eq!(new["scope"], "a b");
    let (status, _) = grant(refresh.to_string()).await;
    assert_eq!(status, 400, "an old refresh token is not valid again");
    server.settings(|settings| settings.reject_refresh = true);
    let (status, error) = grant(new["refresh_token"].as_str().unwrap().to_string()).await;
    assert_eq!((status, &error["error"]), (400, &json!("invalid_grant")));
    assert_eq!(server.record().refresh_grants, 3);
}

#[tokio::test]
async fn registered_client_with_a_secret_must_send_it() {
    let server = serve_auth(AuthSettings {
        register_with_secret: true,
        ..AuthSettings::default()
    })
    .await
    .unwrap();
    let client = register(&server).await;
    let (client_id, secret) = (
        client["client_id"].as_str().unwrap(),
        client["client_secret"].as_str().unwrap(),
    );
    assert_eq!(client["token_endpoint_auth_method"], "client_secret_post");
    let callback = authorize(&server, client_id, "").await;
    let (status, error) = exchange(&server, client_id, &callback["code"]).await;
    assert_eq!((status, &error["error"]), (401, &json!("invalid_client")));
    let callback = authorize(&server, client_id, "").await;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{client_id}:{secret}"));
    let response = http()
        .post(format!("{}/token", origin(&server)))
        .header("authorization", format!("Basic {basic}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form(&[
            ("grant_type", "authorization_code"),
            ("code", &callback["code"]),
            ("redirect_uri", REDIRECT),
            ("code_verifier", VERIFIER),
            ("resource", "http://127.0.0.1:1/mcp"),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn new_issuer_path_moves_the_metadata_and_the_resource_metadata_follows() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let fixture = serve_http_with_oauth(Mode::Modern, &server, None)
        .await
        .unwrap();
    let first = server.issuer();
    server.settings(|settings| settings.issuer_path = "/b".into());
    assert_eq!(server.issuer(), format!("{first}/b"));
    let moved = http()
        .get(format!("{first}/.well-known/oauth-authorization-server/b"))
        .send()
        .await
        .unwrap();
    let (status, metadata) = json_of(moved).await;
    assert_eq!(
        (status, &metadata["issuer"]),
        (200, &json!(server.issuer()))
    );
    let old = http()
        .get(format!("{first}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .unwrap();
    assert_eq!(old.status().as_u16(), 404);
    let response = http().get(metadata_url(&fixture.url)).send().await.unwrap();
    let (_, resource) = json_of(response).await;
    assert_eq!(resource["authorization_servers"], json!([server.issuer()]));
}

async fn post_mcp(url: &str, token: Option<&str>, body: &Value) -> reqwest::Response {
    let request = http()
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(body.to_string());
    let request = match token {
        Some(token) => request.bearer_auth(token),
        None => request,
    };
    request.send().await.unwrap()
}

fn www_authenticate(response: &reqwest::Response) -> String {
    response.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn protected_fixture_names_its_metadata_and_takes_only_issued_tokens() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let fixture = serve_http_with_oauth(Mode::Modern, &server, None)
        .await
        .unwrap();
    let metadata = metadata_url(&fixture.url);
    assert!(metadata.ends_with("/.well-known/oauth-protected-resource/mcp"));
    let (status, resource) = json_of(http().get(&metadata).send().await.unwrap()).await;
    assert_eq!(status, 200);
    assert_eq!(resource["resource"], fixture.url);
    assert_eq!(resource["authorization_servers"], json!([server.issuer()]));
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let response = post_mcp(&fixture.url, None, &ping).await;
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(
        www_authenticate(&response),
        format!("Bearer resource_metadata=\"{metadata}\"")
    );
    let response = post_mcp(&fixture.url, Some("at-unknown"), &ping).await;
    assert_eq!(response.status().as_u16(), 401);
    assert!(www_authenticate(&response).contains("error=\"invalid_token\""));
    let tokens = login(&server, "a").await;
    let response = post_mcp(&fixture.url, tokens["access_token"].as_str(), &ping).await;
    assert_ne!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn expired_token_gets_401() {
    let server = serve_auth(AuthSettings {
        expires_in: 0,
        ..AuthSettings::default()
    })
    .await
    .unwrap();
    let fixture = serve_http_with_oauth(Mode::Modern, &server, None)
        .await
        .unwrap();
    let tokens = login(&server, "a").await;
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let response = post_mcp(&fixture.url, tokens["access_token"].as_str(), &ping).await;
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn step_up_tool_needs_its_scope() {
    let server = serve_auth(AuthSettings::default()).await.unwrap();
    let step_up = StepUp {
        tool: "echo".into(),
        scope: "b".into(),
    };
    let fixture = serve_http_with_oauth(Mode::Modern, &server, Some(step_up))
        .await
        .unwrap();
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "echo", "arguments": {"text": "x"}}});
    let narrow = login(&server, "a").await;
    let response = post_mcp(&fixture.url, narrow["access_token"].as_str(), &call).await;
    assert_eq!(response.status().as_u16(), 403);
    let challenge = www_authenticate(&response);
    assert!(
        challenge.contains("error=\"insufficient_scope\"") && challenge.contains("scope=\"b\""),
        "{challenge}"
    );
    let list = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"});
    let response = post_mcp(&fixture.url, narrow["access_token"].as_str(), &list).await;
    assert_ne!(response.status().as_u16(), 403);
    let wide = login(&server, "a b").await;
    let response = post_mcp(&fixture.url, wide["access_token"].as_str(), &call).await;
    assert_ne!(response.status().as_u16(), 403);
}
