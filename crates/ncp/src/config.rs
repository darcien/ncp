//! The configuration model: the common `mcpServers` JSON core.
//!
//! The files are JSONC: JSON with comments and trailing commas, as VS Code
//! and Zed read them. No checked tool reads JSON5, so its forms are errors.
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
/// Comments and trailing commas only. The other options of the parser are
/// JSON5 forms.
const JSONC: jsonc_parser::ParseOptions = jsonc_parser::ParseOptions {
    allow_comments: true,
    allow_trailing_commas: true,
    allow_loose_object_property_names: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};
const STDIO_KEYS: [&str; 5] = ["type", "command", "args", "env", "cwd"];
const REMOTE_KEYS: [&str; 4] = ["type", "url", "headers", "oauth"];
/// The keys of the `oauth` object that the MVP reads (design C4, B68).
/// `authorizationServer` is the key of Claude Desktop for the issuer.
const OAUTH_KEYS: [&str; 4] = ["clientId", "clientSecret", "issuer", "authorizationServer"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: ServerName,
    /// The values after `${NAME}` expansion.
    pub transport: Transport,
    /// The target as the file writes it, before expansion. Output and the
    /// protocol version cache use it, so an expanded value never goes to a file or to
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
    /// `oauth.clientId`: a client that the user registered with the
    /// authorization server. With it, `ncp` does no registration.
    pub oauth_client: Option<PreRegisteredClient>,
    /// The values of the `${NAME}` references in `url`. Output hides each
    /// of them wherever it occurs, for example in a URL that a library
    /// makes from the server URL.
    pub url_values: Vec<String>,
}

/// A client that the user registered with an authorization server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreRegisteredClient {
    pub client_id: String,
    pub kind: ClientKind,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ClientKind {
    /// A client with no secret. A configured issuer binds it.
    Public { issuer: Option<String> },
    /// A client with a secret. The secret goes only to its issuer, so that
    /// a changed or hostile MCP server cannot get it.
    Confidential { secret: Template, issuer: String },
}

impl PreRegisteredClient {
    /// The issuer that the user registered the client with.
    #[must_use]
    pub fn issuer(&self) -> Option<&str> {
        match &self.kind {
            ClientKind::Public { issuer } => issuer.as_deref(),
            ClientKind::Confidential { issuer, .. } => Some(issuer),
        }
    }

    /// The secret as the file writes it: a literal value or `${NAME}`.
    #[must_use]
    pub fn secret(&self) -> Option<&Template> {
        match &self.kind {
            ClientKind::Public { .. } => None,
            ClientKind::Confidential { secret, .. } => Some(secret),
        }
    }
}

