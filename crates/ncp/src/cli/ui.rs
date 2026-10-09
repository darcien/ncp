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
    /// The level for the counts of `-q` and `-v` flags. clap does not
    /// permit both. `-qq` is the same as `-q` for now, as uv accepts it.
    #[must_use]
    pub fn from_flags(quiet: u8, count: u8) -> Self {
        match count {
            0 if quiet > 0 => Self::Quiet,
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
    /// Color in the labels of errors, warnings, hints and diagnostics on
    /// stderr. Only in text output.
    pub color: bool,
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
            // JSON on a TTY comes only from `--output-format json`; a
            // person reads it, so it is pretty (section 4).
            Format::Json if std::io::stdout().is_terminal() => {
                write_stdout(&format!("{}\n", escape_raw_controls(pretty_json(&json()))))
            }
            Format::Json => write_stdout(&json_line(&json())),
        }
    }

    pub fn warning(self, server: Option<&str>, message: &str) {
        if self.verbosity > Verbosity::Quiet {
            self.note(&note("warning", self.format, server, message));
        }
    }

    pub fn hint(self, server: Option<&str>, message: &str) {
        if self.verbosity > Verbosity::Quiet {
            self.note(&note("hint", self.format, server, message));
        }
    }

    /// A `-v` diagnostic. It never holds a secret value.
    pub fn debug(self, server: Option<&str>, message: &str) {
        if self.verbosity >= Verbosity::Verbose {
            self.note(&note("debug", self.format, server, message));
        }
    }

    /// A line that is ready for stderr, for example a `-vv` trace or the
    /// timings.
    pub fn diagnostic(self, line: &str) {
        self.stderr(line);
    }

    pub fn error(self, failure: &Failure) {
        self.note(&error_text(self.format, failure));
    }

    /// A note of `ncp`, with colored labels when color is on. A line of a
    /// server log never gets color, so that a server cannot look like
    /// `ncp`.
    fn note(self, text: &str) {
        match (self.format, self.color) {
            (Format::Text, true) => stderr(&painted(&printable(text))),
            _ => self.stderr(text),
        }
    }

    /// Text goes out with control characters as escapes. JSON lines are
    /// escaped already.
    fn stderr(self, text: &str) {
        match self.format {
            Format::Text => stderr(&printable(text)),
            Format::Json => stderr(text),
        }
    }

    /// The progress of the requests to the server, on stderr. `-q` hides
    /// it, as it hides hints.
    #[must_use]
    pub fn progress(self, server: &str) -> crate::client::ProgressSink {
        if self.verbosity == Verbosity::Quiet {
            return crate::client::ProgressSink::default();
        }
        let server = server.to_string();
        let last = std::sync::Mutex::new(None::<std::time::Instant>);
        crate::client::ProgressSink::new(move |progress| {
            // At most a few lines a second (the spec: rate limiting), but
            // always the last step.
            let now = std::time::Instant::now();
            let done = progress
                .total
                .is_some_and(|total| progress.progress >= total);
            let Ok(mut last) = last.lock() else { return };
            if done || last.is_none_or(|at| now.duration_since(at) >= PROGRESS_STEP) {
                *last = Some(now);
                self.stderr(&progress_line(self.format, &server, progress));
            }
        })
    }

    /// The route of the stderr of a stdio server. Text: each line, with
    /// control characters as escapes. JSON:
    /// `{"serverLog": {"server": "...", "message": "..."}}`.
    #[must_use]
    pub fn server_log(self, server: &str) -> ServerLog {
        let server = server.to_string();
        ServerLog::Lines(Arc::new(move |line| match self.format {
            Format::Text => printable(&format!("{line}\n")).into_owned(),
            Format::Json => json_line(&json!({"serverLog": {"server": server, "message": line}})),
        }))
    }
}

/// Writes bytes to stdout, for example the contents of a resource.
///
/// # Errors
///
/// Returns a failure when stdout cannot be written.
pub fn write_stdout_bytes(bytes: &[u8]) -> Result<(), Failure> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => Err(Failure::protocol(format!(
            "cannot write the result to stdout: {error}"
        ))),
        _ => Ok(()),
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

/// The shortest time between two progress lines.
const PROGRESS_STEP: std::time::Duration = std::time::Duration::from_millis(200);

/// A number of a progress notification: with no decimals when it is whole.
fn amount(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

/// Text: `progress: server NAME: 3/10 message`. JSON: `{"progress":
/// {"server", "progress", "total", "message"}}`, with the fields of the
/// notification.
#[must_use]
pub fn progress_line(format: Format, server: &str, progress: &crate::client::Progress) -> String {
    match format {
        Format::Text => {
            let total = progress
                .total
                .map(|total| format!("/{}", amount(total)))
                .unwrap_or_default();
            let message = progress
                .message
                .as_deref()
                .map(|message| format!(" {}", one_line(&message.replace('\t', " "))))
                .unwrap_or_default();
            format!(
                "progress: server {server}: {}{total}{message}\n",
                amount(progress.progress)
            )
        }
        Format::Json => {
            let mut body = json!({"server": server, "progress": progress.progress});
            if let Some(total) = progress.total {
                body["total"] = json!(total);
            }
            if let Some(message) = &progress.message {
                body["message"] = json!(message);
            }
            json_line(&json!({ "progress": body }))
        }
    }
}

/// The color of each label, as ANSI SGR codes.
const LABELS: [(&str, &str); 4] = [
    ("error:", "1;31"),
    ("warning:", "1;33"),
    ("hint:", "1;36"),
    ("debug:", "2"),
];

/// The text with the label at the start of each line in its color.
#[must_use]
pub fn painted(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            LABELS
                .iter()
                .find_map(|(label, code)| {
                    line.strip_prefix(label)
                        .map(|rest| format!("\u{1b}[{code}m{label}\u{1b}[0m{rest}"))
                })
                .unwrap_or_else(|| line.to_string())
        })
        .collect()
}

