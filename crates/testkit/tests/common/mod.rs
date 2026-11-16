//! Helpers for the tests that run the `ncp` binary.
//!
//! Each test runs the binary with a clean environment, its own `HOME` and
//! XDG directories, and no proxy. stdout is a pipe. `Home::ncp` keeps the
//! default text output. `Home::run` adds `--output-format json`.

// Each test file uses only some of the helpers.
#![allow(dead_code)]

use std::{
    ffi::OsStr,
    os::unix::process::CommandExt as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::LazyLock,
};

use regex::{Captures, Regex};
use serde_json::{Value, json};

pub const NCP: &str = env!("CARGO_BIN_EXE_ncp-cli");
const FAKE_SECURITY: &str = env!("CARGO_BIN_EXE_fake-security");

/// Compares the filtered transcript of one run with the inline snapshot.
/// Returns the run.
#[allow(unused_macros)]
macro_rules! ncp_snapshot {
    ($filters:expr, $command:expr, @$snapshot:literal) => {{
        let (snapshot, run) = $crate::common::run_and_format($command, &$filters, "");
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        run
    }};
    ($filters:expr, $command:expr, input = $input:expr, @$snapshot:literal) => {{
        let (snapshot, run) = $crate::common::run_and_format($command, &$filters, $input);
        ::insta::assert_snapshot!(snapshot, @$snapshot);
        run
    }};
}
#[allow(unused_imports)]
pub(crate) use ncp_snapshot;

/// A temporary home for one test.
pub struct Home {
    pub root: PathBuf,
    pub env: Vec<(String, String)>,
    /// The current directory of each run. The home directory by default, where
    /// the search for a project file stops.
    pub cwd: PathBuf,
    filters: Vec<(String, String)>,
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
            // `ncp` also reads the proxy of the macOS system settings, which
            // the environment does not hold. `*` matches each host name but
            // no IP address. A test of a proxy removes this value.
            ("NO_PROXY".into(), "*,127.0.0.1,::1".into()),
        ];
        Self {
            filters: standard_filters(&root),
            cwd: root.clone(),
            root,
            env,
        }
    }

    /// The path filters, then the filters of measured values.
    pub fn filters(&self) -> Vec<(&str, &str)> {
        self.filters
            .iter()
            .map(|(pattern, replacement)| (pattern.as_str(), replacement.as_str()))
            .collect()
    }

    /// The command with text output and `ncp` as its program name.
    pub fn ncp<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(Path::new(NCP));
        command.arg0("ncp").args(args);
        command
    }

    pub fn read(&self, relative_path: &str) -> String {
        std::fs::read_to_string(self.root.join(relative_path)).unwrap()
    }

    /// Writes `.mcp.json` with the servers in the directory (relative to the
    /// root) with a `.git` directory, and runs the next commands there.
    pub fn project(mut self, relative_dir: &str, servers: &Value) -> Self {
        let dir = self.root.join(relative_dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            json!({ "mcpServers": servers }).to_string(),
        )
        .unwrap();
        self.cwd = dir;
        self
    }

    /// The path of the project file of the current directory, as `ncp` finds
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

    /// The program in the current directory, with only the environment of
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

    /// Runs `ncp` with `--output-format json`, because the tests read JSON.
    /// A test of text gives `--output-format text` itself. A completion
    /// command of the shell scripts takes no options.
    pub fn run_program(&self, program: &Path, args: &[&str], stdin: &str) -> Run {
        let json = !args.iter().any(|arg| arg.starts_with("--output-format"))
            && !args
                .first()
                .is_some_and(|arg| arg.starts_with("__complete-"));
        let format: &[&str] = if json {
            &["--output-format", "json"]
        } else {
            &[]
        };
        self.spawn(program, &[format, args].concat(), stdin)
    }

    /// Runs `ncp` with no `--output-format`, as a user who gives none.
    pub fn run_with_default_format(&self, args: &[&str]) -> Run {
        self.spawn(Path::new(NCP), args, "")
    }

    fn spawn(&self, program: &Path, args: &[&str], stdin: &str) -> Run {
        let mut command = self.command(program);
        command.args(args);
        Run(output_of(command, stdin))
    }
}

fn output_of(mut command: Command, stdin: &str) -> Output {
    use std::io::Write as _;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Usage errors can close stdin before the write.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}

pub fn run_and_format(command: Command, filters: &[(&str, &str)], stdin: &str) -> (String, Run) {
    let reports_version = reports_cli_version(&command);
    let output = output_of(command, stdin);
    let transcript = transcript(&output);
    eprintln!("----- transcript with no filter -----\n{transcript}");
    let transcript = if reports_version {
        apply_filters(transcript, &[CLI_VERSION_FILTER])
    } else {
        transcript
    };
    (
        apply_filters(normalize_timings(&transcript), filters),
        Run(output),
    )
}

