//! Questions to a person at a TTY: the elicitation of a server.
//!
//! `ncp` asks only when stdin and stderr are a TTY. Else a server gets no
//! elicitation capability, and an `input_required` result gives exit 5
//! (design section 3). The questions go to stderr, so that stdout keeps
//! only the result. Each answer has the limit of `--timeout`, so that a
//! question that nobody answers does not keep `ncp` for ever.

use std::{
    io::IsTerminal,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use super::{
    auth::OpenBrowser,
    outcome::{Exit, Failure, Warning},
    session::{self, Context},
    ui::{self, Ui},
};
use crate::{
    client::{Continuation, Elicitor},
    elicit::{self, Action, Request},
    redact::Known,
    text::code,
};

/// The most rounds of one request. A server that asks again and again
/// must not keep `ncp` for ever.
const MAX_ROUNDS: usize = 8;

/// True when a person can answer: stdin and stderr are a TTY.
#[must_use]
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// The end of stdin: the person cannot answer.
    End,
    Interrupted,
    TimedOut,
}

/// The answer to the last question of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Yes,
    No,
    Cancel,
}

/// The answer to the last question of a form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormDecision {
    Send,
    Change,
    Decline,
    Cancel,
}

/// How `ncp` asks: the output, the limit of each answer, and Ctrl-C.
#[derive(Clone)]
struct Asker {
    ui: Ui,
    server: String,
    open_browser: OpenBrowser,
    cancel: CancellationToken,
    limit: Duration,
    /// The warnings that showed. An elicitation can run 8 rounds and give
    /// the same warning in each round, so each warning shows one time
    /// (design rule 20).
    warned: Arc<Mutex<Vec<Warning>>>,
    /// The secret values of the server, which a warning hides: a question
    /// of the server can echo one.
    known: Known,
}

// Text of the server in a block, with escapes. Each line after the first gets
// an indent, so that no line can look like a question or a note of `ncp`.
fn block(text: &str) -> String {
    ui::printable(&ui::indented(text)).into_owned()
}

// Text of the server on one line: a label, a title, a default, a URL.
fn single(text: &str) -> String {
    ui::printable(&ui::one_line(text)).into_owned()
}

impl Asker {
    fn new(context: &Context<'_>, server: &str, known: Known) -> Self {
        Self {
            ui: context.ui,
            server: server.to_string(),
            open_browser: context.open_browser,
            cancel: context.cancel.clone(),
            limit: context.request_limit,
            warned: Arc::default(),
            known,
        }
    }

    fn warn(&self, warning: Warning) {
        let warning = warning
            .about(self.server.as_str())
            .map_text(|text| self.known.hide(&text));
        let new = self.warned.lock().map_or(true, |mut warned| {
            let new = !warned.contains(&warning);
            if new {
                warned.push(warning.clone());
            }
            new
        });
        if new {
            self.ui.warn(&warning);
        }
    }

    fn say(&self, text: &str) {
        self.ui.diagnostic(text);
    }

