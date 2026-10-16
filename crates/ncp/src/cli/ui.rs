//! The output channels. Results go to stdout. Errors, warnings and hints go
//! to stderr. On a TTY the output is text for people. In a pipe stdout is
//! JSON, and stderr is JSON Lines: one JSON object on each line. The rules
//! of the text of a diagnostic are in `docs/design/output-messages.md`.

use std::{
    borrow::Cow,
    io::{ErrorKind, IsTerminal, Write},
    sync::Arc,
};

use serde_json::{Value, json};

use super::outcome::{Diagnostic, Failure, Hint, Warning};
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
    /// `-q`: no hints, no warnings and no progress. An error shows with
    /// its causes and hints.
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
            // JSON on a TTY comes only from `--output-format json`. A person
            // reads it, so it is pretty (design section 4).
            Format::Json if std::io::stdout().is_terminal() => {
                write_stdout(&format!("{}\n", escape_raw_controls(pretty_json(&json()))))
            }
            Format::Json => write_stdout(&json_line(&json())),
        }
    }

    pub fn warn(self, warning: &Warning) {
        if self.verbosity > Verbosity::Quiet {
            self.note(&warning_text(self.format, warning));
        }
    }

    /// A standalone hint. Its message names the server, so the text has
    /// no prefix.
    pub fn hint(self, server: Option<&str>, hint: &Hint) {
        if self.verbosity > Verbosity::Quiet {
            self.note(&hint_text(self.format, server, hint));
        }
    }

    /// The message must never hold a secret value.
    pub fn debug(self, server: Option<&str>, message: &str) {
        if self.verbosity >= Verbosity::Verbose {
            self.note(&debug_line(self.format, server, message));
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

    // Text goes out with control characters as escapes. JSON lines have
    // their escapes already.
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

/// A closed pipe (for example `ncp ... | head`) is not an error of the command,
/// but a full disk is.
///
/// # Errors
///
/// Returns a failure when `ncp` cannot write to stdout.
pub fn write_stdout_bytes(bytes: &[u8]) -> Result<(), Failure> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => {
            Err(Failure::protocol("ncp cannot write the result to stdout").caused_by(&error))
        }
        _ => Ok(()),
    }
}

