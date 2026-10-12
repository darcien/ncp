//! Live tests: `ncp` on this Mac, with the login Keychain, a real browser,
//! the system settings and a login at a real server. See
//! `docs/live-tests.md`.
//!
//! They change the state of the Mac, and most need a person, so only the
//! user runs them. Thus they are ignored by default. Run them with `mise
//! run live`. The proxy test needs only `sudo`, so a CI runner on macOS can
//! run it.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]
#![cfg(target_os = "macos")]

mod common;

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use common::{Home, Run};
use ncp_testkit::Mode;
use serde_json::{Value, json};

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

/// The name of the test secret in the Keychain.
const SECRET: &str = "NCP_LIVE_CHECK";

/// The `ncp` binary of the `ncp` package: the native store and the real
/// browser. `mise run live` builds it before the tests.
fn ncp() -> PathBuf {
    let dir = std::env::current_exe().unwrap();
    let path = dir.parent().unwrap().parent().unwrap().join("ncp");
    assert!(path.is_file(), "no {}: run `mise run live`", path.display());
    path
}

/// A home with the real `HOME`, so that `/usr/bin/security` finds the
/// login Keychain. The XDG folders stay in the test home.
fn live_home(test: &str) -> Home {
    Home::new(test)
        .unset("HOME")
        .set("HOME", &std::env::var("HOME").unwrap())
}

fn run(home: &Home, program: &Path, args: &[&str], stdin: &str) -> Run {
    home.run_program(program, args, stdin)
}

/// A login that the person sees and answers: stdin and stderr are the
/// terminal, and the output is text, as for a user. Gives the result line.
fn login_with_person(home: &Home, args: &[&str]) -> String {
    let output = home
        .command(&ncp())
        .arg("--output-format=text")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .unwrap();
    let run = Run(output);
    assert_eq!(run.code(), 0, "{}", run.describe());
    run.stdout_text()
}

fn ok(run: &Run) -> Value {
    assert_eq!(run.code(), 0, "{}", run.describe());
    run.stdout()
}

/// The JSON result of a run of `ncp` that must pass.
fn ncp_ok(home: &Home, args: &[&str]) -> Value {
    ok(&run(home, &ncp(), args, ""))
}

fn say(text: &str) {
    println!("\n>>> {text}\n");
}

/// Runs the command when the test ends, also after a failure, so that no
/// test item stays in the Keychain.
struct Cleanup<'a> {
    home: &'a Home,
    args: &'a [&'a str],
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = run(self.home, &ncp(), self.args, "");
    }
}

/// Items that `/usr/bin/security` made trust that program, not `ncp`. Two
/// binaries stand for an update, which gives `ncp` a new code identity.
#[test]
#[ignore = "uses the login Keychain"]
fn keychain_gives_a_secret_to_a_new_binary_with_no_dialog() {
    say(
        "Do nothing. This test uses the login Keychain. If a Keychain dialog opens, click Deny. The test then fails.",
    );
    let home = live_home("keychain").set("NCP_TEST_SECURITY_PROGRAM", "/usr/bin/security");
    let _cleanup = Cleanup {
        home: &home,
        args: &["secret", "delete", SECRET, "--all"],
    };
    let add = [
        "server", "add", "check", "--global", "--env", SECRET, "--secret", SECRET, "--", FIXTURE,
        "modern",
    ];
    let added = ok(&run(&home, &ncp(), &add, "check value"));
    assert_eq!(added["secrets"][0]["credentialStores"], json!(["native"]));

    // The test binary uses `/usr/bin/security` through the same code.
    for program in [ncp(), PathBuf::from(common::NCP)] {
        let list = ok(&run(&home, &program, &["secret", "list"], ""));
        assert!(
            list["secrets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["name"] == SECRET && row["credentialStores"] == json!(["native"])),
            "{list}"
        );
        let call = ["tool", "call", "check", "env", &format!("name={SECRET}")];
        let call = ok(&run(&home, &program, &call, ""));
        assert_eq!(call["content"][0]["text"], "check value", "{program:?}");
    }

    ncp_ok(&home, &["secret", "delete", SECRET, "--all"]);
    let list = ncp_ok(&home, &["secret", "list"]);
    assert!(!list.to_string().contains(SECRET), "{list}");
}

/// The values of the `Authorization` headers in the trace of a run.
fn authorization_values(run: &Run) -> Vec<String> {
    run.stderr()
        .iter()
        .filter_map(|line| line["trace"]["http"]["headers"].as_object())
        .flat_map(|headers| headers.iter())
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.as_str().unwrap().to_string())
        .collect()
}