    // One line from stdin, read on its own thread: a blocking read on the
    // runtime stops the time limits and delays the exit.
    async fn line(&self) -> Result<String, Stop> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let read = std::io::stdin().read_line(&mut text);
            let _ = sender.send(read.ok().filter(|count| *count > 0).map(|_| text));
        });
        tokio::select! {
            read = receiver => read.ok().flatten().ok_or(Stop::End),
            () = self.cancel.cancelled() => Err(Stop::Interrupted),
            () = tokio::time::sleep(self.limit) => Err(Stop::TimedOut),
        }
    }

    // A yes or no question, for example to open a page. The answer is a
    // letter or the whole word.
    async fn decision(&self, question: &str) -> Result<Decision, Stop> {
        loop {
            self.say(&format!("{question} [y]es, [n]o, [c]ancel: "));
            match self.line().await?.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(Decision::Yes),
                "n" | "no" => return Ok(Decision::No),
                "c" | "cancel" => return Ok(Decision::Cancel),
                _ => self.say("Type y, n or c.\n"),
            }
        }
    }

    // The last question of a form. The answer is a letter or the whole
    // word, so that "change" cannot cancel. `y` also sends and `n` also
    // declines, as a person can type yes or no.
    async fn form_decision(&self, server: &str) -> Result<FormDecision, Stop> {
        loop {
            self.say(&format!(
                "Send these values to {server}? [s]end, c[h]ange a field, [d]ecline, [c]ancel: "
            ));
            match self.line().await?.trim().to_ascii_lowercase().as_str() {
                "s" | "send" | "y" | "yes" => return Ok(FormDecision::Send),
                "h" | "change" => return Ok(FormDecision::Change),
                "d" | "decline" | "n" | "no" => return Ok(FormDecision::Decline),
                "c" | "cancel" => return Ok(FormDecision::Cancel),
                _ => self.say("Type s, h, d or c.\n"),
            }
        }
    }

    // One field, until the answer passes the checks of the schema. `None`
    // for an optional field with no answer.
    async fn field(&self, field: &elicit::Field) -> Result<Option<Value>, Stop> {
        if let Some(description) = &field.description {
            self.say(&format!("  {}\n", block(description)));
        }
        loop {
            self.say(&format!("  {}: ", single(&elicit::hint(field))));
            match elicit::answer(field, &self.line().await?) {
                Ok(value) => return Ok(value),
                // The problem can hold text of the server.
                Err(problem) => self.say(&format!("  {}\n", block(&problem))),
            }
        }
    }

    async fn form(&self, fields: &[elicit::Field]) -> Result<Map<String, Value>, Stop> {
        let mut content = Map::new();
        for field in fields {
            if let Some(value) = self.field(field).await? {
                content.insert(field.name.clone(), value);
            }
        }
        Ok(content)
    }

    // Asks which field to change, then asks that field again.
    async fn change(
        &self,
        fields: &[elicit::Field],
        content: &mut Map<String, Value>,
    ) -> Result<(), Stop> {
        for (number, field) in fields.iter().enumerate() {
            let value = content
                .get(&field.name)
                .map_or_else(|| "(no value)".to_string(), Value::to_string);
            self.say(&format!(
                "  {}. {}: {}\n",
                number + 1,
                single(&field.name),
                single(&value)
            ));
        }
        let chosen = loop {
            self.say(&format!("Field to change (1-{}): ", fields.len()));
            let typed = self.line().await?;
            match typed.trim().parse::<usize>() {
                Ok(number) if (1..=fields.len()).contains(&number) => break &fields[number - 1],
                _ => self.say(&format!("Type a number from 1 to {}.\n", fields.len())),
            }
        };
        match self.field(chosen).await? {
            Some(value) => content.insert(chosen.name.clone(), value),
            None => content.remove(&chosen.name),
        };
        Ok(())
    }

    async fn ask(&self, params: &Value) -> Result<Action, Stop> {
        let server = &self.server;
        let request = match elicit::parse(params) {
            Ok(request) => request,
            Err(error) => {
                self.warn(
                    Warning::new(format!(
                        "The server {} sent a question that ncp cannot show. ncp declines.",
                        code(server)
                    ))
                    .caused_by(&error),
                );
                return Ok(Action::Decline);
            }
        };
        match request {
            Request::Form { message, fields } => {
                self.say(&format!("The server {server} asks: {}\n", block(&message)));
                let mut content = self.form(&fields).await?;
                loop {
                    match self.form_decision(server).await? {
                        FormDecision::Send => return Ok(Action::Accept(content)),
                        FormDecision::Change if !fields.is_empty() => {
                            self.change(&fields, &mut content).await?;
                        }
                        FormDecision::Change => self.say("The form has no field.\n"),
                        FormDecision::Decline => return Ok(Action::Decline),
                        FormDecision::Cancel => return Ok(Action::Cancel),
                    }
                }
            }
            Request::Url { message, url } => {
                let url = match elicit::safe_url(&url) {
                    Ok(url) => url,
                    Err(reason) => {
                        self.warn(
                            Warning::new(format!(
                                "The server {} asks ncp to open {}. ncp declines.",
                                code(server),
                                code(&url)
                            ))
                            .cause(reason),
                        );
                        return Ok(Action::Decline);
                    }
                };
                let host = url.host_str().unwrap_or_default();
                if has_punycode_label(host) {
                    self.warn(Warning::new(format!(
                        "The page of the server {} is on the host {}. This host has a Punycode label (`xn--`), so its name can look like another name.",
                        code(server),
                        code(host)
                    )));
                }
                self.say(&format!(
                    "The server {server} asks you to open a page of {host}: {}\n  {}\n",
                    block(&message),
                    url.as_str()
                ));
                Ok(match self.decision("Open it in the browser?").await? {
                    Decision::Yes => {
                        // The person said yes and has the URL on the screen,
                        // so the answer stays yes when no browser opens.
                        if let Err(error) = (self.open_browser)(url.as_str()) {
                            self.warn(
                                Warning::new(format!(
                                    "ncp cannot open a browser for the page of the server {}",
                                    code(server)
                                ))
                                .caused_by(&error)
                                .hint("Open the URL above in a browser"),
                            );
                        }
                        Action::Opened
                    }
                    Decision::No => Action::Decline,
                    Decision::Cancel => Action::Cancel,
                })
            }
        }
    }
}

/// True when a label of the host is Punycode (RFC 3492): such a name can
/// look like another name in Unicode.
pub(super) fn has_punycode_label(host: &str) -> bool {
    host.split('.')
        .any(|label| label.to_ascii_lowercase().starts_with("xn--"))
}

impl Asker {
    // Asks to open each page that the server needs, as a URL-mode question.
    // `ncp` does not run the request again: the server can need time, and the
    // person knows when the work on the page is complete. The hint of the
    // failure says so, also for a page that the person opens later.
    async fn open_pages(&self, failure: Failure) -> Failure {
        for page in failure.pages() {
            let params =
                serde_json::json!({"mode": "url", "message": page.message, "url": page.url});
            match self.ask(&params).await {
                Ok(_) | Err(Stop::End | Stop::TimedOut) => {}
                Err(Stop::Interrupted) => return session::interrupted(Some(&self.server)),
            }
        }
        failure
    }
}

