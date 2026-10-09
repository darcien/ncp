//! The adapter between the official MCP conformance suite and `ncp`.
//!
//! The suite starts this program with the URL of its server as the last
//! argument, and with the scenario in `MCP_CONFORMANCE_SCENARIO`, the
//! protocol version in `MCP_CONFORMANCE_PROTOCOL_VERSION` and extra data in
//! `MCP_CONFORMANCE_CONTEXT`. The adapter maps the scenario to `ncp`
//! commands, so that the suite tests the command line as a black box.
//!
//! The adapter runs `ncp-cli`, the build of `ncp` in the test kit, in a
//! temporary home with the file credential store and the fake browser,
//! which follows the redirect of the authorization server.
//!
//! The suite says that a client derives the lifecycle from the protocol
//! version it gets. `ncp` finds the era itself, so the adapter writes the
//! era of the version into the era cache before the first command.

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode, Output, Stdio},
};

use ncp::{
    config::Target,
    era::{Era, EraCache, ProtocolVersion},
    era_file,
    server::ServerName,
};
use serde_json::{Map, Value, json};

const SERVER: &str = "conformance";

/// The logins that one scenario can need: the first one and two step-ups.
const MAX_LOGINS: usize = 3;

/// Exit code 3 of `ncp`: a protocol, transport or local error.
const PROTOCOL_ERROR: i32 = 3;

/// Exit code 4 of `ncp`: the server needs a login or more scopes.
const NEEDS_LOGIN: i32 = 4;

struct Run {
    ncp: PathBuf,
    home: PathBuf,
}

