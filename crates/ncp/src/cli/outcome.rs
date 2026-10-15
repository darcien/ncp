//! Exit codes, failures, warnings and hints. Each class of result has one
//! exit code and one `code` name in the JSON error object. The parts of a
//! diagnostic and their rules are in `docs/design/output-messages.md`.

use std::process::ExitCode;

pub use crate::text::{chain, chain_text};
use crate::{
    credential::Destination,
    elicit::Page,
    text::{kept_causes, source_texts},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success,
    /// `tool call` got a result with `isError: true`.
    Tool,
    Usage,
    /// The server, the connection, the protocol, or a local file failed.
    Protocol,
    Auth,
    InputRequired,
    Interrupted,
}

impl Exit {
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Tool => 1,
            Self::Usage => 2,
            Self::Protocol => 3,
            Self::Auth => 4,
            Self::InputRequired => 5,
            // 128 + SIGINT, as shells use.
            Self::Interrupted => 130,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Tool => "tool",
            Self::Usage => "usage",
            Self::Protocol => "protocol",
            Self::Auth => "auth",
            Self::InputRequired => "input_required",
            Self::Interrupted => "interrupted",
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit.code())
    }
}

/// One fix or one piece of advice. `command` is present only for an `ncp`
/// command that reads state or starts a login: an agent can run it with no
/// review (design rule 10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    message: String,
    command: Option<String>,
}

impl Hint {
    /// "To GOAL, run `COMMAND`", with the command as data.
    #[must_use]
    pub fn run(goal: &str, command: impl Into<String>) -> Self {
        let command = command.into();
        Self {
            message: format!("To {goal}, run `{command}`"),
            command: Some(command),
        }
    }

    /// The sentence of `run` for a command that changes state. A person
    /// must decide to run it, so it is not data.
    #[must_use]
    pub fn run_text(goal: &str, command: &str) -> Self {
        Self::text(format!("To {goal}, run `{command}`"))
    }

    #[must_use]
    pub fn text(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            command: None,
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    fn map_text(self, f: &impl Fn(String) -> String) -> Self {
        Self {
            message: f(self.message),
            command: self.command.map(f),
        }
    }
}

impl From<String> for Hint {
    fn from(message: String) -> Self {
        Self::text(message)
    }
}

impl From<&str> for Hint {
    fn from(message: &str) -> Self {
        Self::text(message)
    }
}

/// The server of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
enum About {
    /// The message names the server.
    Named(String),
    /// The text puts `server NAME:` before a message of the older form.
    /// It goes when each message names its server.
    Prefixed(String),
}

/// The parts that an error and a warning share: the message, the data
/// lines, the causes and the hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostic {
    server: Option<About>,
    message: String,
    data: Box<[String]>,
    causes: Box<[String]>,
    hints: Box<[Hint]>,
}

