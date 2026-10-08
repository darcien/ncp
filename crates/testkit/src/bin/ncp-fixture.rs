//! The stdio fixture server.
//!
//! Usage: `ncp-fixture <mode> [linger|ignore-term]`
//! - `linger`: stay alive after stdin closes, until SIGTERM.
//! - `ignore-term`: stay alive after stdin closes, and ignore SIGTERM.
//!
//! The two options test the stop steps of the client.
//!
//! When `NCP_FIXTURE_PID_FILE` is set, the server writes its process ID to
//! that file at start. A test of the command line uses it to check that
//! `ncp` stopped the server.

use std::process::ExitCode;

use ncp_testkit::{Fixture, Mode};
use rmcp::{ServiceExt, transport::stdio};
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mode) = args.first().and_then(|text| Mode::parse(text).ok()) else {
        eprintln!(
            "usage: ncp-fixture <modern|legacy|slow-MS|modern-only|slow-modern-only-MS> [linger|ignore-term]"
        );
        return ExitCode::from(2);
    };
    if let Some(path) = std::env::var_os("NCP_FIXTURE_PID_FILE")
        && std::fs::write(path, std::process::id().to_string()).is_err()
    {
        return ExitCode::FAILURE;
    }
    let option = args.get(1).map(String::as_str);
    // A SIGTERM handler replaces the default action, so install it only for
    // the options that test the stop steps.
    let mut terminate = match option {
        Some("linger" | "ignore-term") => match signal(SignalKind::terminate()) {
            Ok(terminate) => Some(terminate),
            Err(_) => return ExitCode::FAILURE,
        },
        _ => None,
    };
    if let Ok(service) = Fixture::new(mode).serve(stdio()).await {
        let _ = service.waiting().await;
    }
    match (option, terminate.as_mut()) {
        (Some("linger"), Some(terminate)) => {
            terminate.recv().await;
        }
        (Some("ignore-term"), Some(terminate)) => loop {
            terminate.recv().await;
        },
        _ => {}
    }
    ExitCode::SUCCESS
}