/// Replaces each number on a `--timings` line, in text and JSON output, so
/// the count of times stays visible. A filter pair cannot do this: without
/// lookaround, a regex cannot limit a match to the numbers of one line.
pub fn normalize_timings(text: &str) -> String {
    static LINE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)^(?:timings: [^\n]*|\{"timings":\{[^{}\n]*\}\})$"#).unwrap()
    });
    static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?(?: ms)?").unwrap());
    LINE.replace_all(text, |line: &Captures| {
        TIME.replace_all(&line[0], "[TIME]").into_owned()
    })
    .into_owned()
}

/// The version line of `ncp --version` and of each subcommand, for example
/// `ncp-server 1.2.3`. Only a `--version` run gets this filter, so a tool
/// result with the same text stays exact.
const CLI_VERSION_FILTER: (&str, &str) = (
    r"(?m)(^----- stdout -----\nncp(?:-[a-z]+)* )\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)*(\n|$)",
    "${1}[VERSION]${2}",
);

fn reports_cli_version(command: &Command) -> bool {
    command.get_program() == OsStr::new(NCP)
        && command
            .get_args()
            .take_while(|arg| *arg != "--")
            .any(|arg| arg == "--version" || arg == "-V")
}

/// The exit status and each nonempty stream, without changes to whitespace.
pub fn transcript(output: &Output) -> String {
    let outcome = if output.status.success() {
        "success"
    } else {
        "failure"
    };
    let mut text = match output.status.code() {
        Some(code) => format!("exit_code: {code} ({outcome})\n"),
        None => format!(
            "exit_code: -1 ({outcome})\nexit_status: {}\n",
            output.status
        ),
    };
    let mut has_stream = false;
    for (name, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
        if !bytes.is_empty() {
            if has_stream {
                text.push_str(if text.ends_with('\n') { "\n" } else { "\n\n" });
            }
            text.push_str("----- ");
            text.push_str(name);
            text.push_str(" -----\n");
            text.push_str(&String::from_utf8_lossy(bytes));
            has_stream = true;
        }
    }
    text
}

/// Applies each regex once, in order.
pub fn apply_filters(text: String, filters: &[(&str, &str)]) -> String {
    filters.iter().fold(text, |text, (pattern, replacement)| {
        Regex::new(pattern)
            .unwrap_or_else(|error| panic!("the filter {pattern:?} is not a regex: {error}"))
            .replace_all(&text, *replacement)
            .into_owned()
    })
}

/// Normalizes missing-file messages only. A path or a cause before the
/// message stays, and other OS errors stay exact.
pub fn missing_file_filters() -> Vec<(&'static str, &'static str)> {
    vec![(
        r#"(?m)(^|: |")[^:"\n]* \(os error [23]\)"#,
        "${1}[OS ERROR 2]",
    )]
}

/// Normalizes the native refused-connection causes on supported systems.
pub fn connection_refused_filters() -> Vec<(&'static str, &'static str)> {
    vec![(
        r"(?:Connection refused \(os error (?:61|111)\)|No connection could be made because the target machine actively refused it\. \(os error 10061\))",
        "[CONNECTION REFUSED]",
    )]
}

fn standard_filters(home: &Path) -> Vec<(String, String)> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut paths = [
        (Path::new(NCP), "NCP", false),
        (
            Path::new(env!("CARGO_BIN_EXE_ncp-fixture")),
            "FIXTURE",
            false,
        ),
        (Path::new(FAKE_SECURITY), "FAKE_SECURITY", false),
        (
            Path::new(env!("CARGO_BIN_EXE_ncp-blocked-store")),
            "BLOCKED_STORE",
            false,
        ),
        (home, "HOME", true),
        (workspace, "WORKSPACE", true),
    ]
    .into_iter()
    .map(|(path, name, directory)| path_filters(path, name, directory))
    .collect::<Vec<_>>();
    // Longest paths first, so that a parent or an alias cannot replace part
    // of a longer path.
    paths.sort_by_key(|group| {
        std::cmp::Reverse(
            group
                .iter()
                .map(|(pattern, _)| pattern.len())
                .max()
                .unwrap(),
        )
    });
    let mut filters = paths.into_iter().flatten().collect::<Vec<_>>();
    filters.push((r"\b127\.0\.0\.1:[0-9]+\b".into(), "[LOCALHOST]".into()));
    filters.extend([
        (
            r"(?m)^(trace: server [^\n]+: [A-Z]+ https?://\S+ \[[^\n]*\]: [^\n]* in )\d+(?:\.\d+)? ms$",
            "${1}[TIME]",
        ),
        (
            r#"(?m)^(\{"trace":\{"http":\{"headers":\{[^{}\n]*\},"method":"[A-Z]+","ms":)\d+(?:\.\d+)?"#,
            "${1}[TIME]",
        ),
        (
            r#"(?m)^((?:debug: server [^\n]+: |\{"debug":\{"message":")tools from the list cache, )\d+ s old"#,
            "${1}[N] s old",
        ),
        (
            r#"(?m)^((?:trace: server [^\n]+: sent |\{"trace":\{"sent":)\{(?:"id":(?:\d+|"[^"]+"),)?"jsonrpc":"2\.0","method":"[^"]+","params":\{"_meta":\{(?:"io\.modelcontextprotocol/clientCapabilities":\{[^\n]*?\},)?"io\.modelcontextprotocol/clientInfo":\{"name":"ncp","version":")[^"]+(")"#,
            "${1}[VERSION]${2}",
        ),
        (
            r#"(?m)^((?:trace: server [^\n]+: sent |\{"trace":\{"sent":)\{(?:"id":(?:\d+|"[^"]+"),)?"jsonrpc":"2\.0","method":"initialize","params":\{"capabilities":\{[^\n]*?\},"clientInfo":\{"name":"ncp","version":")[^"]+(")"#,
            "${1}[VERSION]${2}",
        ),
    ].into_iter().map(|(pattern, replacement)| (pattern.into(), replacement.into())));
    filters
}

