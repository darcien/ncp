//! Tests of Ctrl-C and of the exit time of the command line.
//!
//! A test sends SIGINT to the `ncp` process only, as a terminal does to its
//! foreground process group. The stdio server is in its own process group,
//! so `ncp` must stop it.

// Test code can use unwrap: a panic is a test failure.
#![allow(clippy::unwrap_used)]

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::json;

const NCP: &str = env!("CARGO_BIN_EXE_ncp-cli");
const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");
const BLOCKED_STORE: &str = env!("CARGO_BIN_EXE_ncp-blocked-store");

struct Home {
    root: PathBuf,
}

impl Home {
    fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ncp-int-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/ncp")).unwrap();
        Self { root }
    }

    fn configure(&self, servers: &serde_json::Value) {
        let config = self.root.join("config/ncp/mcp.json");
        std::fs::write(config, json!({ "mcpServers": servers }).to_string()).unwrap();
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn spawn(&self, args: &[&str]) -> Child {
        Command::new(NCP)
            .args(args)
            .env_clear()
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(mode: &str, pid_file: &Path) -> serde_json::Value {
    json!({"command": FIXTURE, "args": [mode],
        "env": {"NCP_FIXTURE_PID_FILE": pid_file.display().to_string()}})
}

fn wait_for_file(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.is_empty()
        {
            return text;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no file {}", path.display())
}

fn signal(pid: u32, name: &str) -> bool {
    Command::new("kill")
        .args([name, &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn is_alive(pid: &str) -> bool {
    signal(pid.trim().parse().unwrap(), "-0")
}

// Sends SIGINT to `ncp`, and gives its exit code and the time to the exit.
fn interrupt(mut child: Child) -> (i32, Duration, String) {
    assert!(signal(child.id(), "-INT"));
    let start = Instant::now();
    let output = loop {
        if child.try_wait().unwrap().is_some() {
            break child.wait_with_output().unwrap();
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "ncp did not exit"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    (
        output.status.code().unwrap(),
        start.elapsed(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn ctrl_c_during_a_request_sends_cancel_stops_the_server_and_exits_130() {
    let home = Home::new("request");
    let pid_file = home.path("server.pid");
    let marker = home.path("cancelled");
    home.configure(&json!({"fx": fixture("modern", &pid_file)}));
    let started = home.path("started");
    let marker_arg = format!("marker={}", marker.display());
    let started_arg = format!("started={}", started.display());
    let child = home.spawn(&[
        "tool",
        "call",
        "fx",
        "wait",
        "ms=30000",
        &marker_arg,
        &started_arg,
    ]);
    let server = wait_for_file(&pid_file);
    // Interrupt only when the server runs the call, also on a busy host.
    wait_for_file(&started);
    let (code, elapsed, stderr) = interrupt(child);
    assert_eq!(code, 130, "{stderr}");
    assert!(elapsed < Duration::from_secs(6), "{elapsed:?}");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "cancelled");
    assert!(!is_alive(&server), "server {server} is alive");
    assert!(stderr.contains("\"interrupted\""), "{stderr}");
}

#[test]
fn ctrl_c_during_the_handshake_stops_the_server_and_exits_130() {
    let home = Home::new("handshake");
    let pid_file = home.path("server.pid");
    home.configure(&json!({"fx": fixture("slow-30000", &pid_file)}));
    let child = home.spawn(&["server", "show", "fx"]);
    let server = wait_for_file(&pid_file);
    std::thread::sleep(Duration::from_millis(300));
    let (code, elapsed, stderr) = interrupt(child);
    assert_eq!(code, 130, "{stderr}");
    assert!(elapsed < Duration::from_secs(6), "{elapsed:?}");
    assert!(!is_alive(&server), "server {server} is alive");
}

#[test]
fn blocked_store_call_does_not_delay_the_exit_by_more_than_3_seconds() {
    let start = Instant::now();
    let status = Command::new(BLOCKED_STORE).status().unwrap();
    assert!(status.success());
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn ctrl_c_during_a_question_exits_130() {
    let home = Home::new("question");
    let pid_file = home.path("server.pid");
    home.configure(&json!({"fx": fixture("modern", &pid_file)}));
    // A person at a TTY who does not answer: stdin stays open.
    let mut child = Command::new(NCP)
        .args(["tool", "call", "fx", "elicit", "text=x"])
        .env_clear()
        .env("HOME", &home.root)
        .env("XDG_CONFIG_HOME", home.root.join("config"))
        .env("NCP_TEST_TTY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take();
    let server = wait_for_file(&pid_file);
    // The question comes after the first round of the call.
    std::thread::sleep(Duration::from_millis(500));
    let (code, elapsed, stderr) = interrupt(child);
    drop(stdin);
    assert_eq!(code, 130, "{stderr}");
    assert!(stderr.contains("Who are you?"), "{stderr}");
    assert!(elapsed < Duration::from_secs(6), "{elapsed:?}");
    assert!(!is_alive(&server), "server {server} is alive");
}
