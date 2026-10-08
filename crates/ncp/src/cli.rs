//! The command line: the imperative shell around the client core.

pub mod args;
mod commands;
mod outcome;
mod render;
mod schema_check;
mod session;
mod tool_args;
mod ui;

use std::{process::ExitCode, time::Duration};

use clap::{Parser, error::ErrorKind};
use tokio_util::sync::CancellationToken;

use self::{
    args::{Cli, Command},
    outcome::{Exit, Failure},
    session::Context,
    ui::{Format, Ui},
};
use crate::{expand::Env, paths::Paths};

/// The time that the runtime waits for its tasks at the exit. A native store
/// call that passed its limit can stay on its blocking thread.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(2);

/// Runs `ncp` and gives its exit code.
#[must_use]
pub fn main() -> ExitCode {
    let ui = Ui {
        format: Format::detect(),
    };
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(ui, &error),
    };
    let env = process_env();
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let failure = Failure::protocol(format!("cannot start the async runtime: {error}"));
            ui.error(&failure);
            return failure.exit.into();
        }
    };
    let exit = runtime.block_on(run(cli, &env, ui));
    shut_down(runtime);
    exit.into()
}

/// The async runtime of `ncp`: one thread, because each command does one
/// thing at a time. Store calls run on blocking threads.
///
/// # Errors
///
/// Returns an error when the runtime cannot start.
pub fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// Stops the runtime. A store call that passed its limit can stay on its
/// blocking thread, so the wait for it has a limit.
pub fn shut_down(runtime: tokio::runtime::Runtime) {
    runtime.shutdown_timeout(SHUTDOWN_LIMIT);
}

/// The environment of the process. A variable that is not UTF-8 cannot be
/// a `${NAME}` value, and is left out.
fn process_env() -> Env {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

async fn run(cli: Cli, env: &Env, ui: Ui) -> Exit {
    let result = async {
        let paths = Paths::from_env(env).map_err(|error| Failure::protocol(error.to_string()))?;
        let context = Context {
            env,
            ui,
            paths,
            request_limit: Duration::from_secs(cli.options.timeout),
            cancel: interrupt_token(),
        };
        match cli.command {
            Command::Server(command) => commands::server(&context, command).await,
            Command::Tool(command) => commands::tool(&context, command).await,
        }
    }
    .await;
    match result {
        Ok(()) => Exit::Success,
        Err(failure) => {
            ui.error(&failure);
            failure.exit
        }
    }
}

/// A token that fires on Ctrl-C. The terminal sends SIGINT to `ncp` only,
/// because each stdio server runs in its own process group. Thus `ncp` can
/// send the cancel request and stop the server itself (the spec,
/// basic/utilities/cancellation).
fn interrupt_token() -> CancellationToken {
    let token = CancellationToken::new();
    let fire = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            fire.cancel();
        }
    });
    token
}

/// Help and version go to stdout with exit 0. A parse error is a usage
/// error: clap text on a TTY, the JSON error object in a pipe.
fn parse_failure(ui: Ui, error: &clap::Error) -> ExitCode {
    match (error.kind(), ui.format) {
        (ErrorKind::DisplayHelp | ErrorKind::DisplayVersion, _) | (_, Format::Text) => {
            let _ = error.print();
            if error.use_stderr() {
                Exit::Usage.into()
            } else {
                Exit::Success.into()
            }
        }
        (_, Format::Json) => {
            let message = match error.kind() {
                // clap gives the help text as the error for these kinds.
                ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ErrorKind::MissingSubcommand => "missing command".to_string(),
                _ => error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("error: ")
                    .to_string(),
            };
            let failure = Failure::usage(message).hint("ncp --help");
            ui.error(&failure);
            failure.exit.into()
        }
    }
}