/// The secret can be a literal value, so `Debug` hides it.
impl std::fmt::Debug for ClientKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public { issuer } => f.debug_struct("Public").field("issuer", issuer).finish(),
            Self::Confidential { issuer, .. } => f
                .debug_struct("Confidential")
                .field("secret", &"<hidden>")
                .field("issuer", issuer)
                .finish(),
        }
    }
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
    #[error("configuration is not valid JSONC: {0}")]
    Syntax(String),
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
    #[error("\"oauth.{0}\" needs \"oauth.clientId\"")]
    WithoutClientId(&'static str),
    /// The URL is the expanded `url` of the entry, so that `auth login`
    /// can find the issuer that the server names.
    #[error(
        "\"oauth.clientSecret\" needs \"oauth.issuer\": the issuer of the authorization server where you registered the client"
    )]
    SecretWithoutIssuer { url: ServerUrl },
    #[error("\"oauth.issuer\" and \"oauth.authorizationServer\" do not agree")]
    IssuersDisagree,
    #[error("\"oauth.clientSecret\" is not a valid ${{NAME}} reference")]
    BadClientSecret,
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

    /// True when the file has an entry of the name, valid or not.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The view after a trust of a project file: each project entry
    /// replaces the global entry of the same name as a whole, with no
    /// merge of fields, as mise does for a tool.
    #[must_use]
    pub fn with_project(self, project: Self) -> Self {
        Self {
            entries: self.entries.into_iter().chain(project.entries).collect(),
            warnings: self.warnings.into_iter().chain(project.warnings).collect(),
            variables: self
                .variables
                .into_iter()
                .chain(project.variables)
                .collect(),
        }
    }

    /// The names of all `${NAME}` in `env` and `headers` values and in
    /// `oauth.clientSecret`, also in invalid entries. Only these values can
    /// come from a store.
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
    let root = parse_root(text)?;
    let top_warnings = root
        .keys()
        .filter(|key| *key != SERVERS_KEY)
        .map(|key| Warning {
            server: None,
            key: key.clone(),
        });
    let none = Map::new();
    let servers = servers_of(&root)?.unwrap_or(&none);
    let parsed: Vec<(String, Result<Server, EntryError>, Vec<Warning>)> = servers
        .iter()
        .map(|(name, value)| {
            let (entry, warnings) = parse_entry(name, value, Expansion::From(env));
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
    servers
        .values()
        .filter_map(Value::as_object)
        .filter_map(|entry| kind_of(entry).ok().map(|kind| store_values(entry, kind)))
        .flatten()
        .for_each(|values| variables_in(values, &mut variables));
    Ok(Config {
        entries,
        warnings,
        variables,
    })
}

fn parse_root(text: &str) -> Result<Map<String, Value>, ConfigError> {
    match jsonc_parser::parse_to_serde_value(text, &JSONC) {
        Ok(Value::Object(root)) => Ok(root),
        Ok(_) => Err(ConfigError::NotObject),
        Err(error) => Err(ConfigError::Syntax(error.to_string())),
    }
}

fn servers_of(root: &Map<String, Value>) -> Result<Option<&Map<String, Value>>, ConfigError> {
    root.get(SERVERS_KEY)
        .map(|value| value.as_object().ok_or(ConfigError::ServersNotObject))
        .transpose()
}

/// The values that can read a credential store: `env` of a stdio entry,
/// `headers` and `oauth.clientSecret` of a remote entry. Only the field of
/// the transport expands: `headers` in a stdio entry is an unknown key.
fn store_values(entry: &Map<String, Value>, kind: Kind) -> Vec<&Value> {
    let values = match kind {
        Kind::Stdio => vec![entry.get("env")],
        Kind::Http | Kind::Sse => vec![
            entry.get("headers"),
            entry
                .get("oauth")
                .and_then(|oauth| oauth.get("clientSecret")),
        ],
    };
    values.into_iter().flatten().collect()
}

fn names_in<'a>(values: impl IntoIterator<Item = &'a Value>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for value in values {
        variables_in(value, &mut found);
    }
    found
}

/// One entry of a file, as the file writes it, for a person to check
/// before a trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpandedEntry {
    pub name: String,
    /// An error here is an entry that the loader cannot read at all.
    pub shape: Result<Unexpanded, EntryError>,
}

/// An entry with no expansion. The target comes from the code that gives
/// the target of a loaded server, so the entry that shows is the entry
/// that runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unexpanded {
    pub transport: &'static str,
    pub target: Target,
    /// The names of the `env` variables of a stdio entry. Never their
    /// values: a value can be a literal secret.
    pub env: Vec<String>,
    /// Each `${NAME}` of `headers` and `oauth.clientSecret` of a remote
    /// entry, with the host that gets its value.
    pub sends: Vec<(String, String)>,
    /// The names that can come from a store.
    pub variables: BTreeSet<String>,
    /// An error of the checks of the loader that need no expansion. The
    /// loader then refuses the entry, so nothing runs.
    pub problem: Option<EntryError>,
}

/// The entries of the text with no expansion: no value of the environment
/// goes into them.
///
/// # Errors
///
/// Returns an error when the text is not JSONC, or does not have the
/// `mcpServers` shape.
pub fn unexpanded_entries(text: &str) -> Result<Vec<UnexpandedEntry>, ConfigError> {
    Ok(servers_of(&parse_root(text)?)?
        .map(|servers| {
            servers
                .iter()
                .map(|(name, value)| UnexpandedEntry {
                    name: name.clone(),
                    shape: unexpanded(name, value),
                })
                .collect()
        })
        .unwrap_or_default())
}

