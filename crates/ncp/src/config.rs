//! The configuration model: the common `mcpServers` JSON core.
//!
//! Other agent tools write the same shape, so `ncp` reads their files too.
//! Unknown keys give warnings and not errors. An invalid entry does not stop
//! the other entries: its error shows when the user selects that server.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    expand::{self, Env, ExpandError, Template, expand},
    server::{NameError, ServerName, ServerUrl, UrlError},
};

const SERVERS_KEY: &str = "mcpServers";
const STDIO_KEYS: [&str; 5] = ["type", "command", "args", "env", "cwd"];
const REMOTE_KEYS: [&str; 3] = ["type", "url", "headers"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: ServerName,
    /// The values after `${NAME}` expansion.
    pub transport: Transport,
    /// The target as the file writes it, before expansion. Output and the
    /// era cache use it, so an expanded value never goes to a file or to
    /// the screen.
    pub target: Target,
}

impl Server {
    /// A server with no variables, so the target is the transport.
    #[must_use]
    pub fn new(name: ServerName, transport: Transport) -> Self {
        let target = match &transport {
            Transport::Stdio(stdio) => Target::Stdio {
                command: stdio.command.clone(),
                args: stdio.args.clone(),
                cwd: stdio.cwd.clone(),
            },
            Transport::Http(remote) | Transport::Sse(remote) => Target::Remote {
                url: remote.url.as_str().to_string(),
            },
        };
        Self {
            name,
            transport,
            target,
        }
    }
}

/// One `env` or `headers` value after expansion. A value from `${NAME}` can
/// be a secret, so `Debug` hides it, and HTTP marks it sensitive.
#[derive(Clone, PartialEq, Eq)]
pub struct Expanded {
    pub value: String,
    pub from_variable: bool,
}

impl std::fmt::Debug for Expanded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.from_variable {
            f.write_str("Expanded(<hidden>)")
        } else {
            write!(f, "Expanded({:?})", self.value)
        }
    }
}

/// The `env` values of a stdio server or the `headers` of an HTTP server,
/// after expansion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    pub env: BTreeMap<String, Expanded>,
    pub headers: BTreeMap<String, Expanded>,
}

impl Server {
    /// The names in the `env` or `headers` values, in order. Their values
    /// can come from a credential store.
    #[must_use]
    pub fn value_names(&self) -> BTreeSet<&str> {
        self.templates()
            .flat_map(|(_, _, template)| template.names())
            .collect()
    }

    /// Expands the `env` or `headers` values with `values`.
    ///
    /// # Errors
    ///
    /// Returns an error that names the field when a name has no value.
    pub fn expand_values(&self, values: &Env) -> Result<Values, EntryError> {
        let mut expanded = Values::default();
        for (field, key, template) in self.templates() {
            let value = Expanded {
                value: template
                    .expand(values)
                    .map_err(|error| EntryError::Variable {
                        field: format!("{field}.{key}"),
                        error,
                    })?,
                from_variable: template.has_variables(),
            };
            match field {
                "env" => expanded.env.insert(key.clone(), value),
                _ => expanded.headers.insert(key.clone(), value),
            };
        }
        Ok(expanded)
    }

    fn templates(&self) -> impl Iterator<Item = (&'static str, &String, &Template)> {
        let (field, map) = match &self.transport {
            Transport::Stdio(stdio) => ("env", &stdio.env),
            Transport::Http(remote) | Transport::Sse(remote) => ("headers", &remote.headers),
        };
        map.iter()
            .map(move |(key, template)| (field, key, template))
    }
}

