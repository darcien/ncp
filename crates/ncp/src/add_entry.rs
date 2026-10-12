//! The entry of `server add`, from its arguments. Pure functions: the
//! rules of docs/design/server-add.md section 4.1 and rule 12, and the
//! checks of the loader with no expansion.
//!
//! An error never holds the text of a `--header` or an `--env` flag that
//! is not a valid name, because the user can give a secret there by
//! mistake. It names the position of the flag.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::{
    binding,
    config::{self, EntryError, Server, Target, Unexpanded},
    credential::SecretName,
    expand, redact,
};

/// The two forms of `server add`: one word after the name is a URL, and
/// the words after `--` are a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    Remote { url: String },
    Stdio { command: String, args: Vec<String> },
}

/// The arguments of `server add`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub form: Form,
    /// Each `--header 'NAME: VALUE'`.
    pub headers: Vec<String>,
    /// Each `--env NAME` or `--env NAME=VALUE`.
    pub env: Vec<String>,
    pub secret: Option<SecretName>,
}

/// The entry: its fields in the order that the file gets (`type` first),
/// the entry as the loader reads it, and its server with no expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub fields: Vec<(String, Value)>,
    pub shape: Unexpanded,
    pub server: Server,
}

impl Entry {
    /// The entry as one JSON value, to compare with an entry of a file.
    #[must_use]
    pub fn value(&self) -> Value {
        Value::Object(self.fields.iter().cloned().collect())
    }