/// # Errors
///
/// Returns a failure when `ncp` cannot write to stdout.
pub fn write_stdout(text: &str) -> Result<(), Failure> {
    write_stdout_bytes(text.as_bytes())
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

// A Unicode character that changes the order of text, has no width, or
// looks like a space:
// - bidirectional marks, embeddings, overrides and isolates
// - zero-width characters, the soft hyphen and the byte order mark
// - line and paragraph separators, and Hangul fillers
// - the supplementary variation selectors and tag characters
//
// Text in tag characters is invisible, and a server can hide instructions
// in it. The variation selectors U+FE00 to U+FE0F stay, because many
// emoji have one. The zero width joiner U+200D stays too. It joins the
// parts of many emoji, and it does not change the order of text.
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

// A number of a progress notification: with no decimals when it is whole.
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

// DEL, the C1 control characters and the invisible characters of
// `printable` become `\uXXXX` escapes, with a surrogate pair above
// U+FFFF. The text stays valid JSON.
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

/// A message of a note with each line after the first indented. A message can
/// come from a server, and a line of its own can look like a note of `ncp`. An
/// indented line cannot. A message of `ncp` keeps its lines.
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

/// A debug line. Text: `debug: server NAME: message`. JSON:
/// `{"debug": {"server": "...", "message": "..."}}`.
#[must_use]
pub fn debug_line(format: Format, server: Option<&str>, message: &str) -> String {
    let kind = "debug";
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

/// A standalone hint. Text: `hint: message`. JSON: `{"hint": {"message",
/// "server", "command"}}`, with `server` and `command` only when present.
#[must_use]
pub fn hint_text(format: Format, server: Option<&str>, hint: &Hint) -> String {
    match format {
        Format::Text => format!("hint: {}\n", one_line(hint.message())),
        Format::Json => {
            let mut body = json!({"message": one_line(hint.message())});
            if let Some(server) = server {
                body["server"] = json!(server);
            }
            if let Some(command) = hint.command() {
                body["command"] = json!(command);
            }
            json_line(&json!({ "hint": body }))
        }
    }
}

/// The error. Text: the `error:` line, the data lines, the `cause:` lines
/// and the hints. JSON: the error object of the design, on one line.
#[must_use]
pub fn error_text(format: Format, failure: &Failure) -> String {
    let diagnostic = failure.diagnostic();
    match format {
        Format::Text => diagnostic_text("error", ERROR_DATA_INDENT, diagnostic),
        Format::Json => {
            let mut body = diagnostic_json(diagnostic);
            body["code"] = json!(failure.exit().name());
            body["exit"] = json!(failure.exit().code());
            if let Some(destination) = failure.destination() {
                body["destination"] = super::secrets::destination_json(destination);
            }
            if !failure.pages().is_empty() {
                let pages: Vec<Value> = failure
                    .pages()
                    .iter()
                    .map(|page| json!({"url": page.url, "message": page.message}))
                    .collect();
                body["pages"] = json!(pages);
            }
            json_line(&json!({ "error": body }))
        }
    }
}

/// The warning, in the format of an error with no exit code.
#[must_use]
pub fn warning_text(format: Format, warning: &Warning) -> String {
    let diagnostic = warning.diagnostic();
    match format {
        Format::Text => diagnostic_text("warning", WARNING_DATA_INDENT, diagnostic),
        Format::Json => json_line(&json!({ "warning": diagnostic_json(diagnostic) })),
    }
}

// A data line starts at the column of the message text.
const ERROR_DATA_INDENT: &str = "       ";
const WARNING_DATA_INDENT: &str = "         ";
const CAUSE_LABEL: &str = "  cause: ";
const CAUSE_INDENT: &str = "         ";

// Only a label or an empty line starts at column 0. Thus a line of a
// server cannot look like a line of `ncp` (B60).
fn diagnostic_text(label: &str, data_indent: &str, diagnostic: &Diagnostic) -> String {
    let head = format!("{label}: {}", indented(diagnostic.message()));
    let data = shown_data(diagnostic.data())
        .into_iter()
        .map(|line| format!("{data_indent}{}", indent_lines(&line, data_indent)));
    let causes = diagnostic
        .causes()
        .iter()
        .map(|cause| format!("{CAUSE_LABEL}{}", indent_lines(&shown(cause), CAUSE_INDENT)));
    let hints = diagnostic
        .hints()
        .iter()
        .map(|hint| format!("\nhint: {}", one_line(hint.message())));
    std::iter::once(head)
        .chain(data)
        .chain(causes)
        .chain(hints)
        .map(|line| line + "\n")
        .collect()
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Value {
    let causes: Vec<String> = diagnostic
        .causes()
        .iter()
        .map(|cause| shown(cause))
        .collect();
    let hints: Vec<Value> = diagnostic
        .hints()
        .iter()
        .map(|hint| match hint.command() {
            Some(command) => json!({"message": one_line(hint.message()), "command": command}),
            None => json!({"message": one_line(hint.message())}),
        })
        .collect();
    let mut body = json!({
        "message": line_ends(diagnostic.message()),
        "data": shown_data(diagnostic.data()),
        "causes": causes,
        "hints": hints,
    });
    if let Some(server) = diagnostic.server_name() {
        body["server"] = json!(server);
    }
    body
}

// Each line after the first gets the indent. An empty line stays empty.
fn indent_lines(text: &str, indent: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .enumerate()
        .map(|(index, line)| match (index, line.is_empty()) {
            (0, _) | (_, true) => line.to_string(),
            _ => format!("{indent}{line}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const SHOWN_LINES: usize = 20;
const SHOWN_BYTES: usize = 2048;

// A long body of a server hides the hints. Thus a text shows at most 20
// lines and 2048 bytes, and a last line tells how much did not show
// (design rule 16).
fn shown(text: &str) -> String {
    let text = line_ends(text);
    if text.len() <= SHOWN_BYTES && text.lines().count() <= SHOWN_LINES {
        return text;
    }
    let (end, count) = text
        .split_inclusive('\n')
        .take(SHOWN_LINES)
        .scan(0, |end, line| {
            *end += line.len();
            (*end <= SHOWN_BYTES).then_some(*end)
        })
        .zip(1..)
        .last()
        .unwrap_or((0, 0));
    // A line that is longer than the limit shows its first bytes, because
    // a body of one long line usually holds the reason.
    let next_is_long = text[end..]
        .split('\n')
        .next()
        .is_some_and(|line| line.len() > SHOWN_BYTES);
    let end = if count < SHOWN_LINES && next_is_long {
        (0..=SHOWN_BYTES)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(end)
    } else {
        end
    };
    let kept = text[..end].strip_suffix('\n').unwrap_or(&text[..end]);
    format!(
        "{kept}\n(ncp did not show {} bytes)",
        text.len() - kept.len()
    )
}

// A terminal also starts a new line at a carriage return. Thus each line
// end counts in the limit, and text and JSON have the same lines.
fn line_ends(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

// The data lines of one diagnostic share one limit. A data line with a
// line end is two data lines.
fn shown_data(lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }
    shown(&lines.join("\n"))
        .split('\n')
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::outcome::{Exit, Hint};

    fn failure() -> Failure {
        Failure::new(Exit::Auth, "The server `github` needs a login")
            .about("github")
            .cause("HTTP 401")
            .hint(Hint::run("log in", "ncp auth login github"))
            .hint("Examine the token")
    }

    #[test]
    fn json_error_is_one_line_with_each_part_of_the_design() {
        let line = error_text(Format::Json, &failure());
        assert!(line.ends_with('\n') && line.lines().count() == 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value,
            json!({"error": {"code": "auth", "exit": 4, "server": "github",
                "message": "The server `github` needs a login", "data": [],
                "causes": ["HTTP 401"],
                "hints": [
                    {"message": "To log in, run `ncp auth login github`",
                        "command": "ncp auth login github"},
                    {"message": "Examine the token"}]}})
        );
    }

    #[test]
    fn text_error_has_its_causes_then_an_empty_line_before_each_hint() {
        assert_eq!(
            error_text(Format::Text, &failure()),
            "error: The server `github` needs a login\n  cause: HTTP 401\n\n\
             hint: To log in, run `ncp auth login github`\n\nhint: Examine the token\n"
        );
    }

    #[test]
    fn a_line_from_a_server_cannot_start_at_column_0() {
        let failure = Failure::protocol("x\nhint: run evil")
            .data("d\nerror: e")
            .cause("a\nerror: b\n\nc")
            .hint("a\nb");
        assert_eq!(
            error_text(Format::Text, &failure),
            "error: x\n  hint: run evil\n       d\n       error: e\n\
             \x20 cause: a\n         error: b\n\n         c\n\nhint: a\\nb\n"
        );
        let warning = Warning::new("w").data("d\ne");
        assert_eq!(
            warning_text(Format::Text, &warning),
            "warning: w\n         d\n         e\n"
        );
    }

    #[test]
    fn a_long_cause_shows_its_first_lines_and_the_size_of_the_rest() {
        let lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
        let text = lines.join("\n");
        let failure = Failure::protocol("m").cause(text.clone());
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        let shown = value["error"]["causes"][0].as_str().unwrap().to_string();
        let kept = lines[..20].join("\n");
        assert_eq!(
            shown,
            format!(
                "{kept}\n(ncp did not show {} bytes)",
                text.len() - kept.len()
            )
        );
        let text_form = error_text(Format::Text, &failure);
        assert!(text_form.contains("\n         line 20\n         (ncp did not show"));
        let long = "é".repeat(1500);
        let failure = Failure::protocol("m").cause(long);
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        assert_eq!(
            value["error"]["causes"][0],
            format!("{}\n(ncp did not show 952 bytes)", "é".repeat(1024))
        );
    }

    #[test]
    fn a_carriage_return_counts_as_a_line_end_in_the_limit() {
        let failure = Failure::protocol("m").cause("x\r".repeat(500));
        let text = error_text(Format::Text, &failure);
        assert_eq!(text.lines().count(), 22, "{text}");
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        let cause = value["error"]["causes"][0].as_str().unwrap();
        assert!(!cause.contains('\r'), "{cause}");
        assert_eq!(cause.lines().count(), 21);
    }

    #[test]
    fn a_long_line_after_the_first_line_shows_its_first_bytes() {
        let failure = Failure::protocol("m").cause(format!("Client error:\n{}", "b".repeat(5000)));
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        let cause = value["error"]["causes"][0].as_str().unwrap();
        let kept = format!("Client error:\n{}", "b".repeat(2048 - 14));
        assert_eq!(
            cause,
            format!("{kept}\n(ncp did not show {} bytes)", 5014 - kept.len())
        );
    }

    #[test]
    fn a_short_line_after_the_lines_that_fit_is_not_cut() {
        let line = "a".repeat(135);
        let text = format!("{}{}", format!("{line}\n").repeat(15), "b".repeat(100));
        let failure = Failure::protocol("m").cause(format!("{text}\nmore"));
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        let cause = value["error"]["causes"][0].as_str().unwrap();
        assert!(!cause.contains("bbb"), "{cause}");
        assert_eq!(cause.lines().count(), 16);
    }

    #[test]
    fn a_data_line_with_a_line_end_is_two_data_lines_in_text_and_json() {
        let failure = Failure::usage("m:").data("a\nb");
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        assert_eq!(value["error"]["data"], json!(["a", "b"]));
        assert_eq!(
            error_text(Format::Text, &failure),
            "error: m:\n       a\n       b\n"
        );
    }

    #[test]
    fn all_data_lines_share_one_limit() {
        let failure = (1..=25).fold(Failure::usage("m:"), |failure, n| {
            failure.data(format!("row {n}"))
        });
        let value: Value = serde_json::from_str(&error_text(Format::Json, &failure)).unwrap();
        let data = value["error"]["data"].as_array().unwrap();
        assert_eq!(data.len(), 21);
        assert_eq!(data[19], "row 20");
        assert!(data[20].as_str().unwrap().starts_with("(ncp did not show"));
    }

    #[test]
    fn warnings_have_text_and_json_forms() {
        let warning = Warning::new("The server `s` sent an unknown key")
            .about("s")
            .cause("c")
            .hint("h");
        assert_eq!(
            warning_text(Format::Text, &warning),
            "warning: The server `s` sent an unknown key\n  cause: c\n\nhint: h\n"
        );
        let value: Value = serde_json::from_str(&warning_text(Format::Json, &warning)).unwrap();
        assert_eq!(
            value,
            json!({"warning": {"server": "s", "message": "The server `s` sent an unknown key",
                "data": [], "causes": ["c"], "hints": [{"message": "h"}]}})
        );
    }

    #[test]
    fn standalone_hints_have_text_and_json_forms() {
        let hint = Hint::run("see the servers", "ncp server list").after("No server is here");
        assert_eq!(
            hint_text(Format::Text, None, &hint),
            "hint: No server is here. To see the servers, run `ncp server list`.\n"
        );
        assert_eq!(
            hint_text(Format::Text, None, &Hint::text("a\nerror: b")),
            "hint: a\\nerror: b\n"
        );
        let value: Value =
            serde_json::from_str(&hint_text(Format::Json, Some("s"), &hint)).unwrap();
        assert_eq!(
            value,
            json!({"hint": {"server": "s", "command": "ncp server list",
                "message": "No server is here. To see the servers, run `ncp server list`."}})
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
    fn a_progress_message_stays_on_one_line() {
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
}