fn unexpanded(name: &str, value: &Value) -> Result<Unexpanded, EntryError> {
    ServerName::parse(name)?;
    let object = value.as_object().ok_or(EntryError::NotObject)?;
    let kind = kind_of(object)?;
    let target = configured_target(object, kind);
    let problem = transport(object, kind, Expansion::Off).err();
    let (env, sends) = match &target {
        Target::Stdio { .. } => (
            object
                .get("env")
                .and_then(Value::as_object)
                .map(|env| env.keys().cloned().collect())
                .unwrap_or_default(),
            Vec::new(),
        ),
        Target::Remote { url } => (Vec::new(), sends(object, url)),
    };
    Ok(Unexpanded {
        transport: kind.name(),
        variables: names_in(store_values(object, kind)),
        target,
        env,
        sends,
        problem,
    })
}

/// Each `${NAME}` of `headers` with the host of the URL, and of
/// `oauth.clientSecret` with the host of the issuer, which gets it.
fn sends(object: &Map<String, Value>, url: &str) -> Vec<(String, String)> {
    let oauth = object.get("oauth").and_then(Value::as_object);
    let issuer = oauth
        .and_then(|oauth| {
            let issuer = oauth
                .get("issuer")
                .and_then(Value::as_str)
                .map(str::to_string);
            configured_issuer(issuer, oauth.get("authorizationServer")).ok()?
        })
        .map_or_else(|| host_of(url), |issuer| host_of(&issuer));
    let headers = names_in(object.get("headers"))
        .into_iter()
        .map(|name| (name, host_of(url)));
    let secret = names_in(oauth.and_then(|oauth| oauth.get("clientSecret")))
        .into_iter()
        .map(|name| (name, issuer.clone()));
    headers.chain(secret).collect()
}

/// The host that gets a value. With `${` in the authority, the host as
/// the file writes it: a parse would change the case of a variable name.
fn host_of(url: &str) -> String {
    let authority = authority(url);
    if authority.contains("${") {
        return authority
            .rsplit('@')
            .next()
            .unwrap_or(authority)
            .to_string();
    }
    url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| authority.to_string())
}

/// The text between `://` and the first `/`, `?`, `#` or `\`, as a URL
/// parser ends the authority.
fn authority(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#', '\\']).next().unwrap_or(rest)
}

/// The URL that the checks with no expansion use. A `${NAME}` in the path
/// or the query becomes a literal, so the rules of the URL still apply. A
/// `${NAME}` in the scheme or the authority decides the scheme and the
/// host: no rule of the URL applies before the expansion, and a fixed URL
/// stands in, so that the checks of `headers` and `oauth` still run.
fn url_to_check(url: &str) -> String {
    let scheme = url.split_once("://").map_or("", |(scheme, _)| scheme);
    if scheme.contains("${") || authority(url).contains("${") {
        return "https://localhost/".to_string();
    }
    let literals: Env = expand::names(url)
        .into_iter()
        .map(|name| (name.to_string(), "x".to_string()))
        .collect();
    expand(url, &literals).unwrap_or_else(|_| url.to_string())
}

/// How the parse treats `${NAME}` in `command`, `args`, `cwd` and `url`.
#[derive(Clone, Copy)]
enum Expansion<'a> {
    From(&'a Env),
    /// Keep the text, and check only its syntax: the entries of a project
    /// file before the trust.
    Off,
}