/// The parts of a server configuration that identify the server process or
/// origin. Environment values and headers are not part of it: they can hold
/// secrets, and they do not change the server program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum Target {
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    Remote {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Stdio(StdioServer),
    Http(RemoteServer),
    /// Read from the file, but not supported for connections in the MVP.
    Sse(RemoteServer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdioServer {
    pub command: String,
    pub args: Vec<String>,
    /// Not expanded: a value can come from a credential store.
    pub env: BTreeMap<String, Template>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteServer {
    pub url: ServerUrl,
    /// Not expanded: a value can come from a credential store.
    pub headers: BTreeMap<String, Template>,
}

/// An unknown key. `server` is `None` for a key at the top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub server: Option<String>,
    pub key: String,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.server {
            Some(server) => write!(f, "server {server}: unknown key {:?} ignored", self.key),
            None => write!(f, "unknown key {:?} ignored", self.key),
        }
    }
}

/// An error in the file as a whole. No server can load.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration is not a JSON object")]
    NotObject,
    #[error("\"{SERVERS_KEY}\" is not a JSON object")]
    ServersNotObject,
}

/// An error in one server entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EntryError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error("entry is not a JSON object")]
    NotObject,
    #[error("entry has a \"url\" but no \"type\"; add \"type\": \"http\" or \"type\": \"sse\"")]
    UrlWithoutType,
    #[error("entry has no \"command\" and no \"url\"")]
    NoTransport,
    #[error("type {0:?} is not \"stdio\", \"http\" or \"sse\"")]
    UnknownType(String),
    #[error("\"{0}\" is missing")]
    Missing(&'static str),
    #[error("\"{field}\" must be {expected}")]
    WrongType {
        field: String,
        expected: &'static str,
    },
    #[error("\"url\": {0}")]
    Url(#[from] UrlError),
    #[error("\"{field}\": {error}")]
    Variable { field: String, error: ExpandError },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    #[error("no server {0:?} in the configuration")]
    NotFound(String),
    #[error("server {name}: {error}")]
    Invalid { name: String, error: EntryError },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    entries: BTreeMap<String, Result<Server, EntryError>>,
    warnings: Vec<Warning>,
    variables: BTreeSet<String>,
}

impl Config {
    /// # Errors
    ///
    /// Returns an error when no server of that name exists, or when its entry
    /// is invalid.
    pub fn server(&self, name: &str) -> Result<&Server, LookupError> {
        match self.entries.get(name) {
            None => Err(LookupError::NotFound(name.to_string())),
            Some(Ok(server)) => Ok(server),
            Some(Err(error)) => Err(LookupError::Invalid {
                name: name.to_string(),
                error: error.clone(),
            }),
        }
    }

    pub fn servers(&self) -> impl Iterator<Item = &Server> {
        self.entries
            .values()
            .filter_map(|entry| entry.as_ref().ok())
    }

    pub fn invalid(&self) -> impl Iterator<Item = (&str, &EntryError)> {
        self.entries
            .iter()
            .filter_map(|(name, entry)| entry.as_ref().err().map(|e| (name.as_str(), e)))
    }

    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// The names of all `${NAME}` in `env` and `headers` values, also in
    /// invalid entries. Only these values can come from a store.
    #[must_use]
    pub fn variables(&self) -> &BTreeSet<String> {
        &self.variables
    }
}

/// The names of the variables in each string value of a JSON value. Keys
/// do not expand, so their names do not count.
fn variables_in(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => found.extend(expand::names(text).into_iter().map(str::to_string)),
        Value::Array(items) => items.iter().for_each(|item| variables_in(item, found)),
        Value::Object(map) => map.values().for_each(|item| variables_in(item, found)),
        _ => {}
    }
}

