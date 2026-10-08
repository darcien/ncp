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
                exchange.outcome,
                ms(exchange.elapsed),
                body("request", &exchange.request_body),
                body("response", &exchange.response_body),
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
                "outcome": exchange.outcome,
                "ms": ms(exchange.elapsed),
            });
            if let Some(body) = &exchange.request_body {
                http["requestBody"] = json!(body);
            }
            if let Some(body) = &exchange.response_body {
                http["responseBody"] = json!(body);
            }
            json_line(&json!({"trace": {"server": server, "http": http}}))
        }
    }
}

/// Adds the time of the event to the timings.
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

    fn hidden(url: &str) -> String {
        crate::redact::url(url)
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
            url: "https://u:p@h.test/mcp?api_key=k".into(),
            headers: vec![("authorization".into(), "****".into())],
            outcome: "JSON response".into(),
            elapsed: Duration::from_micros(3_140),
            request_body: None,
            response_body: None,
        };
        let line = trace_line(Format::Text, "gh", &Event::Http(&exchange), &hidden).unwrap();
        assert_eq!(
            line,
            "trace: server gh: POST https://****@h.test/mcp?api_key=**** \
             [authorization: ****]: JSON response in 3.1 ms\n"
        );
        let line = trace_line(Format::Json, "gh", &Event::Http(&exchange), &hidden).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value["trace"]["http"]["url"],
            "https://****@h.test/mcp?api_key=****"
        );
        assert_eq!(value["trace"]["http"]["headers"]["authorization"], "****");
    }

    #[test]
    fn time_events_make_no_trace() {
        let event = Event::Connect {
            elapsed: Duration::from_millis(1),
        };
        assert!(trace_line(Format::Text, "gh", &event, &hidden).is_none());
    }
}