impl Expansion<'_> {
    fn apply(self, text: &str) -> Result<String, ExpandError> {
        match self {
            Self::From(env) => expand(text, env),
            Self::Off => Template::parse(text).map(|_| text.to_string()),
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Stdio,
    Http,
    Sse,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

fn parse_entry(
    name: &str,
    value: &Value,
    expansion: Expansion<'_>,
) -> (Result<Server, EntryError>, Vec<Warning>) {
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
            let server = transport(object, kind, expansion).map(|transport| Server {
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
    let oauth_keys = object
        .get("oauth")
        .and_then(Value::as_object)
        .filter(|_| !matches!(kind, Kind::Stdio))
        .into_iter()
        .flat_map(|oauth| oauth.keys())
        .filter(|key| !OAUTH_KEYS.contains(&key.as_str()))
        .map(|key| format!("oauth.{key}"));
    object
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .cloned()
        .chain(oauth_keys)
        .map(|key| Warning {
            server: Some(server.to_string()),
            key,
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

fn transport(
    object: &Map<String, Value>,
    kind: Kind,
    expansion: Expansion<'_>,
) -> Result<Transport, EntryError> {
    match kind {
        Kind::Stdio => stdio(object, expansion).map(Transport::Stdio),
        Kind::Http => remote(object, expansion).map(Transport::Http),
        Kind::Sse => remote(object, expansion).map(Transport::Sse),
    }
}

fn stdio(object: &Map<String, Value>, expansion: Expansion<'_>) -> Result<StdioServer, EntryError> {
    let command = string(object, "command", expansion)?.ok_or(EntryError::Missing("command"))?;
    Ok(StdioServer {
        command,
        args: strings(object, "args", expansion)?,
        env: template_map(object, "env")?,
        cwd: string(object, "cwd", expansion)?,
    })
}

fn remote(
    object: &Map<String, Value>,
    expansion: Expansion<'_>,
) -> Result<RemoteServer, EntryError> {
    let url = string(object, "url", expansion)?.ok_or(EntryError::Missing("url"))?;
    let url = match expansion {
        Expansion::From(_) => ServerUrl::parse(&url)?,
        Expansion::Off => ServerUrl::parse(&url_to_check(&url))?,
    };
    let url_values = object
        .get("url")
        .and_then(Value::as_str)
        .map(expand::names)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|name| match expansion {
            Expansion::From(env) => env.get(name).filter(|value| !value.is_empty()).cloned(),
            Expansion::Off => None,
        })
        .collect();
    Ok(RemoteServer {
        headers: template_map(object, "headers")?,
        oauth_client: oauth_client(object, &url)?,
        url,
        url_values,
    })
}

/// The pre-registered client of the `oauth` object (design B68). A client
/// ID and an issuer are not secrets, so they have no `${NAME}`.
fn oauth_client(
    object: &Map<String, Value>,
    url: &ServerUrl,
) -> Result<Option<PreRegisteredClient>, EntryError> {
    let Some(oauth) = object.get("oauth") else {
        return Ok(None);
    };
    let oauth = oauth
        .as_object()
        .ok_or_else(|| wrong_type("oauth", "an object"))?;
    let text = |key: &str| match oauth.get(key) {
        None => Ok(None),
        Some(Value::String(text)) if !text.is_empty() => Ok(Some(text.clone())),
        Some(_) => Err(wrong_type(
            &format!("oauth.{key}"),
            "a string that is not empty",
        )),
    };
    let client_id = text("clientId")?;
    let secret = text("clientSecret")?;
    let issuer = configured_issuer(text("issuer")?, oauth.get("authorizationServer"))?;
    let Some(client_id) = client_id else {
        return match (&secret, &issuer) {
            (Some(_), _) => Err(EntryError::WithoutClientId("clientSecret")),
            (None, Some(_)) => Err(EntryError::WithoutClientId("issuer")),
            (None, None) => Ok(None),
        };
    };
    let kind = match (secret, issuer) {
        (None, issuer) => ClientKind::Public { issuer },
        (Some(_), None) => return Err(EntryError::SecretWithoutIssuer { url: url.clone() }),
        // The error does not show the value: a literal secret can be in it.
        (Some(secret), Some(issuer)) => ClientKind::Confidential {
            secret: Template::parse(&secret).map_err(|_| EntryError::BadClientSecret)?,
            issuer,
        },
    };
    Ok(Some(PreRegisteredClient { client_id, kind }))
}

/// `oauth.issuer`, else `oauth.authorizationServer`: a string, or an array
/// with one string. One issuer, because a client is registered at one
/// authorization server.
fn configured_issuer(
    issuer: Option<String>,
    authorization_server: Option<&Value>,
) -> Result<Option<String>, EntryError> {
    let other = match authorization_server {
        None => None,
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Array(items)) => match items.as_slice() {
            [Value::String(text)] if !text.is_empty() => Some(text.clone()),
            _ => {
                return Err(wrong_type(
                    "oauth.authorizationServer",
                    "a string or an array with one string",
                ));
            }
        },
        Some(_) => {
            return Err(wrong_type(
                "oauth.authorizationServer",
                "a string or an array with one string",
            ));
        }
    };
    match (issuer, other) {
        (Some(issuer), Some(other)) if issuer != other => Err(EntryError::IssuersDisagree),
        (issuer, other) => Ok(issuer.or(other)),
    }
}

fn string(
    object: &Map<String, Value>,
    field: &str,
    expansion: Expansion<'_>,
) -> Result<Option<String>, EntryError> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::String(text)) => expand_field(field, text, expansion).map(Some),
        Some(_) => Err(wrong_type(field, "a string")),
    }
}

fn strings(
    object: &Map<String, Value>,
    field: &str,
    expansion: Expansion<'_>,
) -> Result<Vec<String>, EntryError> {
    match object.get(field) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let field = format!("{field}[{index}]");
                item.as_str()
                    .ok_or_else(|| wrong_type(&field, "a string"))
                    .and_then(|text| expand_field(&field, text, expansion))
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

fn expand_field(field: &str, text: &str, expansion: Expansion<'_>) -> Result<String, EntryError> {
    expansion.apply(text).map_err(|error| EntryError::Variable {
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
                oauth_client: None,
                url_values: Vec::new(),
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
    fn reads_oauth_client_id_and_warns_about_other_oauth_keys() {
        let config = load(
            r#"{"mcpServers": {
                "r": {"type": "http", "url": "https://x/mcp",
                      "oauth": {"clientId": "c1", "callbackPort": 1}},
                "bad": {"type": "http", "url": "https://x/mcp", "oauth": {"clientId": 1}}
            }}"#,
        );
        let Transport::Http(remote) = &config.server("r").unwrap().transport else {
            panic!("not http")
        };
        assert_eq!(
            remote
                .oauth_client
                .as_ref()
                .map(|client| client.client_id.as_str()),
            Some("c1")
        );
        let text: Vec<String> = config.warnings().iter().map(ToString::to_string).collect();
        assert_eq!(
            text,
            ["server r: unknown key \"oauth.callbackPort\" ignored"]
        );
        assert!(config.server("bad").is_err());
    }

    fn oauth_entry(oauth: &str) -> Result<Option<PreRegisteredClient>, EntryError> {
        let config = load(&format!(
            r#"{{"mcpServers": {{"r": {{"type": "http", "url": "https://x/mcp", "oauth": {oauth}}}}}}}"#
        ));
        config.server("r").map_err(|error| match error {
            LookupError::Invalid { error, .. } => error,
            LookupError::NotFound(_) => panic!("no entry"),
        })?;
        match &config.server("r").unwrap().transport {
            Transport::Http(remote) => Ok(remote.oauth_client.clone()),
            _ => panic!("not http"),
        }
    }

    #[test]
    fn reads_a_client_secret_with_its_issuer() {
        let client = oauth_entry(
            r#"{"clientId": "c1", "clientSecret": "${CLIENT_SECRET}", "issuer": "https://as.test"}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(client.issuer(), Some("https://as.test"));
        assert_eq!(client.secret().unwrap().names(), ["CLIENT_SECRET"]);
        let literal = oauth_entry(
            r#"{"clientId": "c1", "clientSecret": "s3cr3t", "issuer": "https://as.test"}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(literal.secret().unwrap().as_str(), "s3cr3t");
        assert!(!format!("{literal:?}").contains("s3cr3t"));
    }

    #[test]
    fn refuses_a_client_secret_without_a_client_id_or_an_issuer() {
        assert_eq!(
            oauth_entry(r#"{"clientSecret": "s", "issuer": "https://as.test"}"#),
            Err(EntryError::WithoutClientId("clientSecret"))
        );
        assert_eq!(
            oauth_entry(r#"{"issuer": "https://as.test"}"#),
            Err(EntryError::WithoutClientId("issuer"))
        );
        assert!(matches!(
            oauth_entry(r#"{"clientId": "c1", "clientSecret": "s"}"#),
            Err(EntryError::SecretWithoutIssuer { .. })
        ));
    }

    #[test]
    fn reads_the_issuer_from_authorization_server_when_issuer_is_absent() {
        let issuer = |oauth: &str| {
            oauth_entry(oauth).map(|client| client.and_then(|c| c.issuer().map(str::to_string)))
        };
        assert_eq!(
            issuer(r#"{"clientId": "c", "authorizationServer": "https://a"}"#),
            Ok(Some("https://a".into()))
        );
        assert_eq!(
            issuer(r#"{"clientId": "c", "authorizationServer": ["https://a"]}"#),
            Ok(Some("https://a".into()))
        );
        assert_eq!(
            issuer(
                r#"{"clientId": "c", "issuer": "https://a", "authorizationServer": "https://a"}"#
            ),
            Ok(Some("https://a".into()))
        );
        assert_eq!(
            issuer(
                r#"{"clientId": "c", "issuer": "https://a", "authorizationServer": "https://b"}"#
            ),
            Err(EntryError::IssuersDisagree)
        );
        assert!(
            issuer(r#"{"clientId": "c", "authorizationServer": ["https://a", "https://b"]}"#)
                .is_err()
        );
        assert_eq!(issuer(r#"{"clientId": "c"}"#), Ok(None));
    }

    #[test]
    fn a_client_secret_reference_counts_as_a_used_variable() {
        let config = load(
            r#"{"mcpServers": {"r": {"type": "http", "url": "https://x/mcp",
                "oauth": {"clientId": "c", "clientSecret": "${PRE_SECRET}", "issuer": "https://a"}}}}"#,
        );
        assert!(config.variables().contains("PRE_SECRET"));
    }

    #[test]
    fn shows_no_part_of_a_client_secret_that_is_not_a_valid_reference() {
        let error = oauth_entry(
            r#"{"clientId": "c", "clientSecret": "ab${x-y}cd", "issuer": "https://a"}"#,
        )
        .unwrap_err();
        assert_eq!(error, EntryError::BadClientSecret);
        assert!(!error.to_string().contains("x-y"));
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
    fn a_project_entry_replaces_the_global_entry_of_the_same_name() {
        let global =
            load(r#"{"mcpServers": {"gh": {"command": "global"}, "g": {"command": "g"}}}"#);
        let project = load(r#"{"mcpServers": {"gh": {"command": "project"}}}"#);
        let merged = global.with_project(project);
        let command = |name: &str| match &merged.server(name).unwrap().transport {
            Transport::Stdio(stdio) => stdio.command.clone(),
            _ => panic!("not stdio"),
        };
        assert_eq!(command("gh"), "project");
        assert_eq!(command("g"), "g");
    }

    /// For each entry: the shown target is the target that runs, and an
    /// entry that the loader refuses shows a problem or an error.
    #[test]
    fn the_unexpanded_entry_is_the_entry_that_runs() {
        let entries = [
            serde_json::json!({"type": "stdio", "command": "sh", "args": ["-c", "x"], "url": "https://x.test"}),
            serde_json::json!({"command": "sh", "args": [1, "-c", "x"]}),
            serde_json::json!({"command": "sh", "env": {"A": 1}}),
            serde_json::json!({"command": "sh", "headers": {"A": 1}}),
            serde_json::json!({"type": "http", "url": "http://example.com/mcp"}),
            serde_json::json!({"type": "http", "url": "not a url"}),
            serde_json::json!({"type": "http", "url": "https://${HOST}/mcp"}),
            serde_json::json!({"type": "http", "url": "http://evil.example/${P}"}),
            serde_json::json!({"type": "http", "url": "https://u@evil.example/${P}"}),
            serde_json::json!({"type": "http", "url": "https://${HOST}:${P}/mcp", "headers": {"X": 1}}),
            serde_json::json!({"type": "http", "url": "https://${HOST}/mcp", "oauth": {"clientSecret": "${S}"}}),
            serde_json::json!({"type": "http", "url": "https://x.test/mcp",
                "oauth": {"clientId": "c", "issuer": "https://a", "authorizationServer": "https://b"}}),
            serde_json::json!({"url": "https://x.test/mcp"}),
            serde_json::json!({"type": 1, "command": "sh"}),
            serde_json::json!({"command": "${env:SHELL_NAME}"}),
        ];
        for entry in entries {
            let text = serde_json::json!({"mcpServers": {"s": &entry}}).to_string();
            let shown = unexpanded_entries(&text).unwrap().remove(0).shape;
            let env = Env::from([
                ("HOST".to_string(), "x.test".to_string()),
                ("P".to_string(), "443".to_string()),
                ("S".to_string(), "s".to_string()),
                ("SHELL_NAME".to_string(), "sh".to_string()),
            ]);
            let loaded = parse(&text, &env).unwrap();
            match (shown, loaded.server("s")) {
                (Ok(shown), Ok(server)) => {
                    assert_eq!(shown.problem, None, "{entry}");
                    assert_eq!(shown.target, server.target, "{entry}");
                }
                (Ok(shown), Err(_)) => assert!(shown.problem.is_some(), "{entry}"),
                (Err(_), Err(_)) => {}
                (Err(error), Ok(_)) => panic!("{entry}: shown as {error}, but it runs"),
            }
        }
    }

    #[test]
    fn the_host_of_a_value_ends_where_a_url_parser_ends_it() {
        assert_eq!(
            host_of("https://evil.example#@good.example/${P}"),
            "evil.example"
        );
        assert_eq!(
            host_of("https://evil.example?@good.example/${P}"),
            "evil.example"
        );
        assert_eq!(host_of("https://u@${HOST}/x"), "${HOST}");
        assert_eq!(host_of("https://Mcp.Example.com/mcp"), "mcp.example.com");
    }

    #[test]
    fn a_variable_host_shows_as_the_file_writes_it() {
        let shown = unexpanded(
            "web",
            &serde_json::json!({"type": "http", "url": "https://${HOST}/mcp",
                "headers": {"A": "${TOKEN}"}}),
        )
        .unwrap();
        assert_eq!(shown.sends, [("TOKEN".to_string(), "${HOST}".to_string())]);
    }

    #[test]
    fn unexpanded_gives_each_variable_with_the_host_that_gets_it() {
        let shown = unexpanded(
            "web",
            &serde_json::json!({"type": "http", "url": "https://mcp.example.com/mcp",
                "headers": {"Authorization": "Bearer ${TOKEN}"},
                "oauth": {"clientId": "c", "clientSecret": "${SECRET}",
                          "issuer": "https://auth.example.com"}}),
        )
        .unwrap();
        assert_eq!(
            shown.sends,
            [
                ("TOKEN".to_string(), "mcp.example.com".to_string()),
                ("SECRET".to_string(), "auth.example.com".to_string()),
            ]
        );
        assert_eq!(
            unexpanded("s", &serde_json::json!({"cwd": "."})),
            Err(EntryError::NoTransport)
        );
    }

    #[test]
    fn missing_server_map_gives_empty_configuration() {
        let config = load("{}");
        assert_eq!(config.servers().count(), 0);
        assert_eq!(config.server("x"), Err(LookupError::NotFound("x".into())));
    }

    #[test]
    fn reads_comments_and_trailing_commas() {
        let config = load(
            r#"{
                // line comment
                "mcpServers": { /* block */ "s": {"command": "srv", "args": ["a",],}, },
            }"#,
        );
        assert!(config.server("s").is_ok());
    }

    #[test]
    fn refuses_json5_forms() {
        for text in [
            r"{'mcpServers': {}}",
            r"{mcpServers: {}}",
            r#"{"mcpServers": {"s": {"command": "srv", "n": 0x1F}}}"#,
            r#"{"mcpServers": {"s": {"command": "srv", "n": +1}}}"#,
            r#"{"mcpServers": {"a": {"command": "x"} "b": {"command": "y"}}}"#,
        ] {
            assert!(
                matches!(parse(text, &env()), Err(ConfigError::Syntax(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn last_duplicate_key_wins() {
        let config = load(r#"{"mcpServers": {"s": {"command": "one"}, "s": {"command": "two"}}}"#);
        assert_eq!(
            config.server("s").unwrap().target,
            Target::Stdio {
                command: "two".into(),
                args: Vec::new(),
                cwd: None
            }
        );
    }

    #[test]
    fn rejects_text_that_is_not_the_mcp_servers_shape() {
        assert!(matches!(parse("[]", &env()), Err(ConfigError::NotObject)));
        assert!(matches!(parse("{", &env()), Err(ConfigError::Syntax(_))));
        assert!(matches!(
            parse(r#"{"mcpServers": []}"#, &env()),
            Err(ConfigError::ServersNotObject)
        ));
    }
}