/// Parses the configuration text. `env` gives the values for `${VAR}`.
///
/// # Errors
///
/// Returns an error when the text is not JSON, or does not have the
/// `mcpServers` shape.
pub fn parse(text: &str, env: &Env) -> Result<Config, ConfigError> {
    let root: Value = serde_json::from_str(text)?;
    let root = root.as_object().ok_or(ConfigError::NotObject)?;
    let top_warnings = root
        .keys()
        .filter(|key| *key != SERVERS_KEY)
        .map(|key| Warning {
            server: None,
            key: key.clone(),
        });
    let servers = match root.get(SERVERS_KEY) {
        None => &Map::new(),
        Some(value) => value.as_object().ok_or(ConfigError::ServersNotObject)?,
    };
    let parsed: Vec<(String, Result<Server, EntryError>, Vec<Warning>)> = servers
        .iter()
        .map(|(name, value)| {
            let (entry, warnings) = parse_entry(name, value, env);
            (name.clone(), entry, warnings)
        })
        .collect();
    let warnings = top_warnings
        .chain(parsed.iter().flat_map(|(_, _, w)| w.iter().cloned()))
        .collect();
    let entries = parsed
        .into_iter()
        .map(|(name, entry, _)| (name, entry))
        .collect();
    let mut variables = BTreeSet::new();
    // Only the field of the transport expands: `headers` in a stdio entry
    // is an unknown key.
    servers
        .values()
        .filter_map(Value::as_object)
        .filter_map(|entry| {
            let field = match kind_of(entry).ok()? {
                Kind::Stdio => "env",
                Kind::Http | Kind::Sse => "headers",
            };
            entry.get(field)
        })
        .for_each(|values| variables_in(values, &mut variables));
    Ok(Config {
        entries,
        warnings,
        variables,
    })
}

#[derive(Clone, Copy)]
enum Kind {
    Stdio,
    Http,
    Sse,
}

fn parse_entry(name: &str, value: &Value, env: &Env) -> (Result<Server, EntryError>, Vec<Warning>) {
    let head = ServerName::parse(name)
        .map_err(EntryError::from)
        .and_then(|name| {
            let object = value.as_object().ok_or(EntryError::NotObject)?;
            Ok((name, object, kind_of(object)?))
        });
    match head {
        Err(error) => (Err(error), Vec::new()),
        Ok((server_name, object, kind)) => {
            let warnings = unknown_keys(name, object, kind);
            let server = transport(object, kind, env).map(|transport| Server {
                name: server_name,
                transport,
                target: configured_target(object, kind),
            });
            (server, warnings)
        }
    }
}

fn kind_of(object: &Map<String, Value>) -> Result<Kind, EntryError> {
    match object.get("type") {
        Some(Value::String(text)) => match text.as_str() {
            "stdio" => Ok(Kind::Stdio),
            "http" => Ok(Kind::Http),
            "sse" => Ok(Kind::Sse),
            other => Err(EntryError::UnknownType(other.to_string())),
        },
        Some(_) => Err(wrong_type("type", "a string")),
        None if object.contains_key("url") => Err(EntryError::UrlWithoutType),
        None if object.contains_key("command") => Ok(Kind::Stdio),
        None => Err(EntryError::NoTransport),
    }
}

fn unknown_keys(server: &str, object: &Map<String, Value>, kind: Kind) -> Vec<Warning> {
    let known: &[&str] = match kind {
        Kind::Stdio => &STDIO_KEYS,
        Kind::Http | Kind::Sse => &REMOTE_KEYS,
    };
    object
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .map(|key| Warning {
            server: Some(server.to_string()),
            key: key.clone(),
        })
        .collect()
}

/// The target with no expansion. The transport parsed first, so each field
/// has the right type.
fn configured_target(object: &Map<String, Value>, kind: Kind) -> Target {
    let text = |field: &str| {
        object
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    match kind {
        Kind::Stdio => Target::Stdio {
            command: text("command").unwrap_or_default(),
            args: object
                .get("args")
                .and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            cwd: text("cwd"),
        },
        Kind::Http | Kind::Sse => Target::Remote {
            url: text("url").unwrap_or_default(),
        },
    }
}

fn transport(object: &Map<String, Value>, kind: Kind, env: &Env) -> Result<Transport, EntryError> {
    match kind {
        Kind::Stdio => stdio(object, env).map(Transport::Stdio),
        Kind::Http => remote(object, env).map(Transport::Http),
        Kind::Sse => remote(object, env).map(Transport::Sse),
    }
}

fn stdio(object: &Map<String, Value>, env: &Env) -> Result<StdioServer, EntryError> {
    let command = string(object, "command", env)?.ok_or(EntryError::Missing("command"))?;
    Ok(StdioServer {
        command,
        args: strings(object, "args", env)?,
        env: template_map(object, "env")?,
        cwd: string(object, "cwd", env)?,
    })
}

fn remote(object: &Map<String, Value>, env: &Env) -> Result<RemoteServer, EntryError> {
    let url = string(object, "url", env)?.ok_or(EntryError::Missing("url"))?;
    Ok(RemoteServer {
        url: ServerUrl::parse(&url)?,
        headers: template_map(object, "headers")?,
    })
}

fn string(
    object: &Map<String, Value>,
    field: &str,
    env: &Env,
) -> Result<Option<String>, EntryError> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::String(text)) => expand_field(field, text, env).map(Some),
        Some(_) => Err(wrong_type(field, "a string")),
    }
}

