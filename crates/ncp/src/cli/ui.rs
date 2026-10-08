//! The output channels. Results go to stdout. Errors, warnings and hints go
//! to stderr. On a TTY the output is text for people. In a pipe stdout is
//! JSON, and stderr is JSON Lines: one JSON object on each line.

use std::{
    borrow::Cow,
    io::{ErrorKind, IsTerminal, Write},
    sync::Arc,
};

use serde_json::{Value, json};

use super::outcome::Failure;
use crate::client::stdio::ServerLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

impl Format {
    /// Text when stdout is a TTY, else JSON.
    #[must_use]
    pub fn detect() -> Self {
        if std::io::stdout().is_terminal() {
            Self::Text
        } else {
            Self::Json
        }
    }
}

/// How much diagnostic output goes to stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    /// `-q`: no hints and no warnings. Errors stay.
    Quiet,
    Normal,
    /// `-v`: where each value comes from, the proxy route, the era.
    Verbose,
    /// `-vv`: also the JSON-RPC messages and the HTTP requests.
    Trace,
}

impl Verbosity {
    /// The level for `-q` and the count of `-v` flags. clap does not permit
    /// both.
    #[must_use]
    pub fn from_flags(quiet: bool, count: u8) -> Self {
        match count {
            0 if quiet => Self::Quiet,
            0 => Self::Normal,
            1 => Self::Verbose,
            _ => Self::Trace,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ui {
    pub format: Format,
    pub verbosity: Verbosity,
}

impl Ui {
    /// Writes the result: the text for a TTY, or the JSON value.
    ///
    /// # Errors
    ///
    /// Returns a failure when stdout cannot be written.
    pub fn result(
        self,
        text: impl FnOnce() -> String,
        json: impl FnOnce() -> Value,
    ) -> Result<(), Failure> {
        match self.format {
            Format::Text => write_stdout(&printable(&text())),
            Format::Json => write_stdout(&json_line(&json())),
        }
    }

    pub fn warning(self, server: Option<&str>, message: &str) {
        if self.verbosity > Verbosity::Quiet {
            self.stderr(&note("warning", self.format, server, message));
        }
    }

    pub fn hint(self, server: Option<&str>, message: &str) {
        if self.verbosity > Verbosity::Quiet {
            self.stderr(&note("hint", self.format, server, message));
        }
    }

    /// A `-v` diagnostic. It never holds a secret value.
    pub fn debug(self, server: Option<&str>, message: &str) {
        if self.verbosity >= Verbosity::Verbose {
            self.stderr(&note("debug", self.format, server, message));
        }
    }

    /// A line that is ready for stderr, for example a `-vv` trace or the
    /// timings.
    pub fn diagnostic(self, line: &str) {
        self.stderr(line);
    }

    pub fn error(self, failure: &Failure) {
        self.stderr(&error_text(self.format, failure));
    }

    /// Text goes out with control characters as escapes. JSON lines are
    /// escaped already.
    fn stderr(self, text: &str) {
        match self.format {
            Format::Text => stderr(&printable(text)),
            Format::Json => stderr(text),
        }
    }

    /// The route of the stderr of a stdio server. Text: each line, with
    /// control characters as escapes. JSON:
    /// `{"serverLog": {"server": "...", "message": "..."}}`.
    #[must_use]
    pub fn server_log(self, server: &str) -> ServerLog {
        let server = server.to_string();
        ServerLog::Lines(Arc::new(move |line| match self.format {
            Format::Text => self.stderr(&format!("{line}\n")),
            Format::Json => {
                self.stderr(&json_line(
                    &json!({"serverLog": {"server": server, "message": line}}),
                ));
            }
        }))
    }
}

/// Writes to stdout. A closed pipe (for example `ncp ... | head`) is not an
/// error of the command, but a full disk is.
///
/// # Errors
///
/// Returns a failure when stdout cannot be written.
pub fn write_stdout(text: &str) -> Result<(), Failure> {
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => Err(Failure::protocol(format!(
            "cannot write the result to stdout: {error}"
        ))),
        _ => Ok(()),
    }
}

/// The text with each control character as an escape, except new line,
/// tab, and the carriage return of a CRLF line end. A server or a configuration file can contain terminal escape
/// sequences, which can change the terminal title, write the clipboard, or
/// hide lines.
#[must_use]
pub fn printable(text: &str) -> Cow<'_, str> {
    let unsafe_char = |c: char| c.is_control() && c != '\n' && c != '\t';
    if text.chars().any(unsafe_char) {
        // A CRLF line end is a normal line end on a terminal.
        Cow::Owned(
            text.replace("\r\n", "\n")
                .chars()
                .map(|c| {
                    if unsafe_char(c) {
                        c.escape_unicode().to_string()
                    } else {
                        c.to_string()
                    }
                })
                .collect(),
        )
    } else {
        Cow::Borrowed(text)
    }
}

fn stderr(text: &str) {
    let _ = std::io::stderr().lock().write_all(text.as_bytes());
}

/// One JSON value on one line. `serde_json` leaves DEL and the C1 control
/// characters as they are, which is valid JSON. A terminal can still act on
/// them when a person reads the stream, so they go out as `\u00XX` escapes.
#[must_use]
pub fn json_line(value: &Value) -> String {
    let text = value.to_string();
    let escaped: String = if text.chars().any(is_raw_control) {
        text.chars()
            .map(|c| {
                if is_raw_control(c) {
                    format!("\\u{:04x}", u32::from(c))
                } else {
                    c.to_string()
                }
            })
            .collect()
    } else {
        text
    };
    format!("{escaped}\n")
}

fn is_raw_control(c: char) -> bool {
    ('\u{7f}'..='\u{9f}').contains(&c)
}

/// A warning or a hint. Text: `warning: message`. JSON:
/// `{"warning": {"server": "...", "message": "..."}}`.
#[must_use]
pub fn note(kind: &str, format: Format, server: Option<&str>, message: &str) -> String {
    match format {
        Format::Text => match server {
            Some(server) => format!("{kind}: server {server}: {message}\n"),
            None => format!("{kind}: {message}\n"),
        },
        Format::Json => {
            let mut body = json!({"message": message});
            if let Some(server) = server {
                body["server"] = json!(server);
            }
            json_line(&json!({ kind: body }))
        }
    }
}

/// The error. Text: `error: message` and `hint: hint`. JSON: the error
/// object of the design, on one line.
#[must_use]
pub fn error_text(format: Format, failure: &Failure) -> String {
    match format {
        Format::Text => {
            let server = failure
                .server
                .as_deref()
                .map(|server| format!("server {server}: "))
                .unwrap_or_default();
            let hint = failure
                .hint
                .as_deref()
                .map(|hint| format!("hint: {hint}\n"))
                .unwrap_or_default();
            format!("error: {server}{}\n{hint}", failure.message)
        }
        Format::Json => {
            let mut body = json!({
                "code": failure.exit.name(),
                "exit": failure.exit.code(),
                "message": failure.message,
            });
            if let Some(server) = &failure.server {
                body["server"] = json!(server);
            }
            if let Some(hint) = &failure.hint {
                body["hint"] = json!(hint);
            }
            json_line(&json!({ "error": body }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::outcome::Exit;

    fn failure() -> Failure {
        Failure::new(Exit::Auth, "the server needs a login")
            .server("github")
            .hint("ncp auth login github")
    }

    #[test]
    fn json_error_is_one_line_with_code_exit_server_message_and_hint() {
        let line = error_text(Format::Json, &failure());
        assert!(line.ends_with('\n') && line.lines().count() == 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value,
            json!({"error": {"code": "auth", "exit": 4, "server": "github",
                "message": "the server needs a login", "hint": "ncp auth login github"}})
        );
    }

    #[test]
    fn text_error_names_the_server_and_gives_the_hint_on_its_own_line() {
        assert_eq!(
            error_text(Format::Text, &failure()),
            "error: server github: the server needs a login\nhint: ncp auth login github\n"
        );
    }

    #[test]
    fn printable_text_escapes_terminal_control_characters() {
        assert_eq!(printable("a\tb\nc"), "a\tb\nc");
        assert_eq!(
            printable("\u{1b}]0;title\u{7}x\r\u{9b}"),
            "\\u{1b}]0;title\\u{7}x\\u{d}\\u{9b}"
        );
    }

    #[test]
    fn json_line_escapes_del_and_c1_control_characters() {
        let line = json_line(&json!({"m": "a\u{7f}b\u{9b}c\u{1b}"}));
        assert_eq!(line, "{\"m\":\"a\\u007fb\\u009bc\\u001b\"}\n");
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["m"], "a\u{7f}b\u{9b}c\u{1b}");
    }

    #[test]
    fn printable_text_keeps_crlf_line_ends() {
        assert_eq!(printable("a\r\nb\rc"), "a\nb\\u{d}c");
    }

    #[test]
    fn notes_have_text_and_json_forms() {
        assert_eq!(
            note("warning", Format::Text, Some("s"), "unknown key"),
            "warning: server s: unknown key\n"
        );
        assert_eq!(note("hint", Format::Text, None, "x"), "hint: x\n");
        let value: Value =
            serde_json::from_str(&note("hint", Format::Json, Some("s"), "x")).unwrap();
        assert_eq!(value, json!({"hint": {"message": "x", "server": "s"}}));
    }
}
