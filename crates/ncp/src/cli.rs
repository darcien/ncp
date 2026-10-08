//! The command line: the imperative shell around the client core.

pub mod args;
pub mod auth;
mod commands;
mod completion;
mod diagnostics;
mod near;
mod outcome;
mod render;
mod schema_check;
pub mod secrets;
mod session;
mod timings;
mod tool_args;
mod ui;

use std::{
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::{Parser, error::ErrorKind};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

use self::{
    args::{Cli, ColorChoice, Command, OutputFormat},
    outcome::{Exit, Failure},
    session::Context,
    timings::Timings,
    ui::{Format, Ui, Verbosity},
};
use crate::{expand::Env, paths::Paths};

/// The time that the runtime waits for its tasks at the exit. A native store
/// call that passed its limit can stay on its blocking thread.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(2);

/// The parts of `ncp` that reach outside the process with no network: the
/// native store and the browser. Tests give fakes, so that a test never
/// uses the real Keychain, the real Secret Service, or a real browser.
#[derive(Clone, Copy)]
pub struct Platform {
    pub open_native: secrets::OpenNative,
    pub open_browser: auth::OpenBrowser,
}

/// Runs `ncp` and gives its exit code.
#[must_use]
pub fn main() -> ExitCode {
    main_with(Platform {
        open_native: secrets::open_native,
        open_browser: auth::open_browser,
    })
}

/// Runs `ncp` with another platform.
#[must_use]
pub fn main_with(platform: Platform) -> ExitCode {
    let started = Instant::now();
    // The completion scripts run this at each tab. It is not part of the
    // grammar, so that the scripts do not offer it, and it is fast and
    // silent, also with no HOME or a broken configuration.
    if std::env::args_os().nth(1).as_deref() == Some(completion::SERVERS_COMMAND.as_ref()) {
        completion::servers(&process_env());
        return ExitCode::SUCCESS;
    }
    let ui = Ui {
        format: Format::detect(),
        verbosity: Verbosity::Normal,
        color: false,
    };
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(ui, &error),
    };
    let env = process_env();
    let ui = Ui {
        format: match cli.options.output_format {
            Some(OutputFormat::Text) => Format::Text,
            Some(OutputFormat::Json) => Format::Json,
            None => ui.format,
        },
        verbosity: Verbosity::from_flags(cli.options.quiet, cli.options.verbose),
        color: ui::color_on(
            match cli.options.color {
                ColorChoice::Auto => None,
                ColorChoice::Always => Some(true),
                ColorChoice::Never => Some(false),
            },
            std::io::IsTerminal::is_terminal(&std::io::stderr()),
            &env,
        ),
    };
    let timings = cli.options.timings.then(|| Arc::new(Timings::new(started)));
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let failure = Failure::protocol(format!("cannot start the async runtime: {error}"));
            ui.error(&failure);
            return failure.exit.into();
        }
    };
    let exit = runtime.block_on(run(cli, &env, ui, timings.clone(), platform));
    shut_down(runtime);
    if let Some(timings) = timings {
        ui.diagnostic(&timings.report(ui.format));
    }
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

async fn run(
    cli: Cli,
    env: &Env,
    ui: Ui,
    timings: Option<Arc<Timings>>,
    platform: Platform,
) -> Exit {
    let result = async {
        let paths = Paths::from_env(env).map_err(|error| Failure::protocol(error.to_string()))?;
        let paths = match &cli.options.config_file {
            Some(path) => Paths {
                config_file: named_config_file(path)?,
                ..paths
            },
            None => paths,
        };
        let context = Context {
            env,
            ui,
            paths,
            request_limit: Duration::from_secs(cli.options.timeout),
            cancel: interrupt_token(),
            open_native: platform.open_native,
            open_browser: platform.open_browser,
            timings,
        };
        match cli.command {
            Command::Server(command) => commands::server(&context, command).await,
            Command::Tool(command) => commands::tool(&context, command).await,
            Command::Resource(command) => commands::resource(&context, command).await,
            Command::Prompt(command) => commands::prompt(&context, command).await,
            Command::Auth(command) => auth::command(&context, command).await,
            Command::Secret(command) => secrets::command(&context, command).await,
            Command::GenerateShellCompletion { shell } => completion::script(shell),
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

/// The configuration file that `--config-file` names. Unlike the default
/// file, it must exist: a typing error in the path must not look like an
/// empty configuration.
fn named_config_file(path: &std::path::Path) -> Result<std::path::PathBuf, Failure> {
    let path = std::path::absolute(path)
        .map_err(|error| Failure::usage(format!("{}: {error}", path.display())))?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(Failure::usage(format!(
            "the configuration file {} does not exist",
            path.display()
        )))
    }
}

/// A token that fires on Ctrl-C. Each stdio server runs in its own process
/// group, so the SIGINT of the terminal does not reach it, and `ncp` can
/// send the cancel request and stop the server itself (the spec,
/// basic/utilities/cancellation). A store program stays in the group of
/// `ncp`, so Ctrl-C stops it too.
fn interrupt_token() -> CancellationToken {
    let token = CancellationToken::new();
    // The handler is in place when this function returns, before the first
    // await. Thus a Ctrl-C at any time reaches the token, also during a
    // read of stdin. With no handler, the default action stops `ncp`.
    if let Ok(mut interrupt) = signal(SignalKind::interrupt()) {
        let fire = token.clone();
        tokio::spawn(async move {
            if interrupt.recv().await.is_some() {
                fire.cancel();
            }
        });
    }
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