fn strings(object: &Map<String, Value>, field: &str, env: &Env) -> Result<Vec<String>, EntryError> {
    match object.get(field) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let field = format!("{field}[{index}]");
                item.as_str()
                    .ok_or_else(|| wrong_type(&field, "a string"))
                    .and_then(|text| expand_field(&field, text, env))
            })
            .collect(),
        Some(_) => Err(wrong_type(field, "an array of strings")),
    }
}

/// A map of values that expand only when the server starts. The syntax of
/// each value is checked now.
fn template_map(
    object: &Map<String, Value>,
    field: &str,
) -> Result<BTreeMap<String, Template>, EntryError> {
    match object.get(field) {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, value)| {
                let field = format!("{field}.{key}");
                let text = value
                    .as_str()
                    .ok_or_else(|| wrong_type(&field, "a string"))?;
                Template::parse(text)
                    .map(|template| (key.clone(), template))
                    .map_err(|error| EntryError::Variable { field, error })
            })
            .collect(),
        Some(_) => Err(wrong_type(field, "an object of strings")),
    }
}

fn expand_field(field: &str, text: &str, env: &Env) -> Result<String, EntryError> {
    expand(text, env).map_err(|error| EntryError::Variable {
        field: field.to_string(),
        error,
    })
}

fn wrong_type(field: &str, expected: &'static str) -> EntryError {
    EntryError::WrongType {
        field: field.to_string(),
        expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("PORT".to_string(), "8080".to_string()),
        ])
    }

    fn load(text: &str) -> Config {
        parse(text, &env()).unwrap()
    }

    #[test]
    fn reads_stdio_and_http_entries() {
        let config = load(
            r#"{"mcpServers": {
                "local": {"type": "stdio", "command": "srv", "args": ["-v"], "env": {"A": "1"}, "cwd": "."},
                "remote": {"type": "http", "url": "https://example.com/mcp", "headers": {"X-Org": "o"}}
            }}"#,
        );
        assert_eq!(
            config.server("local").unwrap().transport,
            Transport::Stdio(StdioServer {
                command: "srv".into(),
                args: vec!["-v".into()],
                env: BTreeMap::from([("A".into(), Template::parse("1").unwrap())]),
                cwd: Some(".".into()),
            })
        );
        assert_eq!(
            config.server("remote").unwrap().transport,
            Transport::Http(RemoteServer {
                url: ServerUrl::parse("https://example.com/mcp").unwrap(),
                headers: BTreeMap::from([("X-Org".into(), Template::parse("o").unwrap())]),
            })
        );
        assert_eq!(config.warnings(), &[]);
    }

    #[test]
    fn treats_command_without_type_as_stdio() {
        let config = load(r#"{"mcpServers": {"s": {"command": "srv"}}}"#);
        assert!(matches!(
            config.server("s").unwrap().transport,
            Transport::Stdio(_)
        ));
    }

    #[test]
    fn rejects_url_without_type_because_tools_disagree_on_its_meaning() {
        let config = load(r#"{"mcpServers": {"r": {"url": "https://example.com/mcp"}}}"#);
        assert_eq!(
            config.server("r"),
            Err(LookupError::Invalid {
                name: "r".into(),
                error: EntryError::UrlWithoutType
            })
        );
    }

    #[test]
    fn reads_sse_entry() {
        let config =
            load(r#"{"mcpServers": {"r": {"type": "sse", "url": "https://example.com/sse"}}}"#);
        assert!(matches!(
            config.server("r").unwrap().transport,
            Transport::Sse(_)
        ));
    }

    #[test]
    fn invalid_entry_does_not_stop_other_entries() {
        let config = load(
            r#"{"mcpServers": {
                "bad.name": {"command": "x"},
                "no-type": {"url": "https://example.com"},
                "good": {"command": "srv"}
            }}"#,
        );
        assert_eq!(config.servers().count(), 1);
        let invalid: Vec<&str> = config.invalid().map(|(name, _)| name).collect();
        assert_eq!(invalid, ["bad.name", "no-type"]);
    }

    #[test]
    fn warns_about_unknown_keys_at_top_level_and_in_entries() {
        let config = load(
            r#"{"theme": "dark", "mcpServers": {
                "s": {"command": "srv", "disabled": true, "headers": {}}
            }}"#,
        );
        let text: Vec<String> = config.warnings().iter().map(ToString::to_string).collect();
        assert_eq!(
            text,
            [
                "unknown key \"theme\" ignored",
                "server s: unknown key \"disabled\" ignored",
                "server s: unknown key \"headers\" ignored",
            ]
        );
        assert!(config.server("s").is_ok());
    }

    #[test]
    fn expands_command_args_cwd_and_url_but_keeps_env_and_headers_for_later() {
        let config = load(
            r#"{"mcpServers": {
                "s": {"command": "${HOME}/bin/srv", "args": ["--port=${PORT}"],
                      "env": {"${HOME}": "${PORT}"}, "cwd": "${HOME}"},
                "r": {"type": "http", "url": "http://localhost:${PORT}/mcp", "headers": {"X": "${HOME}"}}
            }}"#,
        );
        let Transport::Stdio(stdio) = &config.server("s").unwrap().transport else {
            panic!("not stdio")
        };
        assert_eq!(stdio.command, "/home/u/bin/srv");
        assert_eq!(stdio.args, ["--port=8080"]);
        assert_eq!(
            stdio.env.get("${HOME}").map(Template::as_str),
            Some("${PORT}")
        );
        assert_eq!(stdio.cwd.as_deref(), Some("/home/u"));
        let Transport::Http(remote) = &config.server("r").unwrap().transport else {
            panic!("not http")
        };
        assert_eq!(remote.url.as_str(), "http://localhost:8080/mcp");
        assert_eq!(
            remote.headers.get("X").map(Template::as_str),
            Some("${HOME}")
        );
    }

    #[test]
    fn target_keeps_variables_unexpanded_and_ignores_env_and_headers() {
        let config = load(
            r#"{"mcpServers": {
                "s": {"command": "srv", "args": ["--port=${PORT}"], "env": {"T": "${HOME}"}},
                "r": {"type": "http", "url": "http://localhost:${PORT}/mcp", "headers": {"X": "${HOME}"}}
            }}"#,
        );
        assert_eq!(
            config.server("s").unwrap().target,
            Target::Stdio {
                command: "srv".into(),
                args: vec!["--port=${PORT}".into()],
                cwd: None
            }
        );
        assert_eq!(
            config.server("r").unwrap().target,
            Target::Remote {
                url: "http://localhost:${PORT}/mcp".into()
            }
        );
    }

    #[test]
    fn unset_variable_in_args_is_an_error_for_that_server_only() {
        let config = load(
            r#"{"mcpServers": {"s": {"command": "srv", "args": ["${MISSING}"]}, "t": {"command": "x"}}}"#,
        );
        assert_eq!(
            config.server("s"),
            Err(LookupError::Invalid {
                name: "s".into(),
                error: EntryError::Variable {
                    field: "args[0]".into(),
                    error: ExpandError::Unset("MISSING".into())
                }
            })
        );
        assert!(config.server("t").is_ok());
    }

    #[test]
    fn unset_variable_in_env_or_headers_is_not_an_error_yet() {
        let config = load(
            r#"{"mcpServers": {
                "s": {"command": "srv", "env": {"T": "${MISSING}"}},
                "r": {"type": "http", "url": "https://x/mcp", "headers": {"A": "Bearer ${KEY}"}},
                "bad": {"command": "srv", "env": {"T": "${A-B}"}}
            }}"#,
        );
        assert!(config.server("s").is_ok());
        assert!(config.server("r").is_ok());
        assert!(matches!(
            config.server("bad"),
            Err(LookupError::Invalid {
                error: EntryError::Variable { .. },
                ..
            })
        ));
    }

    #[test]
    fn expands_env_values_and_marks_the_values_from_variables() {
        let config = load(
            r#"{"mcpServers": {"s": {"command": "srv", "env": {"A": "plain", "T": "x-${TOKEN}"}}}}"#,
        );
        let server = config.server("s").unwrap();
        assert_eq!(
            server.value_names().into_iter().collect::<Vec<_>>(),
            ["TOKEN"]
        );
        let values = server
            .expand_values(&Env::from([("TOKEN".to_string(), "s3cret".to_string())]))
            .unwrap();
        assert_eq!(values.env["A"].value, "plain");
        assert!(!values.env["A"].from_variable);
        assert_eq!(values.env["T"].value, "x-s3cret");
        assert!(values.env["T"].from_variable);
        assert!(!format!("{values:?}").contains("s3cret"));
        assert_eq!(
            server.expand_values(&Env::new()),
            Err(EntryError::Variable {
                field: "env.T".into(),
                error: ExpandError::Unset("TOKEN".into())
            })
        );
    }

    #[test]
    fn collects_the_variable_names_of_env_and_headers_values() {
        let config = load(
            r#"{"mcpServers": {
                "s": {"command": "srv", "args": ["${PORT}"], "env": {"${KEY_NAME}": "${env:TOKEN}"}},
                "bad": {"type": "http", "url": "https://x/${HOST}", "headers": {"A": "Bearer ${KEY}"}},
                "other": {"command": "srv", "headers": {"A": "${NOT_READ}"}}
            }}"#,
        );
        let names: Vec<&str> = config.variables().iter().map(String::as_str).collect();
        assert_eq!(names, ["KEY", "TOKEN"]);
    }

    #[test]
    fn reports_wrong_value_types_with_the_field() {
        let config = load(
            r#"{"mcpServers": {"a": {"command": 1}, "b": {"command": "x", "args": [1]}, "c": {"type": 2}}}"#,
        );
        let errors: Vec<String> = config.invalid().map(|(_, e)| e.to_string()).collect();
        assert_eq!(
            errors,
            [
                "\"command\" must be a string",
                "\"args[0]\" must be a string",
                "\"type\" must be a string"
            ]
        );
    }

    #[test]
    fn reports_missing_fields_unknown_type_and_bad_url() {
        let config = load(
            r#"{"mcpServers": {
                "a": {"type": "stdio"},
                "b": {"type": "http"},
                "c": {"type": "ws", "url": "x"},
                "d": {"type": "http", "url": "http://example.com/mcp"},
                "e": {"cwd": "."}
            }}"#,
        );
        let errors: Vec<EntryError> = config.invalid().map(|(_, e)| e.clone()).collect();
        assert_eq!(
            errors,
            [
                EntryError::Missing("command"),
                EntryError::Missing("url"),
                EntryError::UnknownType("ws".into()),
                EntryError::Url(UrlError::PlainHttp),
                EntryError::NoTransport,
            ]
        );
    }

    #[test]
    fn missing_server_map_gives_empty_configuration() {
        let config = load("{}");
        assert_eq!(config.servers().count(), 0);
        assert_eq!(config.server("x"), Err(LookupError::NotFound("x".into())));
    }

    #[test]
    fn rejects_text_that_is_not_the_mcp_servers_shape() {
        assert!(matches!(parse("[]", &env()), Err(ConfigError::NotObject)));
        assert!(matches!(parse("{", &env()), Err(ConfigError::Json(_))));
        assert!(matches!(
            parse(r#"{"mcpServers": []}"#, &env()),
            Err(ConfigError::ServersNotObject)
        ));
    }
}