impl Diagnostic {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.server.as_ref().map(|about| match about {
            About::Named(name) | About::Prefixed(name) => name.as_str(),
        })
    }

    /// The server that the text names before a message of the older form.
    #[must_use]
    pub fn prefix(&self) -> Option<&str> {
        match &self.server {
            Some(About::Prefixed(name)) => Some(name),
            _ => None,
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn data(&self) -> &[String] {
        &self.data
    }

    #[must_use]
    pub fn causes(&self) -> &[String] {
        &self.causes
    }

    #[must_use]
    pub fn hints(&self) -> &[Hint] {
        &self.hints
    }

    fn prefixed(self, server: String) -> Self {
        match self.server {
            Some(About::Named(_)) => self,
            _ => Self {
                server: Some(About::Prefixed(server)),
                ..self
            },
        }
    }

    fn data_line(self, line: String) -> Self {
        Self {
            data: push(self.data, line),
            ..self
        }
    }

    fn cause(self, text: String) -> Self {
        let causes = kept_causes(&self.message, self.causes.into_vec(), [text]);
        Self { causes, ..self }
    }

    fn caused_by(self, error: &dyn std::error::Error) -> Self {
        let causes = kept_causes(&self.message, self.causes.into_vec(), source_texts(error));
        Self { causes, ..self }
    }

    fn hint(self, hint: Hint) -> Self {
        Self {
            hints: push(self.hints, hint),
            ..self
        }
    }

    fn context(self, message: String) -> Self {
        let old = std::iter::once(self.message).chain(self.causes.into_vec());
        Self {
            causes: kept_causes(&message, Vec::new(), old),
            message,
            ..self
        }
    }

    fn map_text(self, f: &impl Fn(String) -> String) -> Self {
        let each = |texts: Box<[String]>| texts.into_vec().into_iter().map(f).collect();
        Self {
            message: f(self.message),
            data: each(self.data),
            causes: each(self.causes),
            hints: self
                .hints
                .into_vec()
                .into_iter()
                .map(|hint| hint.map_text(f))
                .collect(),
            ..self
        }
    }
}

fn push<T>(items: Box<[T]>, item: T) -> Box<[T]> {
    let mut items = items.into_vec();
    items.push(item);
    items.into_boxed_slice()
}

/// A warning: the parts of an error with no exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(Diagnostic);

impl Warning {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(Diagnostic::new(message))
    }

    #[must_use]
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }

    /// Sets the server that the message names.
    #[must_use]
    pub fn about(self, server: impl Into<String>) -> Self {
        Self(Diagnostic {
            server: Some(About::Named(server.into())),
            ..self.0
        })
    }

    /// Sets the server of a message of the older form. A message that
    /// names its server keeps it, so that the text does not name the
    /// server two times.
    #[must_use]
    pub fn server(self, server: impl Into<String>) -> Self {
        Self(self.0.prefixed(server.into()))
    }

    #[must_use]
    pub fn data(self, line: impl Into<String>) -> Self {
        Self(self.0.data_line(line.into()))
    }

    #[must_use]
    pub fn cause(self, text: impl Into<String>) -> Self {
        Self(self.0.cause(text.into()))
    }

    #[must_use]
    pub fn caused_by(self, error: &dyn std::error::Error) -> Self {
        Self(self.0.caused_by(error))
    }

    #[must_use]
    pub fn hint(self, hint: impl Into<Hint>) -> Self {
        Self(self.0.hint(hint.into()))
    }
}

/// A failure shows as a warning when the command can go on without the
/// work that failed. All parts stay, so no reason gets lost.
impl From<Failure> for Warning {
    fn from(failure: Failure) -> Self {
        Self(failure.diagnostic)
    }
}

/// A result that is not a success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    exit: Exit,
    diagnostic: Diagnostic,
    // Boxed, because few failures have these parts. Thus each `Result`
    // with a failure stays small.
    extra: Option<Box<Extra>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Extra {
    /// The challenge of a `401` on a connection with no token: a stored
    /// login can answer it.
    login_challenge: Option<String>,
    /// The pages that the server asks the user to open before it can
    /// answer. A person at a TTY can open them. An agent gets them in the
    /// JSON error.
    pages: Box<[Page]>,
    /// The destination of a stored value that no place has.
    destination: Option<Destination>,
}

