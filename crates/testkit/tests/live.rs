//! Live tests: `ncp` on this Mac, with the login Keychain, a real browser,
//! the system settings and a login at a real server. See
//! `docs/live-tests.md`.
//!
//! They change the state of the Mac, so only the user runs them. Thus they
//! are ignored by default. Run them with `mise run live`.

// The whole file is test code, where the agent rules permit unwrap.
#![allow(clippy::unwrap_used)]
#![cfg(target_os = "macos")]

mod common;

use std::{
    os::unix::process::CommandExt,
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

/// The value of a `key: value` line of `scutil` or `networksetup`.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim())
}

fn output_of(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The port of the HTTPS proxy of the system settings, when it is on and
/// at 127.0.0.1.
fn system_https_proxy() -> Option<u16> {
    let text = output_of("/usr/sbin/scutil", &["--proxy"]);
    let on = field(&text, "HTTPSEnable") == Some("1");
    let local = field(&text, "HTTPSProxy") == Some("127.0.0.1");
    field(&text, "HTTPSPort")
        .and_then(|port| port.parse().ok())
        .filter(|_| on && local)
}

/// The network services that are on. The first line of the list is a
/// note, and a service that is off starts with `*`.
fn network_services() -> Vec<String> {
    output_of(NETWORKSETUP, &["-listallnetworkservices"])
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty() && !line.starts_with('*'))
        .map(str::to_string)
        .collect()
}

/// The stored HTTPS proxy server of a service, when it is not the server of
/// this test. The test does not change such a proxy, because it cannot set
/// back its password.
fn other_https_proxy(service: &str) -> Option<String> {
    let text = output_of(NETWORKSETUP, &["-getsecurewebproxy", service]);
    field(&text, "Server")
        .filter(|server| !server.is_empty() && *server != "127.0.0.1")
        .map(str::to_string)
}

fn sudo(args: &[&str]) -> bool {
    Command::new("/usr/bin/sudo")
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

/// The HTTPS proxy of the system settings for one test, on each network
/// service that is on, so that the service in use has it. The drop turns
/// it off again. A watchdog with root rights turns it off too when the test
/// process ends with no drop, for example after Ctrl-C.
struct SystemProxy {
    services: Vec<String>,
}

impl SystemProxy {
    fn set(port: u16) -> Self {
        let services = network_services();
        for service in &services {
            if let Some(server) = other_https_proxy(service) {
                panic!("{service} has the HTTPS proxy {server}: the test does not change it");
            }
        }
        assert!(sudo(&["-v"]), "sudo did not accept the password");
        Self::watch(&services);
        let proxy = Self { services };
        let port_text = port.to_string();
        for service in &proxy.services {
            let set = [
                NETWORKSETUP,
                "-setsecurewebproxy",
                service,
                "127.0.0.1",
                &port_text,
            ];
            assert!(sudo(&set), "cannot set the proxy of {service}");
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

    /// Starts the watchdog in its own process group, so that the Ctrl-C of
    /// the terminal does not stop it. The names go in as arguments, so the
    /// shell does not read them.
    fn watch(services: &[String]) {
        let script = "test=$1; shift; \
            while kill -0 \"$test\" 2>/dev/null; do sleep 1; done; \
            for service in \"$@\"; do \
            /usr/sbin/networksetup -setsecurewebproxystate \"$service\" off; done";
        let pid = std::process::id().to_string();
        let started = Command::new("/usr/bin/sudo")
            .args(["-n", "/bin/sh", "-c", script, "watchdog", &pid])
            .args(services)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        assert!(started.is_ok(), "cannot start the watchdog");
    }
}

impl Drop for SystemProxy {
    fn drop(&mut self) {
        for service in &self.services {
            sudo(&[NETWORKSETUP, "-setsecurewebproxystate", service, "off"]);
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
/// local proxy to a local HTTPS server.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "changes the HTTPS proxy of the system settings with sudo"]
async fn system_proxy_carries_a_request_when_no_variable_is_set() {
    say(
        "sudo asks for your password. The test sets the HTTPS proxy of the system settings, and turns it off at the end, also when the test stops.",
    );
    let fixture = ncp_testkit::serve_https(Mode::Modern).await.unwrap();
    let proxy = ncp_testkit::serve_test_proxy().await.unwrap();
    let _system = SystemProxy::set(proxy.address.port());
    let home = Home::new("system-proxy").set("SSL_CERT_FILE", ncp_testkit::TEST_CA);
    home.configure(&json!({"web": {"type": "http", "url": fixture.url}}));

    let run = home.run(&["-v", "tool", "list", "web"]);
    assert_eq!(run.code(), 0, "{}", run.describe());
    let route = format!(
        "route to {}: proxy {}/ from the system settings",
        fixture.url,
        proxy.url()
    );
    assert_eq!(routes(&run), [route]);
    let lines = proxy.request_lines();
    assert!(
        lines.contains(&common::connect_line(&fixture.url)),
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