    /// True when the entry sends a credential header: the server then
    /// needs no OAuth login.
    #[must_use]
    pub fn has_credential_header(&self) -> bool {
        match &self.server.transport {
            config::Transport::Http(remote) | config::Transport::Sse(remote) => remote
                .headers
                .keys()
                .any(|name| redact::is_credential_header(name)),
            config::Transport::Stdio(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddError {
    #[error("--header is only for an http entry")]
    HeaderOnStdio,
    #[error("--env is only for a stdio entry")]
    EnvOnRemote,
    #[error("--header {0} has no valid field name before \":\"")]
    HeaderName(usize),
    #[error("the header {0} is given more than one time")]
    HeaderTwice(String),
    #[error("the value of the header {0} is empty")]
    HeaderEmpty(String),
    #[error("the value of the header {0} has a control character, which RFC 9110 does not permit")]
    HeaderControl(String),
    #[error("--env {0} has no valid variable name")]
    EnvName(usize),
    #[error("--env {0} is given more than one time")]
    EnvTwice(String),
    #[error("the value of --env {0} is empty")]
    EnvEmpty(String),
    #[error("the command after -- is empty")]
    EmptyCommand,
    #[error(
        "the value of the {0} header must be ${{NAME}} or <scheme> ${{NAME}}, so that it comes from a store"
    )]
    CredentialHeader(String),
    #[error("the name after --secret is not a ${{NAME}} of this entry")]
    SecretNotUsed,
    #[error(
        "--secret needs a URL with no ${{NAME}} in its scheme, host or port: ncp sends a stored value only to a literal origin"
    )]
    SecretOrigin,
    /// The field only: the text of a reference that is not valid can be a
    /// value that the user typed by mistake.
    #[error("the value of {0} has a ${{...}} that is not a valid reference")]
    Reference(String),
    #[error("the entry is not valid: {0}")]
    Entry(#[from] EntryError),
}

/// Builds the entry and checks it with the rules of the loader.
///
/// # Errors
///
/// Returns the first rule that the arguments break.
pub fn build(name: &str, request: &Request) -> Result<Entry, AddError> {
    let fields = match &request.form {
        Form::Remote { url } => remote_fields(url, request)?,
        Form::Stdio { command, args } => stdio_fields(command, args, request)?,
    };
    let value = Value::Object(fields.iter().cloned().collect());
    let shape = config::check_entry(name, &value).map_err(|error| match error {
        EntryError::Variable { field, .. } => AddError::Reference(field),
        other => AddError::Entry(other),
    })?;
    let server = config::unexpanded_server(name, &value)?;
    if let Some(secret) = &request.secret {
        if !shape.variables.contains(secret.as_str()) {
            return Err(AddError::SecretNotUsed);
        }
        if binding::value_destination(&server).is_err() {
            return Err(AddError::SecretOrigin);
        }
    }
    Ok(Entry {
        fields,
        shape,
        server,
    })
}

fn remote_fields(url: &str, request: &Request) -> Result<Vec<(String, Value)>, AddError> {
    if !request.env.is_empty() {
        return Err(AddError::EnvOnRemote);
    }
    let headers = headers(&request.headers)?;
    let mut fields = vec![field("type", json!("http")), field("url", json!(url))];
    if !headers.is_empty() {
        fields.push(field("headers", Value::Object(headers)));
    }
    Ok(fields)
}

fn stdio_fields(
    command: &str,
    args: &[String],
    request: &Request,
) -> Result<Vec<(String, Value)>, AddError> {
    if !request.headers.is_empty() {
        return Err(AddError::HeaderOnStdio);
    }
    if command.trim().is_empty() {
        return Err(AddError::EmptyCommand);
    }
    let env = env(&request.env)?;
    let mut fields = vec![
        field("type", json!("stdio")),
        field("command", json!(command)),
    ];
    if !args.is_empty() {
        fields.push(field("args", json!(args)));
    }
    if !env.is_empty() {
        fields.push(field("env", Value::Object(env)));
    }
    Ok(fields)
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn headers(flags: &[String]) -> Result<Map<String, Value>, AddError> {
    flags
        .iter()
        .enumerate()
        .try_fold(Map::new(), |mut headers, (index, flag)| {
            let (name, value) = flag
                .split_once(':')
                .map(|(name, value)| (name.trim(), value.trim()))
                .filter(|(name, _)| !name.is_empty() && name.chars().all(is_token_char))
                .ok_or(AddError::HeaderName(index + 1))?;
            if headers.keys().any(|known| known.eq_ignore_ascii_case(name)) {
                return Err(AddError::HeaderTwice(name.to_string()));
            }
            if value.is_empty() {
                return Err(AddError::HeaderEmpty(name.to_string()));
            }
            // RFC 9110 §5.5 permits a tab in a field value.
            if value.chars().any(|c| c.is_control() && c != '\t') {
                return Err(AddError::HeaderControl(name.to_string()));
            }
            if redact::is_credential_header(name) && !is_credential_template(value) {
                return Err(AddError::CredentialHeader(name.to_string()));
            }
            headers.insert(name.to_string(), json!(value));
            Ok(headers)
        })
}

/// The characters of a field name: an RFC 9110 §5.6.2 token.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// `${NAME}` or `<scheme> ${NAME}`, and nothing more.
fn is_credential_template(value: &str) -> bool {
    let reference = match value.split_once(' ') {
        Some((scheme, _)) if scheme.is_empty() || !scheme.chars().all(is_token_char) => {
            return false;
        }
        Some((_, reference)) => reference,
        None => value,
    };
    reference
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(expand::is_variable_name)
}

/// `--env NAME` writes `"NAME": "${NAME}"`: the short form is the safe
/// form, as `docker run -e NAME`. `--env NAME=VALUE` writes the value as
/// the user gives it.
fn env(flags: &[String]) -> Result<Map<String, Value>, AddError> {
    flags
        .iter()
        .enumerate()
        .try_fold(Map::new(), |mut env, (index, flag)| {
            let (name, value) = match flag.split_once('=') {
                Some((name, value)) => (name, value.to_string()),
                None => (flag.as_str(), format!("${{{flag}}}")),
            };
            if !expand::is_variable_name(name) {
                return Err(AddError::EnvName(index + 1));
            }
            if env.contains_key(name) {
                return Err(AddError::EnvTwice(name.to_string()));
            }
            if value.trim().is_empty() {
                return Err(AddError::EnvEmpty(name.to_string()));
            }
            env.insert(name.to_string(), json!(value));
            Ok(env)
        })
}

/// The names of `${NAME}` in `command`, `args`, `cwd` and `url`: only the
/// environment gives their values (design section 2.4).
#[must_use]
pub fn environment_names(shape: &Unexpanded) -> BTreeSet<String> {
    let texts: Vec<&str> = match &shape.target {
        Target::Stdio { command, args, cwd } => std::iter::once(command.as_str())
            .chain(args.iter().map(String::as_str))
            .chain(cwd.as_deref())
            .collect(),
        Target::Remote { url } => vec![url.as_str()],
    };
    texts
        .into_iter()
        .flat_map(expand::names)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(headers: &[&str]) -> Request {
        Request {
            form: Form::Remote {
                url: "https://mcp.example.com/mcp".into(),
            },
            headers: headers.iter().map(ToString::to_string).collect(),
            env: Vec::new(),
            secret: None,
        }
    }

    fn stdio(env: &[&str]) -> Request {
        Request {
            form: Form::Stdio {
                command: "srv".into(),
                args: vec!["stdio".into()],
            },
            headers: Vec::new(),
            env: env.iter().map(ToString::to_string).collect(),
            secret: None,
        }
    }

    fn secret(name: &str) -> SecretName {
        SecretName::parse(name).unwrap()
    }

    #[test]
    fn builds_an_http_entry_with_type_first() {
        let entry = build("web", &remote(&["X-Tenant: acme"])).unwrap();
        let keys: Vec<&str> = entry.fields.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["type", "url", "headers"]);
        assert_eq!(
            entry.value(),
            json!({"type": "http", "url": "https://mcp.example.com/mcp",
                   "headers": {"X-Tenant": "acme"}})
        );
        assert!(!entry.has_credential_header());
    }

    #[test]
    fn env_name_writes_a_reference_and_name_value_writes_the_value() {
        let entry = build("gh", &stdio(&["TOKEN", "MODE=fast"])).unwrap();
        assert_eq!(
            entry.value(),
            json!({"type": "stdio", "command": "srv", "args": ["stdio"],
                   "env": {"TOKEN": "${TOKEN}", "MODE": "fast"}})
        );
    }

    #[test]
    fn a_credential_header_must_be_an_exact_template() {
        for good in ["Authorization: Bearer ${KEY}", "authorization: ${KEY}"] {
            let entry = build("web", &remote(&[good])).unwrap();
            assert!(entry.has_credential_header(), "{good}");
        }
        for bad in [
            "Authorization: Bearer abc123",
            "Authorization: Bearer ${KEY} extra",
            "Proxy-Authorization: Basic x${KEY}",
        ] {
            let error = build("web", &remote(&[bad])).unwrap_err();
            assert!(matches!(error, AddError::CredentialHeader(_)), "{bad}");
            assert!(!error.to_string().contains("abc123"));
        }
    }

    #[test]
    fn an_error_names_the_flag_and_not_its_text() {
        let error = build("web", &remote(&["Authorization Bearer user:pa55"])).unwrap_err();
        assert_eq!(error, AddError::HeaderName(1));
        assert!(!error.to_string().contains("user"));
        let error = build("gh", &stdio(&["OK", "sk-live-abc123"])).unwrap_err();
        assert_eq!(error, AddError::EnvName(2));
        assert!(!error.to_string().contains("sk-live"));
    }

    #[test]
    fn a_reference_that_is_not_valid_names_only_the_field() {
        let error = build("web", &remote(&["X-Key: ${sk-live-abc}"])).unwrap_err();
        assert_eq!(error, AddError::Reference("headers.X-Key".into()));
        assert!(!error.to_string().contains("sk-live"));
    }

    #[test]
    fn refuses_empty_values_control_characters_and_duplicate_names() {
        assert_eq!(
            build("web", &remote(&["X-A:   "])),
            Err(AddError::HeaderEmpty("X-A".into()))
        );
        assert_eq!(
            build("web", &remote(&["X-A: a\r\nEvil: b"])),
            Err(AddError::HeaderControl("X-A".into()))
        );
        assert_eq!(
            build("web", &remote(&["X-A: 1", "x-a: 2"])),
            Err(AddError::HeaderTwice("x-a".into()))
        );
        assert_eq!(
            build("gh", &stdio(&["K="])),
            Err(AddError::EnvEmpty("K".into()))
        );
        assert_eq!(
            build("gh", &stdio(&["K", "K=v"])),
            Err(AddError::EnvTwice("K".into()))
        );
        let mut empty = stdio(&[]);
        empty.form = Form::Stdio {
            command: " ".into(),
            args: Vec::new(),
        };
        assert_eq!(build("gh", &empty), Err(AddError::EmptyCommand));
    }

    #[test]
    fn flags_must_match_the_form() {
        let mut request = remote(&[]);
        request.env = vec!["K".into()];
        assert_eq!(build("web", &request), Err(AddError::EnvOnRemote));
        let mut request = stdio(&[]);
        request.headers = vec!["X: 1".into()];
        assert_eq!(build("gh", &request), Err(AddError::HeaderOnStdio));
    }

    #[test]
    fn the_secret_must_be_a_stored_value_of_the_entry_with_a_literal_origin() {
        let mut request = remote(&["Authorization: Bearer ${KEY}"]);
        request.secret = Some(secret("OTHER"));
        assert_eq!(build("web", &request), Err(AddError::SecretNotUsed));
        request.secret = Some(secret("KEY"));
        assert!(build("web", &request).is_ok());
        request.form = Form::Remote {
            url: "https://${HOST}/mcp".into(),
        };
        assert_eq!(build("web", &request), Err(AddError::SecretOrigin));
    }

    #[test]
    fn the_entry_gets_the_checks_of_the_loader() {
        let mut request = remote(&[]);
        request.form = Form::Remote {
            url: "http://example.com/mcp".into(),
        };
        assert!(matches!(build("web", &request), Err(AddError::Entry(_))));
        assert!(matches!(
            build("bad.name", &remote(&[])),
            Err(AddError::Entry(_))
        ));
    }

    #[test]
    fn names_of_the_environment_come_from_the_command_and_the_url() {
        let mut request = stdio(&["TOKEN"]);
        request.form = Form::Stdio {
            command: "${HOME}/bin/srv".into(),
            args: vec!["--port=${PORT}".into()],
        };
        let entry = build("s", &request).unwrap();
        let names: Vec<String> = environment_names(&entry.shape).into_iter().collect();
        assert_eq!(names, ["HOME", "PORT"]);
    }
}