impl Failure {
    #[must_use]
    pub fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            exit,
            diagnostic: Diagnostic::new(message),
            extra: None,
        }
    }

    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(Exit::Usage, message)
    }

    #[must_use]
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(Exit::Protocol, message)
    }

    #[must_use]
    pub fn exit(&self) -> Exit {
        self.exit
    }

    #[must_use]
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.diagnostic.server_name()
    }

    #[must_use]
    pub fn message(&self) -> &str {
        self.diagnostic.message()
    }

    #[must_use]
    pub fn hints(&self) -> &[Hint] {
        self.diagnostic.hints()
    }

    #[must_use]
    pub fn pages(&self) -> &[Page] {
        self.extra.as_ref().map_or(&[], |extra| &extra.pages)
    }

    #[must_use]
    pub fn login_challenge(&self) -> Option<&str> {
        self.extra.as_ref()?.login_challenge.as_deref()
    }

    #[must_use]
    pub fn destination(&self) -> Option<&Destination> {
        self.extra.as_ref()?.destination.as_ref()
    }

    /// Sets the server that the message names.
    #[must_use]
    pub fn about(self, server: impl Into<String>) -> Self {
        self.with_diagnostic(|diagnostic| Diagnostic {
            server: Some(About::Named(server.into())),
            ..diagnostic
        })
    }

    /// Sets the server of a message of the older form. A message that
    /// names its server keeps it, so that the text does not name the
    /// server two times.
    #[must_use]
    pub fn server(self, server: impl Into<String>) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.prefixed(server.into()))
    }

    #[must_use]
    pub fn data(self, line: impl Into<String>) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.data_line(line.into()))
    }

    #[must_use]
    pub fn cause(self, text: impl Into<String>) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.cause(text.into()))
    }

    /// Adds the error and each error of its source chain as causes.
    #[must_use]
    pub fn caused_by(self, error: &dyn std::error::Error) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.caused_by(error))
    }

    /// Adds a hint after the hints that the failure has.
    #[must_use]
    pub fn hint(self, hint: impl Into<Hint>) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.hint(hint.into()))
    }

    /// Replaces all hints. To put a hint first, read `hints()` and give the
    /// new list.
    #[must_use]
    pub fn with_hints(self, hints: impl IntoIterator<Item = Hint>) -> Self {
        self.with_diagnostic(|diagnostic| Diagnostic {
            hints: hints.into_iter().collect(),
            ..diagnostic
        })
    }

    /// Gives the failure a new message. The old message becomes the first
    /// cause, and all other parts stay.
    #[must_use]
    pub fn context(self, message: impl Into<String>) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.context(message.into()))
    }

    /// Applies `f` to each text of the failure. Each redaction uses it, so
    /// that no text can go out with no redaction.
    #[must_use]
    pub fn map_text(self, f: impl Fn(String) -> String) -> Self {
        self.with_diagnostic(|diagnostic| diagnostic.map_text(&f))
    }

    #[must_use]
    pub fn with_exit(self, exit: Exit) -> Self {
        Self { exit, ..self }
    }

    #[must_use]
    pub fn with_pages(self, pages: impl Into<Box<[Page]>>) -> Self {
        self.with_extra(|extra| Extra {
            pages: pages.into(),
            ..extra
        })
    }

    #[must_use]
    pub fn with_login_challenge(self, challenge: Option<String>) -> Self {
        self.with_extra(|extra| Extra {
            login_challenge: challenge,
            ..extra
        })
    }

    #[must_use]
    pub fn with_destination(self, destination: Destination) -> Self {
        self.with_extra(|extra| Extra {
            destination: Some(destination),
            ..extra
        })
    }

    fn with_diagnostic(self, f: impl FnOnce(Diagnostic) -> Diagnostic) -> Self {
        Self {
            diagnostic: f(self.diagnostic),
            ..self
        }
    }

    fn with_extra(self, f: impl FnOnce(Extra) -> Extra) -> Self {
        let extra = f(self.extra.map(|extra| *extra).unwrap_or_default());
        Self {
            extra: (extra != Extra::default()).then(|| Box::new(extra)),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("outer: {inner}")]
    struct Outer {
        #[source]
        inner: Inner,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("{0}")]
    struct Inner(&'static str);

    #[test]
    fn each_exit_class_has_its_documented_code() {
        let codes: Vec<(u8, &str)> = [
            Exit::Success,
            Exit::Tool,
            Exit::Usage,
            Exit::Protocol,
            Exit::Auth,
            Exit::InputRequired,
            Exit::Interrupted,
        ]
        .iter()
        .map(|exit| (exit.code(), exit.name()))
        .collect();
        assert_eq!(
            codes,
            [
                (0, "success"),
                (1, "tool"),
                (2, "usage"),
                (3, "protocol"),
                (4, "auth"),
                (5, "input_required"),
                (130, "interrupted"),
            ]
        );
    }

    #[test]
    fn a_failure_stays_below_the_size_limit_of_clippy() {
        assert!(std::mem::size_of::<Failure>() <= 128);
    }

    #[test]
    fn a_cause_that_the_last_text_holds_does_not_show() {
        let error = Outer {
            inner: Inner("refused"),
        };
        let failure = Failure::protocol("The handshake failed").caused_by(&error);
        assert_eq!(failure.diagnostic().causes(), ["outer: refused"]);
        let failure = Failure::protocol("The handshake failed")
            .cause("timeout")
            .cause("timeout")
            .cause("a timeout of 5 s");
        assert_eq!(
            failure.diagnostic().causes(),
            ["timeout", "a timeout of 5 s"]
        );
        let failure = Failure::protocol("x refused").cause("refused");
        assert_eq!(failure.diagnostic().causes(), Vec::<String>::new());
    }

    #[test]
    fn chain_text_joins_the_chain_with_no_repeated_text() {
        let error = Outer {
            inner: Inner("refused"),
        };
        assert_eq!(chain_text(&error), "outer: refused");
        let other = Outer {
            inner: Inner("closed"),
        };
        assert_eq!(chain_text(&Inner("x")), "x");
        assert_eq!(chain_text(&other), "outer: closed");
    }

    #[test]
    fn context_makes_the_old_message_the_first_cause() {
        let failure = Failure::usage("The store is locked")
            .cause("security exited with code 51")
            .hint("x")
            .about("web")
            .context("ncp cannot read the tokens of the server `web`");
        assert_eq!(
            failure.message(),
            "ncp cannot read the tokens of the server `web`"
        );
        assert_eq!(
            failure.diagnostic().causes(),
            ["The store is locked", "security exited with code 51"]
        );
        assert_eq!(failure.hints().len(), 1);
        assert_eq!(failure.server_name(), Some("web"));
        assert_eq!(failure.exit(), Exit::Usage);
    }

    #[test]
    fn map_text_reaches_each_text() {
        let failure = Failure::protocol("a k")
            .data("k")
            .cause("k 2")
            .hint(Hint::run("see", "ncp k"))
            .map_text(|text| text.replace('k', "*"));
        let diagnostic = failure.diagnostic();
        assert_eq!(diagnostic.message(), "a *");
        assert_eq!(diagnostic.data(), ["*"]);
        assert_eq!(diagnostic.causes(), ["* 2"]);
        assert_eq!(diagnostic.hints()[0].message(), "To see, run `ncp *`");
        assert_eq!(diagnostic.hints()[0].command(), Some("ncp *"));
    }

    #[test]
    fn only_a_hint_that_reads_state_has_a_command() {
        let read = Hint::run("see the servers", "ncp server list");
        assert_eq!(read.message(), "To see the servers, run `ncp server list`");
        assert_eq!(read.command(), Some("ncp server list"));
        let change = Hint::run_text("trust the file", "ncp trust");
        assert_eq!(change.message(), "To trust the file, run `ncp trust`");
        assert_eq!(change.command(), None);
    }

    #[test]
    fn a_warning_from_a_failure_keeps_each_part() {
        let warning = Warning::from(
            Failure::protocol("ncp cannot write `/x`")
                .about("fx")
                .cause("denied")
                .hint("y"),
        );
        let diagnostic = warning.diagnostic();
        assert_eq!(diagnostic.server_name(), Some("fx"));
        assert_eq!(diagnostic.prefix(), None);
        assert_eq!(diagnostic.causes(), ["denied"]);
        assert_eq!(diagnostic.hints(), [Hint::text("y")]);
    }

    #[test]
    fn a_failure_that_names_its_server_gets_no_prefix() {
        let failure = Failure::usage("The server `fx` failed")
            .about("fx")
            .server("fx");
        assert_eq!(failure.diagnostic().prefix(), None);
        assert_eq!(failure.server_name(), Some("fx"));
    }

    #[test]
    fn rare_parts_stay_out_of_a_failure_that_has_none() {
        let failure = Failure::usage("x").with_login_challenge(None);
        assert!(failure.extra.is_none());
        let failure = failure.with_login_challenge(Some("Bearer".into()));
        assert_eq!(failure.login_challenge(), Some("Bearer"));
        assert_eq!(failure.pages(), []);
    }
}
