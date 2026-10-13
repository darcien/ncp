//! `--timings`: the time of each step of one run, on stderr.
//!
//! The record shows CPU time and network time apart, so that a slow server
//! and a slow `ncp` look different. Each step that did not occur in the run
//! is absent.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde_json::{Map, Value, json};

use super::ui::{Format, json_line};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub config: Option<Duration>,
    pub credentials: Option<Duration>,
    pub server_start: Option<Duration>,
    pub dns: Option<Duration>,
    pub connect: Option<Duration>,
    pub responses: Vec<Duration>,
}

/// Adds the time to a step that can occur more than once, for example a
/// second connect after a probe timeout.
pub fn add(step: &mut Option<Duration>, elapsed: Duration) {
    *step = Some(step.unwrap_or_default() + elapsed);
}

/// The record of the current run. The client core adds to it from its
/// tasks, so it has a lock.
#[derive(Debug)]
pub struct Timings {
    started: Instant,
    record: Mutex<Record>,
}

impl Timings {
    #[must_use]
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            record: Mutex::default(),
        }
    }

    pub fn update(&self, change: impl FnOnce(&mut Record)) {
        if let Ok(mut record) = self.record.lock() {
            change(&mut record);
        }
    }

    #[must_use]
    pub fn report(&self, format: Format) -> String {
        let record = self.record.lock().map(|r| r.clone()).unwrap_or_default();
        render(format, &record, self.started.elapsed(), cpu_time())
    }
}

/// The CPU time of `ncp` (user and system), all threads. `None` when the
/// system cannot give it.
fn cpu_time() -> Option<Duration> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).ok()?;
    let time = |value: nix::sys::time::TimeVal| {
        let seconds = u64::try_from(value.tv_sec()).unwrap_or_default();
        let micros = u64::try_from(value.tv_usec()).unwrap_or_default();
        Duration::from_secs(seconds) + Duration::from_micros(micros)
    };
    Some(time(usage.user_time()) + time(usage.system_time()))
}

/// reqwest looks up the name inside its connector. Thus the connect time
/// includes each lookup, and this function takes the DNS time out.
fn connect_without_dns(record: &Record) -> Option<Duration> {
    record
        .connect
        .map(|connect| connect.saturating_sub(record.dns.unwrap_or_default()))
}

/// Milliseconds with one decimal: a local step takes less than 1 ms.
pub fn ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

/// The steps in their order, with the JSON name and the text name.
fn steps(
    record: &Record,
    total: Duration,
    cpu: Option<Duration>,
) -> Vec<(&'static str, &'static str, Duration)> {
    [
        ("totalMs", "total", Some(total)),
        ("cpuMs", "cpu", cpu),
        ("configMs", "config", record.config),
        ("credentialsMs", "credentials", record.credentials),
        ("serverStartMs", "server start", record.server_start),
        ("dnsMs", "dns", record.dns),
        ("connectMs", "connect", connect_without_dns(record)),
    ]
    .into_iter()
    .filter_map(|(json, text, value)| value.map(|value| (json, text, value)))
    .collect()
}

/// Text: `timings: total 41.2 ms, cpu 8.0 ms, ..., responses 18.0 ms 9.5 ms`.
/// JSON: `{"timings": {"totalMs": 41.2, ..., "responseMs": [18.0, 9.5]}}`.
#[must_use]
pub fn render(format: Format, record: &Record, total: Duration, cpu: Option<Duration>) -> String {
    let steps = steps(record, total, cpu);
    match format {
        Format::Text => {
            let parts = steps
                .iter()
                .map(|(_, name, value)| format!("{name} {:.1} ms", ms(*value)))
                .chain((!record.responses.is_empty()).then(|| {
                    let times: Vec<String> = record
                        .responses
                        .iter()
                        .map(|value| format!("{:.1} ms", ms(*value)))
                        .collect();
                    format!("responses {}", times.join(" "))
                }))
                .collect::<Vec<_>>();
            format!("timings: {}\n", parts.join(", "))
        }
        Format::Json => {
            let mut fields: Map<String, Value> = steps
                .iter()
                .map(|(name, _, value)| ((*name).to_string(), json!(ms(*value))))
                .collect();
            if !record.responses.is_empty() {
                let times: Vec<f64> = record.responses.iter().map(|value| ms(*value)).collect();
                fields.insert("responseMs".into(), json!(times));
            }
            json_line(&json!({ "timings": fields }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        Record {
            config: Some(Duration::from_micros(420)),
            credentials: None,
            server_start: None,
            dns: Some(Duration::from_millis(2)),
            connect: Some(Duration::from_micros(4_150)),
            responses: vec![Duration::from_millis(18), Duration::from_micros(9_540)],
        }
    }

    #[test]
    fn text_timings_list_the_steps_that_occurred_in_order() {
        let text = render(
            Format::Text,
            &record(),
            Duration::from_millis(41),
            Some(Duration::from_millis(8)),
        );
        assert_eq!(
            text,
            "timings: total 41.0 ms, cpu 8.0 ms, config 0.4 ms, dns 2.0 ms, connect 2.2 ms, \
             responses 18.0 ms 9.5 ms\n"
        );
    }

    #[test]
    fn json_timings_use_the_design_names_and_leave_out_absent_steps() {
        let line = render(Format::Json, &record(), Duration::from_millis(41), None);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            value,
            json!({"timings": {"totalMs": 41.0, "configMs": 0.4, "dnsMs": 2.0,
                "connectMs": 2.2, "responseMs": [18.0, 9.5]}})
        );
    }

    #[test]
    fn repeated_steps_add_up() {
        let mut step = None;
        add(&mut step, Duration::from_millis(1));
        add(&mut step, Duration::from_millis(2));
        assert_eq!(step, Some(Duration::from_millis(3)));
    }
}