impl Run {
    fn ncp(&self, args: &[&str]) -> Result<Output, String> {
        use std::io::Write as _;
        let home = self.home.display().to_string();
        let mut child = Command::new(&self.ncp)
            .args(args)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", format!("{home}/config"))
            .env("XDG_CACHE_HOME", format!("{home}/cache"))
            .env("XDG_STATE_HOME", format!("{home}/state"))
            .env("NCP_TEST_BROWSER", "follow")
            // A person at a TTY who takes each default, types y where a
            // value is necessary, and says yes to send.
            .env("NCP_TEST_TTY", "1")
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot run {}: {error}", self.ncp.display()))?;
        if let Some(mut stdin) = child.stdin.take() {
            let answers = "\n".repeat(20) + &"y\n".repeat(20);
            let _ = stdin.write_all(answers.as_bytes());
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("cannot run {}: {error}", self.ncp.display()))?;
        eprintln!(
            "$ ncp {}\nexit {:?}\n{}{}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    fn login(&self) -> Result<(), String> {
        let output = self.ncp(&["auth", "login", SERVER, "--credential-store", "file"])?;
        if output.status.success() {
            Ok(())
        } else {
            Err("auth login failed".into())
        }
    }

    /// Runs the command. When the server needs a login or more scopes, logs
    /// in and runs it again, as a user does after the hint.
    fn with_login(&self, args: &[&str], logins: &mut usize) -> Result<Output, String> {
        loop {
            let output = self.ncp(args)?;
            // A protocol or local error is a defect of ncp or of the server:
            // the adapter fails, so that the suite shows it.
            if output.status.code() == Some(PROTOCOL_ERROR) {
                return Err(format!("ncp {} exited with 3", args.join(" ")));
            }
            if output.status.code() != Some(NEEDS_LOGIN) || *logins >= MAX_LOGINS {
                return Ok(output);
            }
            *logins += 1;
            self.login()?;
        }
    }
}

/// A value for a property of the schema. The suite checks the types, not
/// the values.
fn sample(schema: &Value) -> Value {
    match schema["type"].as_str() {
        Some("number" | "integer") => json!(1),
        Some("boolean") => json!(true),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => json!("x"),
    }
}

/// The arguments of a call: each required property with a sample value.
fn arguments(tool: &Value) -> Value {
    let schema = &tool["inputSchema"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let properties: Map<String, Value> = required
        .iter()
        .map(|name| ((*name).to_string(), sample(&schema["properties"][*name])))
        .collect();
    Value::Object(properties)
}

/// The calls of the scenario: the `toolCalls` of the context, else one
/// call of each listed tool. The schema scenario asks for the input schema
/// of its focal tool, as `ncp tool list` shows it, back through its echo
/// tool: thus it checks that `ncp` keeps each keyword.
fn calls(scenario: &str, context: &Value, tools: &Value) -> Vec<(String, Value)> {
    if scenario == "json-schema-2020-12-preservation" {
        let listed = tools.as_array().cloned().unwrap_or_default();
        let (echo, focal): (Vec<Value>, Vec<Value>) = listed
            .into_iter()
            .partition(|tool| tool["inputSchema"]["properties"]["schema"].is_object());
        return echo
            .first()
            .zip(focal.first())
            .map(|(echo, focal)| {
                let name = echo["name"].as_str().unwrap_or_default().to_string();
                vec![(name, json!({"schema": focal["inputSchema"]}))]
            })
            .unwrap_or_default();
    }
    match context["toolCalls"].as_array() {
        Some(calls) => calls
            .iter()
            .map(|call| {
                (
                    call["name"].as_str().unwrap_or_default().to_string(),
                    call["arguments"].clone(),
                )
            })
            .collect(),
        None => tools
            .as_array()
            .map(|tools| {
                tools
                    .iter()
                    .map(|tool| {
                        (
                            tool["name"].as_str().unwrap_or_default().to_string(),
                            arguments(tool),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn configure(home: &Path, url: &str, version: &str, context: &Value) -> Result<(), String> {
    let config_dir = home.join("config/ncp");
    std::fs::create_dir_all(&config_dir).map_err(|error| error.to_string())?;
    let mut entry = json!({"type": "http", "url": url});
    if let Some(client_id) = context["client_id"].as_str() {
        entry["oauth"] = json!({"clientId": client_id});
    }
    let config = json!({"mcpServers": {SERVER: entry}});
    std::fs::write(config_dir.join("mcp.json"), config.to_string())
        .map_err(|error| error.to_string())?;
    // With NCP_CONFORMANCE_PROBE set, ncp finds the era itself, as it does
    // for a user.
    if std::env::var_os("NCP_CONFORMANCE_PROBE").is_some() {
        return Ok(());
    }
    let version = ProtocolVersion::parse(version).map_err(|error| error.to_string())?;
    let era = if version.is_modern() {
        Era::Modern { version }
    } else {
        Era::Legacy { version }
    };
    let name = ServerName::parse(SERVER).map_err(|error| error.to_string())?;
    let target = Target::Remote {
        url: url.to_string(),
    };
    let cache = EraCache::default().record(name, target, era);
    era_file::save(&home.join("cache/ncp/eras.json"), &cache).map_err(|error| error.to_string())
}

fn run() -> Result<(), String> {
    let url = std::env::args().next_back().ok_or("no server URL")?;
    let scenario = std::env::var("MCP_CONFORMANCE_SCENARIO").unwrap_or_default();
    let version =
        std::env::var("MCP_CONFORMANCE_PROTOCOL_VERSION").unwrap_or_else(|_| "2025-11-25".into());
    let context: Value = std::env::var("MCP_CONFORMANCE_CONTEXT")
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let home = std::env::temp_dir().join(format!("ncp-conformance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    configure(&home, &url, &version, &context)?;
    let run = Run {
        ncp: exe.with_file_name("ncp-cli"),
        home: home.clone(),
    };
    eprintln!("scenario {scenario}, version {version}, url {url}");
    let mut logins = 0;
    let listed = run.with_login(&["tool", "list", SERVER], &mut logins)?;
    let tools: Value = serde_json::from_slice(&listed.stdout).unwrap_or(Value::Null);
    for (name, arguments) in calls(&scenario, &context, &tools["tools"]) {
        let arguments = arguments.to_string();
        run.with_login(
            &["tool", "call", SERVER, &name, &arguments, "--no-validate"],
            &mut logins,
        )?;
    }
    let _ = std::fs::remove_dir_all(&home);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ncp-conformance: {error}");
            ExitCode::FAILURE
        }
    }
}
