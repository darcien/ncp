//! The entry of `server add`, from its arguments. Pure functions: the
//! rules of docs/design/server-add.md section 4.1 and rule 12, and the
//! checks of the loader with no expansion.
//!
//! An error never holds a value of a header or of `--env`, because the
//! user can give a secret there by mistake.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::{
    config::{self, EntryError, Unexpanded},
    expand,
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
    pub secret: Option<String>,
}

/// The entry in the order that the file gets: `type` first, then the
/// fields of the transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub fields: Vec<(String, Value)>,
    pub shape: Unexpanded,
}

impl Entry {
    /// The entry as one JSON value, to compare with an entry of a file.
    #[must_use]
    pub fn value(&self) -> Value {
        Value::Object(self.fields.iter().cloned().collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddError {
    #[error("--header is only for an http entry")]
    HeaderOnStdio,
    #[error("--env is only for a stdio entry")]
    EnvOnRemote,
    #[error("a --header has no \":\" between the name and the value")]
    HeaderShape,
    #[error("{0:?} is not a valid header name")]
    HeaderName(String),
    #[error("the header {0} is given more than one time")]
    HeaderTwice(String),
    #[error("the value of the header {0} is empty")]
    HeaderEmpty(String),
    #[error("{0:?} after --env is not a variable name")]
    EnvName(String),
    #[error("--env {0} is given more than one time")]
    EnvTwice(String),
    #[error("the value of --env {0} is empty")]
    EnvEmpty(String),
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
    #[error("the entry is not valid: {0}")]
    Entry(#[from] EntryError),
}

/// The header fields that RFC 9110 defines as credentials (§11.6.2,
/// §11.7.2). Field names have no case (RFC 9110 §5.1).
const CREDENTIAL_HEADERS: [&str; 2] = ["authorization", "proxy-authorization"];

/// Builds the entry and checks it with the rules of the loader.
///
/// # Errors
///
/// Returns the first rule that the arguments break.
pub fn build(name: &str, request: &Request) -> Result<Entry, AddError> {
    let fields = match &request.form {
        Form::Remote { url } => {
            if !request.env.is_empty() {
                return Err(AddError::EnvOnRemote);
            }
            let headers = headers(&request.headers)?;
            std::iter::once(("type", json!("http")))
                .chain(std::iter::once(("url", json!(url))))
                .chain((!headers.is_empty()).then_some(("headers", Value::Object(headers))))
                .collect::<Vec<_>>()
        }
        Form::Stdio { command, args } => {
            if !request.headers.is_empty() {
                return Err(AddError::HeaderOnStdio);
            }
            let env = env(&request.env)?;
            std::iter::once(("type", json!("stdio")))
                .chain(std::iter::once(("command", json!(command))))
                .chain((!args.is_empty()).then(|| ("args", json!(args))))
                .chain((!env.is_empty()).then_some(("env", Value::Object(env))))
                .collect()
        }
    };
    let fields: Vec<(String, Value)> = fields
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();
    let value = Value::Object(fields.iter().cloned().collect());
    let shape = config::check_entry(name, &value)?;
    if let Some(secret) = &request.secret {
        if !shape.variables.contains(secret) {
            return Err(AddError::SecretNotUsed);
        }
        if let Form::Remote { url } = &request.form
            && origin_has_variable(url)
        {
            return Err(AddError::SecretOrigin);
        }
    }
    Ok(Entry { fields, shape })
}

fn headers(flags: &[String]) -> Result<Map<String, Value>, AddError> {
    flags.iter().try_fold(Map::new(), |mut headers, flag| {
        let (name, value) = flag.split_once(':').ok_or(AddError::HeaderShape)?;
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty() || !name.chars().all(is_token_char) {
            return Err(AddError::HeaderName(name.to_string()));
        }
        if headers.keys().any(|known| known.eq_ignore_ascii_case(name)) {
            return Err(AddError::HeaderTwice(name.to_string()));
        }
        if value.is_empty() {
            return Err(AddError::HeaderEmpty(name.to_string()));
        }
        let credential = CREDENTIAL_HEADERS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(name));
        if credential && !is_credential_template(value) {
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
        Some((scheme, reference)) => {
            if scheme.is_empty() || !scheme.chars().all(is_token_char) {
                return false;
            }
            reference
        }
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
    flags.iter().try_fold(Map::new(), |mut env, flag| {
        let (name, value) = match flag.split_once('=') {
            Some((name, value)) => (name, value.to_string()),
            None => (flag.as_str(), format!("${{{flag}}}")),
        };
        if !expand::is_variable_name(name) {
            return Err(AddError::EnvName(name.to_string()));
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

/// True when `${` is in the scheme or the authority of the URL: the
/// origin is known only after the expansion.
fn origin_has_variable(url: &str) -> bool {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let authority = rest.split(['/', '?', '#', '\\']).next().unwrap_or(rest);
    scheme.contains("${") || authority.contains("${")
}

/// The names of `${NAME}` in `command`, `args`, `cwd` and `url`: only the
/// environment gives their values (design section 2.4).
#[must_use]
pub fn environment_names(shape: &Unexpanded) -> BTreeSet<String> {
    let texts: Vec<&str> = match &shape.target {
        config::Target::Stdio { command, args, cwd } => std::iter::once(command.as_str())
            .chain(args.iter().map(String::as_str))
            .chain(cwd.as_deref())
            .collect(),
        config::Target::Remote { url } => vec![url.as_str()],
    };
    texts
        .into_iter()
        .flat_map(expand::names)
        .map(str::to_string)
        .collect()
}

/// True when the entry sends a credential header: the server then needs
/// no OAuth login.
#[must_use]
pub fn has_credential_header(entry: &Entry) -> bool {
    entry
        .fields
        .iter()
        .filter(|(key, _)| key == "headers")
        .filter_map(|(_, headers)| headers.as_object())
        .flat_map(Map::keys)
        .any(|name| {
            CREDENTIAL_HEADERS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(name))
        })
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
            assert!(build("web", &remote(&[good])).is_ok(), "{good}");
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
    fn refuses_empty_values_and_duplicate_names() {
        assert_eq!(
            build("web", &remote(&["X-A:   "])),
            Err(AddError::HeaderEmpty("X-A".into()))
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
        assert_eq!(
            build("web", &remote(&["no colon"])),
            Err(AddError::HeaderShape)
        );
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
        request.secret = Some("OTHER".into());
        assert_eq!(build("web", &request), Err(AddError::SecretNotUsed));
        request.secret = Some("KEY".into());
        assert!(build("web", &request).is_ok());
        request.form = Form::Remote {
            url: "https://${HOST}/mcp".into(),
        };
        assert_eq!(build("web", &request), Err(AddError::SecretOrigin));
    }

    #[test]
    fn the_entry_gets_the_checks_of_the_loader() {
        let request = Request {
            form: Form::Remote {
                url: "http://example.com/mcp".into(),
            },
            headers: Vec::new(),
            env: Vec::new(),
            secret: None,
        };
        assert!(matches!(build("web", &request), Err(AddError::Entry(_))));
        assert!(matches!(
            build("bad.name", &remote(&[])),
            Err(AddError::Entry(_))
        ));
    }

    #[test]
    fn names_of_the_environment_come_from_the_command_and_the_url() {
        let request = Request {
            form: Form::Stdio {
                command: "${HOME}/bin/srv".into(),
                args: vec!["--port=${PORT}".into()],
            },
            headers: Vec::new(),
            env: vec!["TOKEN".into()],
            secret: None,
        };
        let entry = build("s", &request).unwrap();
        let names: Vec<String> = environment_names(&entry.shape).into_iter().collect();
        assert_eq!(names, ["HOME", "PORT"]);
    }
}