/// The name of the test server. The Keychain items of `ncp` have the
/// server name in their key, so the name must not be one of the user.
const LIVE_SERVER: &str = "ncp-live-cf";

/// The user approved this server on 2026-10-11, for the manual tests of
/// `server add`.
#[test]
#[ignore = "needs a browser and a Cloudflare account"]
fn oauth_login_with_a_browser_keeps_the_tokens_in_the_native_store() {
    // `server add` writes a new `.mcp.json` in a project folder, which
    // the `.git` folder marks, as for a user in a repository.
    let mut home = live_home("oauth");
    home.cwd = home.root.join("project");
    std::fs::create_dir_all(home.cwd.join(".git")).unwrap();
    // `server remove` removes the tokens and the client of the server.
    let _cleanup = Cleanup {
        home: &home,
        args: &["server", "remove", LIVE_SERVER],
    };
    let url = "https://mcp.cloudflare.com/mcp";
    let added = ncp_ok(&home, &["server", "add", LIVE_SERVER, url]);
    assert_eq!(added["created"], true, "{added}");

    say(
        "Login 1 of 2. The browser opens by itself. Log in to Cloudflare, and give the access. \
         Then wait. The test continues by itself.",
    );
    let login = login_with_person(&home, &["auth", "login", LIVE_SERVER]);
    assert!(
        login.contains("The tokens are in the native store."),
        "{login}"
    );
    let status = ncp_ok(&home, &["auth", "status", LIVE_SERVER]);
    assert_eq!(status["servers"][0]["loggedIn"], true, "{status}");
    let tools = ncp_ok(&home, &["tool", "list", LIVE_SERVER]);
    assert!(!tools["tools"].as_array().unwrap().is_empty(), "{tools}");

    say(
        "Login 2 of 2. The browser does not open. Copy the URL below, and open it in a browser. \
         Log in, and give the access. Then the browser goes to a page on 127.0.0.1. \
         If that page loads, wait: the test continues by itself. \
         If it does not load, copy its URL, paste it here, and press Enter.",
    );
    let no_browser = ["auth", "login", LIVE_SERVER, "--no-browser"];
    let login = login_with_person(&home, &no_browser);
    assert!(
        login.contains("The tokens are in the native store."),
        "{login}"
    );

    let traced = run(&home, &ncp(), &["-vv", "tool", "list", LIVE_SERVER], "");
    assert_eq!(traced.code(), 0, "{}", traced.describe());
    let values = authorization_values(&traced);
    assert!(!values.is_empty(), "no Authorization header in the trace");
    assert!(
        values.iter().all(|value| value == ncp::redact::HIDDEN),
        "the trace shows a token"
    );

    let removed = ncp_ok(&home, &["server", "remove", LIVE_SERVER]);
    assert_eq!(removed["credentialStores"], json!(["native"]), "{removed}");
    let status = ncp_ok(&home, &["auth", "status"]);
    assert!(!status.to_string().contains(LIVE_SERVER), "{status}");
}

const NETWORKSETUP: &str = "/usr/sbin/networksetup";

/// The port of the HTTPS proxy of the system settings, when it is on and
/// at 127.0.0.1.
fn system_https_proxy() -> Option<u16> {
    let output = Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let field = |key: &str| {
        text.lines()
            .filter_map(|line| line.trim().split_once(" : "))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    };
    let on = field("HTTPSEnable").as_deref() == Some("1");
    let local = field("HTTPSProxy").as_deref() == Some("127.0.0.1");
    field("HTTPSPort")
        .and_then(|port| port.parse().ok())
        .filter(|_| on && local)
}

/// The network services that are on. The first line of the list is a
/// note, and a service that is off starts with `*`.
fn network_services() -> Vec<String> {
    let output = Command::new(NETWORKSETUP)
        .arg("-listallnetworkservices")
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty() && !line.starts_with('*'))
        .map(str::to_string)
        .collect()
}

/// The HTTPS proxy of a service: on or off, and the stored server and
/// port, if any.
fn secure_web_proxy(service: &str) -> (bool, Option<(String, String)>) {
    let output = Command::new(NETWORKSETUP)
        .args(["-getsecurewebproxy", service])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map_or_else(String::new, |value| value.trim().to_string())
    };
    let server = field("Server:");
    let stored = (!server.is_empty()).then(|| (server, field("Port:")));
    (field("Enabled:") == "Yes", stored)
}

