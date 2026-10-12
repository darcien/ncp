//! `-vv` traces and `--timings` from the events of a connection.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::{
    timings::{self, Timings, ms},
    ui::{Format, Ui, Verbosity, json_line},
};
use crate::client::observe::{Direction, Event, HttpExchange, Observer};

/// The `-vv` line of an event, or `None` for an event that is only a
/// time. `shown_url` hides the secrets of a URL.
#[must_use]
pub fn trace_line(
    format: Format,
    server: &str,
    event: &Event<'_>,
    shown_url: &dyn Fn(&str) -> String,
) -> Option<String> {
    match event {
        Event::Message { direction, message } => {
            let direction = match direction {
                Direction::Sent => "sent",
                Direction::Received => "received",
            };
            Some(match format {
                Format::Text => format!("trace: server {server}: {direction} {message}\n"),
                Format::Json => {
                    json_line(&json!({"trace": {"server": server, direction: message}}))
                }
            })
        }
        Event::Http(exchange) => Some(http_line(format, server, exchange, shown_url)),
        _ => None,
    }
}

fn http_line(
    format: Format,
    server: &str,
    exchange: &HttpExchange,
    shown_url: &dyn Fn(&str) -> String,
) -> String {
    let url = shown_url(&exchange.url);
    let outcome = hide_url_in(&exchange.outcome, &exchange.url, &url);
    // A status line holds no value; an error can hold a URL.
    let outcome = if outcome.starts_with("error") {
        shown_url(&outcome)
    } else {
        outcome
    };
    // OAuth bodies can hold the server URL, for example in `resource`.
    let request_body = exchange.request_body.as_deref().map(shown_url);
    let response_body = exchange.response_body.as_deref().map(shown_url);
    match format {
        Format::Text => {
            let headers: Vec<String> = exchange
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect();
            let body = |label: &str, body: &Option<String>| {
                body.as_ref()
                    .map(|body| format!("trace: server {server}: {label} body {body}\n"))
                    .unwrap_or_default()
            };
            format!(
                "trace: server {server}: {} {url} [{}]: {} in {} ms\n{}{}",
                exchange.method,
                headers.join(", "),
                outcome,
                ms(exchange.elapsed),
                body("request", &request_body),
                body("response", &response_body),
            )
        }
        Format::Json => {
            let headers: Map<String, Value> = exchange
                .headers
                .iter()
                .map(|(name, value)| (name.clone(), json!(value)))
                .collect();
            let mut http = json!({
                "method": exchange.method,
                "url": url,
                "headers": headers,
                "outcome": outcome,
                "ms": ms(exchange.elapsed),
            });
            if let Some(body) = &request_body {
                http["requestBody"] = json!(body);
            }
            if let Some(body) = &response_body {
                http["responseBody"] = json!(body);
            }
            json_line(&json!({"trace": {"server": server, "http": http}}))
        }
    }
}

/// The text with the URL as the request used it replaced by the URL as it
/// shows. An HTTP error names the URL that it got, and that URL can hold a
/// `${NAME}` value or a key.
fn hide_url_in(text: &str, url: &str, shown: &str) -> String {
    let parsed = url::Url::parse(url).map(|url| url.to_string());
    let text = text.replace(url, shown);
    match parsed {
        Ok(parsed) if parsed != url => text.replace(&parsed, shown),
        _ => text,
    }
}

fn record(timings: &Timings, event: &Event<'_>) {
    match event {
        Event::Response { elapsed, .. } => {
            timings.update(|record| record.responses.push(*elapsed));
        }
        Event::Dns { elapsed, .. } => {
            timings.update(|record| timings::add(&mut record.dns, *elapsed));
        }
        Event::Connect { elapsed } => {
            timings.update(|record| timings::add(&mut record.connect, *elapsed));
        }
        Event::ServerStart { elapsed } => {
            timings.update(|record| timings::add(&mut record.server_start, *elapsed));
        }
        Event::Message { .. } | Event::Http(_) => {}
    }
}

/// The observer for one server: traces with `-vv`, times with
/// `--timings`, and no observer when neither is on.
#[must_use]
pub fn observer(
    ui: Ui,
    server: &str,
    timings: Option<Arc<Timings>>,
    shown_url: impl Fn(&str) -> String + Send + Sync + 'static,
) -> Observer {
    let trace = ui.verbosity >= Verbosity::Trace;
    if !trace && timings.is_none() {
        return Observer::default();
    }
    let server = server.to_string();
    Observer::new(move |event| {
        if trace && let Some(line) = trace_line(ui.format, &server, event, &shown_url) {
            ui.diagnostic(&line);
        }
        if let Some(timings) = &timings {
            record(timings, event);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// As the closure of a session: a URL with its secrets hidden, and
    /// other text as it is.
    fn hidden(text: &str) -> String {
        if text.starts_with("http") && !text.contains(' ') {
            crate::redact::url(text)
        } else {
            text.to_string()
        }
    }

    #[test]
    fn message_traces_carry_the_direction_and_the_message() {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        let event = Event::Message {
            direction: Direction::Sent,
            message: &message,
        };
        assert_eq!(
            trace_line(Format::Text, "gh", &event, &hidden).unwrap(),
            "trace: server gh: sent {\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"tools/list\"}\n"
        );
        let line = trace_line(Format::Json, "gh", &event, &hidden).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value, json!({"trace": {"server": "gh", "sent": message}}));
    }

    #[test]
    fn http_traces_hide_the_secrets_of_the_url() {
        let exchange = HttpExchange {
            method: "POST",
            url: "https://u:p@h.test/cb?code=k&q=1".into(),
            headers: vec![("authorization".into(), "****".into())],
            outcome: "JSON response".into(),
            elapsed: Duration::from_micros(3_140),
            request_body: None,
            response_body: None,
        };
        let line = trace_line(Format::Text, "gh", &Event::Http(&exchange), &hidden).unwrap();
        assert_eq!(
            line,
            "trace: server gh: POST https://****@h.test/cb?code=****&q=1 \
             [authorization: ****]: JSON response in 3.1 ms\n"
        );
        let line = trace_line(Format::Json, "gh", &Event::Http(&exchange), &hidden).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value["trace"]["http"]["url"],
            "https://****@h.test/cb?code=****&q=1"
        );
        assert_eq!(value["trace"]["http"]["headers"]["authorization"], "****");
    }

    #[test]
    fn http_errors_show_the_url_with_its_secrets_hidden() {
        let exchange = HttpExchange {
            method: "POST",
            url: "http://h.test/cb?code=k3y".into(),
            headers: Vec::new(),
            outcome: "error: error sending request for url (http://h.test/cb?code=k3y)".into(),
            elapsed: Duration::from_millis(1),
            request_body: None,
            response_body: None,
        };
        let line = trace_line(Format::Text, "gh", &Event::Http(&exchange), &hidden).unwrap();
        assert!(!line.contains("k3y"), "{line}");
    }

    #[test]
    fn time_events_make_no_trace() {
        let event = Event::Connect {
            elapsed: Duration::from_millis(1),
        };
        assert!(trace_line(Format::Text, "gh", &event, &hidden).is_none());
    }
}