/// The elicitor of a connection for a legacy server, which asks with a
/// request of its own. No answer (the end of stdin, the time limit or
/// Ctrl-C) is a cancel.
#[must_use]
pub fn elicitor(context: &Context<'_>, server: &str, known: Known) -> Elicitor {
    let asker = Asker::new(context, server, known);
    std::sync::Arc::new(move |params| {
        let asker = asker.clone();
        Box::pin(async move {
            let action = asker.ask(&params).await.unwrap_or(Action::Cancel);
            elicit::result(&action)
        })
    })
}

// The requests of an `input_required` result as JSON, when `ncp` can
// answer each of them: only elicitation. `None` for a request of another
// kind, for example sampling.
fn elicitations(result: &rmcp::model::InputRequiredResult) -> Option<Vec<(String, Value)>> {
    let requests = serde_json::to_value(&result.input_requests).ok()?;
    requests
        .as_object()?
        .iter()
        .map(|(key, request)| {
            (request.get("method").and_then(Value::as_str) == Some("elicitation/create")).then(
                || {
                    (
                        key.clone(),
                        request.get("params").cloned().unwrap_or_default(),
                    )
                },
            )
        })
        .collect()
}

/// A request that can come back with `input_required`.
pub trait Rounds {
    fn input_required(&self) -> Option<&rmcp::model::InputRequiredResult>;
}

impl Rounds for rmcp::model::CallToolResponse {
    fn input_required(&self) -> Option<&rmcp::model::InputRequiredResult> {
        match self {
            Self::InputRequired(result) => Some(result),
            _ => None,
        }
    }
}

impl<T> Rounds for crate::client::Answer<T> {
    fn input_required(&self) -> Option<&rmcp::model::InputRequiredResult> {
        match self {
            Self::InputRequired(result) => Some(result),
            Self::Complete(_) => None,
        }
    }
}

/// A yes or no question with No as the default, as `mise trust` asks.
/// `None`: no answer (the end of stdin or the time limit).
///
/// # Errors
///
/// Returns exit 130 at Ctrl-C.
pub async fn confirm(context: &Context<'_>, question: &str) -> Result<Option<bool>, Failure> {
    let asker = Asker::new(context, "", Known::default());
    asker.say(question);
    match asker.line().await {
        Ok(line) => Ok(Some(matches!(
            line.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))),
        Err(Stop::Interrupted) => Err(session::interrupted(None)),
        Err(Stop::End | Stop::TimedOut) => Ok(None),
    }
}

fn unanswered(stop: Stop, context: &Context<'_>, server: &str) -> Failure {
    match stop {
        Stop::Interrupted => session::interrupted(Some(server)),
        Stop::TimedOut | Stop::End => Failure::new(
            Exit::InputRequired,
            format!(
                "The question of the server {} got no answer in {} s",
                code(server),
                context.request_limit.as_secs()
            ),
        )
        .about(server)
        .hint(session::wait_longer()),
    }
}

/// The server can ask for input that a person at the TTY can give. Then
/// `ncp` asks, and runs the request again with the answers (MCP
/// 2026-07-28, multi-round requests). In other cases the `input_required` result goes
/// back to the caller, which writes it with exit 5. These cases are: no
/// TTY, a request that `ncp` cannot answer (sampling, roots), and the end
/// of the last round.
///
/// # Errors
///
/// Returns the failure of a round, exit 130 at Ctrl-C, and exit 5 when a
/// question gets no answer in the time limit.
pub async fn with_rounds<R: Rounds>(
    context: &Context<'_>,
    (server, known): (&str, &Known),
    run: impl AsyncFn(Option<&Continuation>) -> Result<R, Failure>,
) -> Result<R, Failure> {
    let asker = Asker::new(context, server, known.clone());
    let mut continuation = None;
    let mut round = 1;
    loop {
        let response = match run(continuation.as_ref()).await {
            Err(failure) if context.interactive && !failure.pages().is_empty() => {
                return Err(asker.open_pages(failure).await);
            }
            other => other?,
        };
        let questions = match response.input_required() {
            Some(result) if context.interactive && round < MAX_ROUNDS => {
                elicitations(result).map(|questions| (questions, result.request_state.clone()))
            }
            _ => None,
        };
        let Some((questions, state)) = questions else {
            return Ok(response);
        };
        let mut responses = rmcp::model::InputResponses::new();
        for (key, params) in questions {
            let action = match asker.ask(&params).await {
                Ok(action) => action,
                // The end of stdin: the person cannot answer, which is a
                // cancel of this question.
                Err(Stop::End) => Action::Cancel,
                Err(stop) => return Err(unanswered(stop, context, server)),
            };
            responses.insert(key, elicit::result(&action));
        }
        continuation = Some(Continuation { responses, state });
        round += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_punycode_label_in_any_place_of_the_host() {
        assert!(has_punycode_label("xn--pple-43d.com"));
        assert!(has_punycode_label("login.XN--80ak6aa92e.com"));
        assert!(!has_punycode_label("login.example.com"));
        assert!(!has_punycode_label("axn--b.com"));
    }
}
