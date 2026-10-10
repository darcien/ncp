//! Exit codes and failures. Each class of result has one exit code and one
//! `code` name in the JSON error object.

use std::process::ExitCode;

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

/// A result that is not a success. The message says what failed. The hints
/// give the fix, when `ncp` knows it: each hint is one command, so the first
/// hint is the first fix to try.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub exit: Exit,
    pub server: Option<String>,
    pub message: String,
    /// Boxed like `pages`, so that each `Result` with a failure stays small.
    pub hints: Box<[String]>,
    /// Set for a `401` on a connection with no token, with the challenge
    /// of the answer: a stored login can answer it.
    pub login_challenge: Option<String>,
    /// The pages that the server asks the user to open before it can
    /// answer. A person at a TTY can open them; an agent gets them in the
    /// JSON error.
    pub pages: Box<[crate::elicit::Page]>,
    /// The destination of a stored value that no place has (S9). Boxed,
    /// as `hints` and `pages` are.
    pub destination: Option<Box<crate::credential::Destination>>,
}

impl Failure {
    #[must_use]
    pub fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            exit,
            server: None,
            message: message.into(),
            hints: Box::default(),
            login_challenge: None,
            pages: Box::default(),
            destination: None,
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

    /// Adds a hint after the hints that the failure has.
    #[must_use]
    pub fn hint(self, hint: impl Into<String>) -> Self {
        let mut hints = self.hints.into_vec();
        hints.push(hint.into());
        Self {
            hints: hints.into_boxed_slice(),
            ..self
        }
    }

    #[must_use]
    pub fn destination(self, destination: crate::credential::Destination) -> Self {
        Self {
            destination: Some(Box::new(destination)),
            ..self
        }
    }

    #[must_use]
    pub fn server(mut self, server: impl Into<String>) -> Self {
        self.server = Some(server.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