/// `networksetup` with root rights. `sudo` asks for the password on the
/// terminal. A CI runner has `sudo` with no password.
fn networksetup_as_root(args: &[&str]) -> bool {
    Command::new("/usr/bin/sudo")
        .arg(NETWORKSETUP)
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

/// The HTTPS proxy of the system settings for one test. It goes on for each
/// network service that is on, so that the service in use has it. The drop
/// sets each service back, also after a failure.
struct SystemProxy {
    /// Each service, with the server and port that it had stored.
    services: Vec<(String, Option<(String, String)>)>,
}

impl SystemProxy {
    fn set(port: u16) -> Self {
        let services = network_services()
            .into_iter()
            .map(|service| {
                let (on, stored) = secure_web_proxy(&service);
                // The test cannot set back the password of a proxy, so it
                // does not change a proxy that is in use.
                assert!(
                    !on,
                    "{service} uses an HTTPS proxy: the test does not change it"
                );
                (service, stored)
            })
            .collect();
        let proxy = Self { services };
        let port_text = port.to_string();
        for (service, _) in &proxy.services {
            let set = ["-setsecurewebproxy", service, "127.0.0.1", &port_text];
            assert!(
                networksetup_as_root(&set),
                "cannot set the proxy of {service}"
            );
        }
        // The system settings change in the background.
        let deadline = Instant::now() + Duration::from_secs(10);
        while system_https_proxy() != Some(port) {
            assert!(
                Instant::now() < deadline,
                "scutil --proxy does not show the proxy"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        proxy
    }
}

impl Drop for SystemProxy {
    fn drop(&mut self) {
        for (service, stored) in &self.services {
            if let Some((server, port)) = stored {
                networksetup_as_root(&["-setsecurewebproxy", service, server, port]);
            }
            if !networksetup_as_root(&["-setsecurewebproxystate", service, "off"]) {
                eprintln!(
                    "set the proxy back: sudo networksetup -setsecurewebproxystate '{service}' off"
                );
            }
        }
    }
}

/// The routes that `-v` shows.
fn routes(run: &Run) -> Vec<String> {
    run.stderr()
        .iter()
        .filter_map(|line| line["debug"]["message"].as_str())
        .filter(|message| message.starts_with("route to "))
        .map(str::to_string)
        .collect()
}

/// `ncp` reads the manual HTTPS proxy of the system settings when no
/// variable is set (design C5). The request goes through the tunnel of a
/// local proxy to a local HTTPS server, so nothing goes to the network, and
/// a CI runner can run the test with no person.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "changes the HTTPS proxy of the system settings with sudo"]
async fn system_proxy_carries_a_request_when_no_variable_is_set() {
    say(
        "This test sets the HTTPS proxy of the system settings to a test proxy. \
         It sets the proxy back at the end, also after a failure. sudo asks for your password. \
         While the test runs, the other HTTPS connections of this Mac fail. \
         If you stop the test with Ctrl-C, set the proxy back: \
         sudo networksetup -setsecurewebproxystate SERVICE off",
    );
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let proxy = ncp_testkit::serve_tunnel_proxy().await.unwrap();
    let _system = SystemProxy::set(proxy.port);
    let home = Home::new("system-proxy").set("SSL_CERT_FILE", ncp_testkit::TEST_CA);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));

    let run = home.run(&["-v", "tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let route = format!(
        "route to {}: proxy {}/ from the system settings",
        fixture.url, proxy.url
    );
    assert_eq!(routes(&run), [route]);
    let authority = fixture
        .url
        .trim_start_matches("https://")
        .trim_end_matches("/mcp");
    let lines = proxy.request_lines.lock().unwrap().clone();
    assert!(
        lines.contains(&format!("CONNECT {authority} HTTP/1.1")),
        "{lines:?}"
    );

    let home = home.set("HTTPS_PROXY", "http://127.0.0.1:9");
    let run = home.run(&["-v", "tool", "list", "web"]);
    let route = format!(
        "route to {}: proxy http://127.0.0.1:9/ from HTTPS_PROXY",
        fixture.url
    );
    assert_eq!(routes(&run), [route], "{}", run.describe());
}