/// Replaces the real path and the given path with `[NAME]`. The delimiters
/// around each match stay.
pub fn path_filters(path: &Path, name: &str, directory: bool) -> Vec<(String, String)> {
    let real = std::fs::canonicalize(path).unwrap();
    let mut paths = vec![real];
    if paths[0] != path {
        paths.push(path.to_path_buf());
    }
    paths
        .into_iter()
        .flat_map(|path| {
            let boundary = if directory {
                r#"(/|[\s"'`,;:)\]}>]|$)"#
            } else {
                r#"([\s"'`,;:)\]}>]|$)"#
            };
            let filter = (
                format!(
                    r#"(?m)(^|[\s"'`=(:,\[<]){}{boundary}"#,
                    regex::escape(&path.to_string_lossy())
                ),
                format!("${{1}}[{name}]${{2}}"),
            );
            // A match consumes its delimiters. The second pass covers adjacent paths.
            [filter.clone(), filter]
        })
        .collect()
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

    /// The error object, after a check of the message rules.
    pub fn error(&self) -> Value {
        let error = self
            .stderr()
            .into_iter()
            .find_map(|line| line.get("error").cloned())
            .unwrap_or_else(|| panic!("no error: {}", self.describe()));
        check_style(&error, &self.describe());
        error
    }

    /// The warning objects, after a check of the message rules.
    pub fn warnings(&self) -> Vec<Value> {
        let warnings: Vec<Value> = self
            .stderr()
            .into_iter()
            .filter_map(|line| line.get("warning").cloned())
            .collect();
        for warning in &warnings {
            check_style(warning, &self.describe());
        }
        warnings
    }

    /// The hint messages of the error, in their order.
    pub fn hints(&self) -> Vec<String> {
        hints(&self.error())
    }

    /// The message, the data lines and the causes of the error, one on
    /// each line. A check of a text that can be in the message or in a
    /// cause reads this.
    pub fn error_text(&self) -> String {
        error_text(&self.error())
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

/// The hint messages of an error or a warning object.
pub fn hints(object: &Value) -> Vec<String> {
    object["hints"]
        .as_array()
        .unwrap_or_else(|| panic!("no hints: {object}"))
        .iter()
        .map(|hint| hint["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The message, the data lines and the causes of an error or a warning
/// object, one on each line.
pub fn error_text(object: &Value) -> String {
    let strings = |key: &str| {
        object[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    std::iter::once(object["message"].as_str().unwrap_or_default().to_string())
        .chain(strings("data"))
        .chain(strings("causes"))
        .collect::<Vec<_>>()
        .join("\n")
}

// The rules of the text of a diagnostic that a test can check on the output:
// the first letter, no semicolon, no hint two times, the server in the
// message, and no empty backticks.
fn check_style(object: &Value, run: &str) {
    let message = object["message"].as_str().unwrap_or_default();
    let hints = hints(object);
    for text in std::iter::once(message).chain(hints.iter().map(String::as_str)) {
        let first = text.chars().next();
        assert!(
            first.is_some_and(|c| c.is_uppercase() || c == '`') || text.starts_with("ncp"),
            "{text:?} does not start with a capital letter, a backtick or ncp\n{run}"
        );
        // A value in backticks, for example a command of the user, can
        // hold a semicolon.
        let own_text: String = text.split('`').step_by(2).collect();
        assert!(
            !own_text.contains(';'),
            "{text:?} has a semicolon outside backticks\n{run}"
        );
        assert!(!text.contains("``"), "{text:?} has empty backticks\n{run}");
    }
    let mut unique = hints.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), hints.len(), "a hint shows two times\n{run}");
    if let Some(server) = object["server"].as_str() {
        assert!(
            message.contains(&format!("`{server}`")),
            "{message:?} does not name the server {server} in backticks\n{run}"
        );
    }
}
