//! Helpers for the tests that run the `ncp` binary.
//!
//! Each test runs the binary with a clean environment and its own `HOME`
//! and XDG directories. stdout is a pipe, so the output is JSON.

// Each test file uses only some of the helpers.
#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use serde_json::{Value, json};

pub const NCP: &str = env!("CARGO_BIN_EXE_ncp-cli");
const FAKE_SECURITY: &str = env!("CARGO_BIN_EXE_fake-security");

/// A temporary home for one test.
pub struct Home {
    pub root: PathBuf,
    pub env: Vec<(String, String)>,
    /// The current folder of each run. The home folder by default, where
    /// the search for a project file stops.
    pub cwd: PathBuf,
}

impl Home {
    pub fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ncp-cli-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let env = vec![
            ("HOME".into(), root.display().to_string()),
            (
                "XDG_CONFIG_HOME".into(),
                root.join("config").display().to_string(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                root.join("cache").display().to_string(),
            ),
            (
                "XDG_STATE_HOME".into(),
                root.join("state").display().to_string(),
            ),
        ];
        Self {
            cwd: root.clone(),
            root,
            env,
        }
    }

    /// Writes `.mcp.json` with the servers in the folder (relative to the
    /// root) with a `.git` folder, and runs the next commands there.
    pub fn project(mut self, folder: &str, servers: &Value) -> Self {
        let dir = self.root.join(folder);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            json!({ "mcpServers": servers }).to_string(),
        )
        .unwrap();
        self.cwd = dir;
        self
    }

    /// The path of the project file of the current folder, as `ncp` finds
    /// it: the real path.
    pub fn project_file(&self) -> PathBuf {
        std::fs::canonicalize(&self.cwd).unwrap().join(".mcp.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config/ncp/mcp.json")
    }

    pub fn configure(&self, servers: &Value) -> &Self {
        let path = self.config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json!({ "mcpServers": servers }).to_string()).unwrap();
        self
    }

    /// A fake `security` program as the native store, with its files in
    /// `keychain/`.
    pub fn with_fake_keychain(self) -> Self {
        let dir = self.root.join("keychain");
        std::fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(FAKE_SECURITY, dir.join("security")).unwrap();
        let program = dir.join("security").display().to_string();
        self.set("NCP_TEST_SECURITY_PROGRAM", &program)
    }

    /// Writes an item to the credential file, with no entry that names it.
    /// `secret set` writes only an item that an entry uses.
    pub fn store_in_file(&self, account: &str, value: &str) {
        use ncp::store::CredentialStore as _;
        let key = ncp::credential::CredentialKey::from_account(account).unwrap();
        ncp::store::FileStore::new(self.root.join("state/ncp/credentials.json"))
            .save(&key, &ncp::credential::Secret::new(value.into()))
            .unwrap();
    }

    /// The accounts of the items in the credential file.
    pub fn file_accounts(&self) -> Vec<String> {
        use ncp::store::CredentialStore as _;
        ncp::store::FileStore::new(self.root.join("state/ncp/credentials.json"))
            .keys()
            .unwrap()
            .iter()
            .map(ncp::credential::CredentialKey::account)
            .collect()
    }

    pub fn set(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.into(), value.into()));
        self
    }

    pub fn unset(mut self, name: &str) -> Self {
        self.env.retain(|(key, _)| key != name);
        self
    }

    pub fn run(&self, args: &[&str]) -> Run {
        self.run_with_stdin(args, "")
    }

    /// The program in the current folder, with only the environment of
    /// this home.
    pub fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.cwd)
            .env_clear()
            .envs(self.env.iter().cloned());
        command
    }

    pub fn run_with_stdin(&self, args: &[&str], stdin: &str) -> Run {
        self.run_program(Path::new(NCP), args, stdin)
    }

    pub fn run_program(&self, program: &Path, args: &[&str], stdin: &str) -> Run {
        use std::io::Write as _;
        let mut child = self
            .command(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // A command can exit before it reads stdin, for example after a
        // usage error. The write then fails with a broken pipe, which is
        // not a test failure.
        let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
        Run(child.wait_with_output().unwrap())
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The `CONNECT` line of a request to the server through a proxy.
pub fn connect_line(server_url: &str) -> String {
    let url = url::Url::parse(server_url).unwrap();
    let host = url.host_str().unwrap();
    let port = url.port_or_known_default().unwrap();
    format!("CONNECT {host}:{port} HTTP/1.1")
}

pub struct Run(pub Output);

impl Run {
    pub fn code(&self) -> i32 {
        self.0.status.code().unwrap()
    }

    pub fn stdout(&self) -> Value {
        serde_json::from_slice(&self.0.stdout)
            .unwrap_or_else(|_| panic!("stdout is not JSON: {}", self.describe()))
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.0.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.0.stderr).into_owned()
    }

    pub fn stdout_is_empty(&self) -> bool {
        self.0.stdout.is_empty()
    }

    /// The JSON Lines on stderr.
    pub fn stderr(&self) -> Vec<Value> {
        String::from_utf8_lossy(&self.0.stderr)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
            .collect()
    }

    pub fn error(&self) -> Value {
        self.stderr()
            .into_iter()
            .find_map(|line| line.get("error").cloned())
            .unwrap_or_else(|| panic!("no error: {}", self.describe()))
    }

    pub fn describe(&self) -> String {
        format!(
            "exit {:?}\nstdout: {}\nstderr: {}",
            self.0.status.code(),
            String::from_utf8_lossy(&self.0.stdout),
            String::from_utf8_lossy(&self.0.stderr)
        )
    }
}
