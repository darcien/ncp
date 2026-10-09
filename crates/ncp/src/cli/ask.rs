//! Questions to a person at a TTY: the elicitation of a server.
//!
//! `ncp` asks only when stdin and stderr are a TTY. Else a server gets no
//! elicitation capability, and an `input_required` result gives exit 5
//! (design section 3). The questions go to stderr, so that stdout keeps
//! only the result.

use std::io::IsTerminal;

use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    auth::OpenBrowser,
    outcome::{Exit, Failure},
    session::Context,
    ui::{self, Ui},
};
use crate::{
    client::{Continuation, Elicitor},
    elicit::{self, Action, Request},
};

/// The most rounds of one request. A server that asks again and again
/// must not keep `ncp` for ever.
const MAX_ROUNDS: usize = 8;

/// True when a person can answer: stdin and stderr are a TTY.
#[must_use]
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// One line from stdin, read on its own thread: a blocking read on the
/// runtime would stop the time limits and delay the exit. `None` at the
/// end of stdin or at Ctrl-C.
async fn line(cancel: &CancellationToken) -> Option<String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = std::io::stdin().read_line(&mut text);
        let _ = sender.send(read.ok().filter(|count| *count > 0).map(|_| text));
    });
    tokio::select! {
        read = receiver => read.ok().flatten(),
        () = cancel.cancelled() => None,
    }
}

fn say(ui: Ui, text: &str) {
    ui.diagnostic(&ui::printable(text));
}

/// Asks the final question: send, decline, or cancel.
async fn decision(ui: Ui, cancel: &CancellationToken, question: &str) -> Option<char> {
    loop {
        say(ui, &format!("{question} [y]es, [n]o, [c]ancel: "));
        let answer = line(cancel).await?;
        match answer.trim().to_ascii_lowercase().chars().next() {
            Some(choice @ ('y' | 'n' | 'c')) => return Some(choice),
            _ => say(ui, "Type y, n or c.\n"),
        }
    }
}

/// Asks each field of a form. `None` at the end of stdin or at Ctrl-C.
async fn form(
    ui: Ui,
    cancel: &CancellationToken,
    fields: &[elicit::Field],
) -> Option<Map<String, Value>> {
    let mut content = Map::new();
    for field in fields {
        if let Some(description) = &field.description {
            say(ui, &format!("  {description}\n"));
        }
        loop {
            say(ui, &format!("  {}: ", elicit::hint(field)));
            let typed = line(cancel).await?;
            match elicit::answer(field, &typed) {
                Ok(Some(value)) => {
                    content.insert(field.name.clone(), value);
                    break;
                }
                Ok(None) => break,
                Err(problem) => say(ui, &format!("  {problem}\n")),
            }
        }
    }
    Some(content)
}

/// Asks the person, and gives the `ElicitResult`. The text of the server
/// shows with the escapes of all text output.
async fn ask(
    ui: Ui,
    server: &str,
    open_browser: OpenBrowser,
    cancel: &CancellationToken,
    params: &Value,
) -> Value {
    let request = match elicit::parse(params) {
        Ok(request) => request,
        Err(error) => {
            ui.warning(Some(server), &format!("cannot ask: {error}; ncp declines"));
            return elicit::result(&Action::Decline);
        }
    };
    let action = match request {
        Request::Form { message, fields } => {
            say(ui, &format!("The server {server} asks: {message}\n"));
            match form(ui, cancel, &fields).await {
                None => Action::Cancel,
                Some(content) => {
                    match decision(ui, cancel, &format!("Send these values to {server}?")).await {
                        Some('y') => Action::Accept(content),
                        Some('n') => Action::Decline,
                        _ => Action::Cancel,
                    }
                }
            }
        }
        Request::Url { message, url } => {
            let host = elicit::host_of(&url).unwrap_or_else(|| "an unknown host".into());
            say(
                ui,
                &format!(
                    "The server {server} asks you to open a page of {host}: {message}\n  {url}\n"
                ),
            );
            match decision(ui, cancel, "Open it in the browser?").await {
                Some('y') => {
                    if let Err(error) = open_browser(&url) {
                        ui.warning(Some(server), &format!("cannot open a browser: {error}"));
                    }
                    Action::Accept(Map::new())
                }
                Some('n') => Action::Decline,
                _ => Action::Cancel,
            }
        }
    };
    let result = elicit::result(&action);
    // The URL mode accepts with no content.
    match (action, result) {
        (Action::Accept(content), mut result) if content.is_empty() => {
            if let Some(fields) = result.as_object_mut() {
                fields.remove("content");
            }
            result
        }
        (_, result) => result,
    }
}

/// The elicitor of a connection for a legacy server, which asks with a
/// request of its own.
#[must_use]
pub fn elicitor(
    ui: Ui,
    server: &str,
    open_browser: OpenBrowser,
    cancel: &CancellationToken,
) -> Elicitor {
    let (server, cancel) = (server.to_string(), cancel.clone());
    std::sync::Arc::new(move |params| {
        let (server, cancel) = (server.clone(), cancel.clone());
        Box::pin(async move { ask(ui, &server, open_browser, &cancel, &params).await })
    })
}

/// The answers to the requests of an `input_required` result (MCP
/// 2026-07-28, multi-round requests). Only elicitation can be answered.
///
/// # Errors
///
/// Returns exit 5 for a request that `ncp` cannot answer, for example
/// sampling.
async fn answer(
    context: &Context<'_>,
    server: &str,
    result: &rmcp::model::InputRequiredResult,
) -> Result<Continuation, Failure> {
    let requests = serde_json::to_value(&result.input_requests).unwrap_or(Value::Null);
    let mut responses = rmcp::model::InputResponses::new();
    for (key, request) in requests.as_object().into_iter().flatten() {
        let method = request.get("method").and_then(Value::as_str).unwrap_or("?");
        if method != "elicitation/create" {
            return Err(Failure::new(
                Exit::InputRequired,
                format!("the server asks for {method}, and ncp cannot give it"),
            )
            .server(server));
        }
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let answer = ask(
            context.ui,
            server,
            context.open_browser,
            &context.cancel,
            &params,
        )
        .await;
        responses.insert(key.clone(), answer);
    }
    Ok(Continuation {
        responses,
        state: result.request_state.clone(),
    })
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

/// Runs the request, and while the server asks for input that a person at
/// the TTY can give, asks and runs it again with the answers. With no TTY,
/// the `input_required` result goes back to the caller (exit 5).
///
/// # Errors
///
/// Returns the failure of a round, or exit 5 for input that `ncp` cannot
/// give.
pub async fn with_rounds<R: Rounds>(
    context: &Context<'_>,
    server: &str,
    run: impl AsyncFn(Option<&Continuation>) -> Result<R, Failure>,
) -> Result<R, Failure> {
    let mut continuation = None;
    for _ in 0..MAX_ROUNDS {
        let response = run(continuation.as_ref()).await?;
        match response.input_required() {
            Some(result) if context.interactive => {
                continuation = Some(answer(context, server, result).await?);
            }
            _ => return Ok(response),
        }
    }
    Err(Failure::new(
        Exit::InputRequired,
        format!("the server asked for input {MAX_ROUNDS} times; ncp stops"),
    )
    .server(server))
}
