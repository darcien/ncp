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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    /// `-q`: no hints and no warnings. Errors stay.
    Quiet,
    Normal,
    /// `-v`: where each value comes from, the proxy route, the protocol version.
    Verbose,
    /// `-vv`: also the JSON-RPC messages and the HTTP requests.
    Trace,
}

impl Verbosity {
    /// clap does not permit both. `-qq` is the same as `-q` for now, as
    /// uv accepts it.
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
}

impl Ui {
    /// # Errors
    ///
    /// Returns a failure when `ncp` cannot write to stdout.
    pub fn result(
        self,
        text: impl FnOnce() -> String,
        json: impl FnOnce() -> Value,
    ) -> Result<(), Failure> {
        match self.format {
            Format::Text => write_stdout(&printable(&text())),
            // JSON on a TTY comes only from `--output-format json`; a
            // person reads it, so it is pretty (design section 4).
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

    /// The message must never hold a secret value.
    pub fn debug(self, server: Option<&str>, message: &str) {
        if self.verbosity >= Verbosity::Verbose {
            self.note(&note("debug", self.format, server, message));
        }
    }

    pub fn diagnostic(self, line: &str) {
        self.stderr(line);
    }

    pub fn error(self, failure: &Failure) {
        self.note(&error_text(self.format, failure));
    }

    fn note(self, text: &str) {
        self.stderr(text);
    }

    /// Text goes out with control characters as escapes. JSON lines have
    /// their escapes already.
    fn stderr(self, text: &str) {
        match self.format {
            Format::Text => stderr(&printable(text)),
            Format::Json => stderr(text),
        }
    }

    /// `-q` hides the progress, as it hides hints.
    #[must_use]
    pub fn progress(self, server: &str) -> crate::client::ProgressSink {
        if self.verbosity == Verbosity::Quiet {
            return crate::client::ProgressSink::default();
        }
        let server = server.to_string();
        let last = std::sync::Mutex::new(None::<std::time::Instant>);
        crate::client::ProgressSink::new(move |progress| {
            // At most a few lines a second (MCP 2025-11-25,
            // basic/utilities/progress: rate limiting), but always the last
            // step.
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

/// # Errors
///
/// Returns a failure when `ncp` cannot write to stdout.
pub fn write_stdout_bytes(bytes: &[u8]) -> Result<(), Failure> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => Err(Failure::protocol(format!(
            "cannot write the result to stdout: {error}"
        ))),
        _ => Ok(()),
    }
}

/// A closed pipe (for example `ncp ... | head`) is not an error of the command,
/// but a full disk is.
///
/// # Errors
///
/// Returns a failure when `ncp` cannot write to stdout.
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

/// Each control character becomes an escape, except new line, tab, and the
/// carriage return of a CRLF line end. A server or a configuration file
/// can contain terminal escape sequences. These can change the terminal
/// title, write the clipboard, or hide lines. The characters that change
/// the direction of text or have no width also become escapes. They can
/// put the parts of a URL or a message in a different order on the screen.
#[must_use]
pub fn printable(text: &str) -> Cow<'_, str> {
    let unsafe_char = |c: char| (c.is_control() && c != '\n' && c != '\t') || invisible_format(c);
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

/// A Unicode character that changes the order of text, has no width, or
/// looks like a space:
/// - bidirectional marks, embeddings, overrides and isolates
/// - zero-width characters, the soft hyphen and the byte order mark
/// - line and paragraph separators, and Hangul fillers
/// - the supplementary variation selectors and tag characters
///
/// Text in tag characters is invisible, and a server can hide instructions
/// in it. The variation selectors U+FE00 to U+FE0F stay, because many
/// emoji have one. The zero width joiner U+200D stays too. It joins the
/// parts of many emoji, and it does not change the order of text.
fn invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{34f}'
            | '\u{61c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{180e}'
            | '\u{200b}'
            | '\u{200c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{3164}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{e0000}'..='\u{e007f}'
            | '\u{e0100}'..='\u{e01ef}'
    )
}

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

/// DEL, the C1 control characters and the invisible characters of
/// `printable` become `\uXXXX` escapes, with a surrogate pair above
/// U+FFFF. The text stays valid JSON.
fn escape_raw_controls(text: String) -> String {
    let escaped = |c: char| is_raw_control(c) || invisible_format(c);
    if text.chars().any(escaped) {
        text.chars()
            .map(|c| {
                if escaped(c) {
                    c.encode_utf16(&mut [0; 2])
                        .iter()
                        .fold(String::new(), |mut text, unit| {
                            let _ = std::fmt::Write::write_fmt(
                                &mut text,
                                format_args!("\\u{unit:04x}"),
                            );
                            text
                        })
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
#[must_use]
pub fn indented(message: &str) -> String {
    message
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\n  ")
}

/// A message on one line, for a progress line or a table cell: a new line
/// shows as `\n`.
#[must_use]
pub fn one_line(message: &str) -> String {
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

/// The error. Text: `error: message` and a `hint: hint` line for each
/// hint. JSON: the error object of the design, on one line, with the first
/// hint. Each other hint follows as a hint note (B3).
#[must_use]
pub fn error_text(format: Format, failure: &Failure) -> String {
    match format {
        Format::Text => {
            let server = failure
                .server
                .as_deref()
                .map(|server| format!("server {server}: "))
                .unwrap_or_default();
            let hints: String = failure
                .hints
                .iter()
                .map(|hint| note("hint", format, None, hint))
                .collect();
            format!("error: {server}{}\n{hints}", indented(&failure.message))
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
            if let Some(hint) = failure.hints.first() {
                body["hint"] = json!(hint);
            }
            if let Some(destination) = &failure.destination {
                body["destination"] = super::secrets::destination_json(destination);
            }
            if !failure.pages.is_empty() {
                let pages: Vec<Value> = failure
                    .pages
                    .iter()
                    .map(|page| json!({"url": page.url, "message": page.message}))
                    .collect();
                body["pages"] = json!(pages);
            }
            let more: String = failure
                .hints
                .iter()
                .skip(1)
                .map(|hint| note("hint", format, failure.server.as_deref(), hint))
                .collect();
            json_line(&json!({ "error": body })) + &more
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
    fn a_second_hint_follows_the_error_as_its_own_line() {
        let failure = failure().hint("ncp secret list");
        assert_eq!(
            error_text(Format::Text, &failure),
            "error: server github: the server needs a login\n\
             hint: ncp auth login github\nhint: ncp secret list\n"
        );
        let text = error_text(Format::Json, &failure);
        let lines: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines[0]["error"]["hint"], "ncp auth login github");
        assert_eq!(
            lines[1],
            json!({"hint": {"message": "ncp secret list", "server": "github"}})
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
    fn printable_text_escapes_direction_and_zero_width_characters() {
        assert_eq!(
            printable("a\u{202e}b\u{200b}c\u{2066}"),
            "a\\u{202e}b\\u{200b}c\\u{2066}"
        );
        assert_eq!(printable("é ü 日本"), "é ü 日本");
        // A family emoji: four people joined with U+200D.
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}";
        assert_eq!(printable(family), family);
        assert_eq!(printable("a\u{200c}b"), "a\\u{200c}b");
        assert_eq!(printable("a\u{e0041}"), "a\\u{e0041}");
        let line = json_line(&json!({"m": "a\u{202e}b\u{e0041}"}));
        assert_eq!(line, "{\"m\":\"a\\u202eb\\udb40\\udc41\"}\n");
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["m"], "a\u{202e}b\u{e0041}");
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