/// Color on stderr: `--color`, else on when stderr is a TTY, `NO_COLOR` is
/// not set or empty (no-color.org), and `TERM` is not `dumb`.
#[must_use]
pub fn color_on(choice: Option<bool>, stderr_is_tty: bool, env: &crate::expand::Env) -> bool {
    choice.unwrap_or_else(|| {
        stderr_is_tty
            && env.get("NO_COLOR").is_none_or(String::is_empty)
            && env.get("TERM").is_none_or(|term| term != "dumb")
    })
}

fn stderr(text: &str) {
    let _ = std::io::stderr().lock().write_all(text.as_bytes());
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// One JSON value on one line. `serde_json` leaves DEL and the C1 control
/// characters as they are, which is valid JSON. A terminal can still act on
/// them when a person reads the stream, so they go out as `\u00XX` escapes.
#[must_use]
pub fn json_line(value: &Value) -> String {
    format!("{}\n", escape_raw_controls(value.to_string()))
}

/// JSON text with DEL and the C1 control characters as `\u00XX` escapes,
/// which keeps it valid JSON.
fn escape_raw_controls(text: String) -> String {
    if text.chars().any(is_raw_control) {
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
    }
}

/// A message of a note with each line after the first indented. A message
/// can come from a server, and a line of its own could look like a note of
/// `ncp`; an indented line cannot. A message of `ncp` keeps its lines.
fn indented(message: &str) -> String {
    message
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\n  ")
}

/// A progress message on one line: a progress line is one line.
fn one_line(message: &str) -> String {
    message
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
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
            Some(server) => format!("{kind}: server {server}: {}\n", indented(message)),
            None => format!("{kind}: {}\n", indented(message)),
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
                .map(|hint| format!("hint: {}\n", indented(hint)))
                .unwrap_or_default();
            format!("error: {server}{}\n{hint}", indented(&failure.message))
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
    fn a_line_in_a_message_cannot_look_like_a_note() {
        let failure = Failure::new(Exit::Protocol, "x\nhint: run evil").hint("a\nb");
        assert_eq!(
            error_text(Format::Text, &failure),
            "error: x\n  hint: run evil\nhint: a\n  b\n"
        );
        assert_eq!(
            note("warning", Format::Text, None, "a\r\nerror: b"),
            "warning: a\n  error: b\n"
        );
        let progress = crate::client::Progress {
            progress: 1.0,
            total: None,
            message: Some("a\nerror: b".into()),
        };
        assert_eq!(
            progress_line(Format::Text, "fx", &progress),
            "progress: server fx: 1 a\\nerror: b\n"
        );
    }

    #[test]
    fn progress_has_text_and_json_forms() {
        let progress = crate::client::Progress {
            progress: 3.0,
            total: Some(10.0),
            message: Some("files".into()),
        };
        assert_eq!(
            progress_line(Format::Text, "fx", &progress),
            "progress: server fx: 3/10 files\n"
        );
        let value: Value =
            serde_json::from_str(&progress_line(Format::Json, "fx", &progress)).unwrap();
        assert_eq!(
            value,
            json!({"progress": {"server": "fx", "progress": 3.0, "total": 10.0, "message": "files"}})
        );
        let half = crate::client::Progress {
            progress: 0.5,
            total: None,
            message: None,
        };
        assert_eq!(
            progress_line(Format::Text, "fx", &half),
            "progress: server fx: 0.5\n"
        );
    }

    #[test]
    fn labels_get_color_at_the_start_of_a_line_only() {
        assert_eq!(
            painted("error: x\nhint: y error: z\n"),
            "\u{1b}[1;31merror:\u{1b}[0m x\n\u{1b}[1;36mhint:\u{1b}[0m y error: z\n"
        );
    }

    #[test]
    fn color_follows_the_option_then_the_tty_no_color_and_term() {
        let env = |pairs: &[(&str, &str)]| -> crate::expand::Env {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert!(color_on(None, true, &env(&[])));
        assert!(!color_on(None, false, &env(&[])));
        assert!(!color_on(None, true, &env(&[("NO_COLOR", "1")])));
        assert!(color_on(None, true, &env(&[("NO_COLOR", "")])));
        assert!(!color_on(None, true, &env(&[("TERM", "dumb")])));
        assert!(color_on(Some(true), false, &env(&[("NO_COLOR", "1")])));
        assert!(!color_on(Some(false), true, &env(&[])));
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
