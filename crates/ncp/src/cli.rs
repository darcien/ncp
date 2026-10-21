//! The command line: the imperative shell around the client core.

pub mod args;
pub mod ask;
pub mod auth;
mod commands;
mod completion;
mod diagnostics;
mod fixes;
mod near;
mod outcome;
mod project_file;
mod render;
mod schema_check;
pub mod secrets;
mod server_edit;
mod session;
mod timings;
mod tool_args;
mod trust;
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
    args::{Cli, Command, OutputFormat},
    outcome::{Exit, Failure, Hint},
    session::Context,
    timings::Timings,
    ui::{Format, Ui, Verbosity},
};
use crate::{
    expand::Env,
    paths::Paths,
    text::{code, path_code},
};

/// The time that the runtime waits for its tasks at the exit. A native store
/// call that passed its limit can stay on its blocking thread.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(2);

/// The parts of `ncp` that tests replace: the native store, the browser,
/// the TTY check, a hook before a rename, and the client metadata URL.
/// Tests give fakes, so that a test never uses the real Keychain, the real
/// Secret Service, or a real browser, and a test server can expect its own
/// client ID.
#[derive(Clone, Copy)]
pub struct Platform {
    pub open_native: secrets::OpenNative,
    pub open_browser: auth::OpenBrowser,
    pub interactive: fn() -> bool,
    pub before_rename: fn(&std::path::Path),
    /// The client ID of a login with a Client ID Metadata Document.
    pub client_metadata_url: &'static str,
}

#[must_use]
pub fn main() -> ExitCode {
    main_with(Platform {
        open_native: secrets::open_native,
        open_browser: auth::open_browser,
        interactive: ask::interactive,
        before_rename: |_| {},
        client_metadata_url: auth::CLIENT_METADATA_URL,
    })
}

#[must_use]
pub fn main_with(platform: Platform) -> ExitCode {
    let started = Instant::now();
    // The completion scripts run this at each tab. It is not part of the
    // grammar, so that the scripts do not offer it. It must be fast and
    // silent, also with no HOME or a broken configuration. `args` stops the
    // program on a word that is not UTF-8, so this uses `args_os`.
    let word = |index| {
        std::env::args_os()
            .nth(index)
            .and_then(|word| word.into_string().ok())
    };
    match word(1).as_deref() {
        Some(completion::SERVERS_COMMAND) => {
            completion::servers(&process_env());
            return ExitCode::SUCCESS;
        }
        Some(completion::TOOLS_COMMAND) => {
            completion::tools(&process_env(), &word(2).unwrap_or_default());
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let ui = Ui::new(Format::detect(), Verbosity::Normal);
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(ui, &error),
    };
    let env = process_env();
    let ui = Ui::new(
        match cli.options.output_format {
            Some(OutputFormat::Text) => Format::Text,
            Some(OutputFormat::Json) => Format::Json,
            None => ui.format,
        },
        Verbosity::from_flags(cli.options.quiet, cli.options.verbose),
    );
    let timings = cli.options.timings.then(|| Arc::new(Timings::new(started)));
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let failure = Failure::protocol("ncp cannot start the async runtime").caused_by(&error);
            ui.error(&failure);
            return failure.exit().into();
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

// The environment of the process. A variable that is not UTF-8 cannot be
// a `${NAME}` value, and is left out.
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
        let paths = Paths::from_env(env)
            .map_err(|error| Failure::protocol("ncp cannot find its files").caused_by(&error))?;
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
            interactive: (platform.interactive)(),
            named_config: cli.options.config_file.is_some(),
            before_rename: platform.before_rename,
            client_metadata_url: platform.client_metadata_url,
            cache: match (cli.options.no_cache, cli.options.refresh) {
                (true, _) => session::CacheUse::Off,
                (false, true) => session::CacheUse::WriteOnly,
                (false, false) => session::CacheUse::ReadWrite,
            },
        };
        match cli.command {
            Command::Server(command) => commands::server(&context, command).await,
            Command::Tool(command) => commands::tool(&context, command).await,
            Command::Resource(command) => commands::resource(&context, command).await,
            Command::Prompt(command) => commands::prompt(&context, command).await,
            Command::Auth(command) => auth::command(&context, command).await,
            Command::Secret(command) => secrets::command(&context, command).await,
            Command::Trust(args) => trust::command(&context, &args),
            Command::GenerateShellCompletion { shell } => completion::script(shell),
        }
    }
    .await;
    match result {
        Ok(()) => Exit::Success,
        Err(failure) => {
            ui.error(&failure);
            failure.exit()
        }
    }
}

// The configuration file that `--config-file` names. Unlike the default
// file, it must exist: a typing error in the path must not look like an
// empty configuration.
fn named_config_file(path: &std::path::Path) -> Result<std::path::PathBuf, Failure> {
    let path = std::path::absolute(path).map_err(|error| {
        Failure::usage(format!(
            "ncp cannot make the path {} absolute",
            path_code(path)
        ))
        .caused_by(&error)
    })?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(Failure::usage(format!(
            "The configuration file {} does not exist",
            path_code(&path)
        )))
    }
}

// Each stdio server runs in its own process group, so the SIGINT of the
// terminal does not reach it. Then `ncp` can send the cancel request and
// stop the server itself (MCP 2025-11-25, basic/utilities/cancellation).
// A store program stays in the group of `ncp`, so Ctrl-C stops it too.
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

// Help and version go to stdout with exit 0. A parse error is a usage
// error: clap text on a TTY, the JSON error object in a pipe.
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
            // `args` stops the program on a word that is not UTF-8.
            let words: Vec<String> = std::env::args_os()
                .skip(1)
                .map(|word| word.to_string_lossy().into_owned())
                .collect();
            let failure = usage_failure(error, &subcommands(&words));
            ui.error(&failure);
            failure.exit().into()
        }
    }
}

// The JSON error of a parse error. The text of clap is foreign text, so it
// is a cause.
fn usage_failure(error: &clap::Error, command: &[String]) -> Failure {
    let help = Hint::run("see the commands", "ncp --help");
    match error.kind() {
        // clap gives the help text as the error for these kinds.
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand | ErrorKind::MissingSubcommand => {
            let message = if command.is_empty() {
                "The command line has no command".to_string()
            } else {
                format!(
                    "The command {} has no subcommand",
                    code(&format!("ncp {}", command.join(" ")))
                )
            };
            Failure::usage(message).hint(help)
        }
        _ => {
            let failure = Failure::usage("The command line is not valid").cause(clap_cause(error));
            match suggestion(error) {
                Some(hint) => failure.hint(hint).hint(help),
                None => failure.hint(help),
            }
        }
    }
}

// The lines of clap up to `Usage:`, with no empty line and no `tip:` line.
fn clap_cause(error: &clap::Error) -> String {
    error
        .to_string()
        .lines()
        .take_while(|line| !line.starts_with("Usage:"))
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with("tip:"))
        .map(|line| line.strip_prefix("error: ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

// The hint with the names that clap suggests for a word with a typing error.
fn suggestion(error: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue};
    let names = [ContextKind::SuggestedSubcommand, ContextKind::SuggestedArg]
        .into_iter()
        .find_map(|kind| match error.get(kind)? {
            ContextValue::String(name) => Some(vec![name.clone()]),
            ContextValue::Strings(names) => Some(names.clone()),
            _ => None,
        })?;
    let names: Vec<String> = names.iter().map(|name| code(name)).collect();
    match names.split_last()? {
        (last, []) => Some(format!("Did you mean {last}?")),
        (last, rest) => Some(format!("Did you mean {} or {last}?", rest.join(", "))),
    }
}

// The subcommands in the words of the command line, for example `tool`
// in `ncp --timeout 5 tool`.
fn subcommands(words: &[String]) -> Vec<String> {
    use clap::CommandFactory as _;
    words
        .iter()
        .scan(Cli::command(), |command, word| {
            let next = command.find_subcommand(word).cloned();
            Some(next.map(|next| {
                *command = next;
                word.clone()
            }))
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure_of(words: &[&str]) -> Failure {
        let words: Vec<String> = words.iter().map(ToString::to_string).collect();
        let error = Cli::try_parse_from(std::iter::once("ncp".to_string()).chain(words.clone()))
            .expect_err("a parse error");
        usage_failure(&error, &subcommands(&words))
    }

    fn hint_messages(failure: &Failure) -> Vec<&str> {
        failure.hints().iter().map(Hint::message).collect()
    }

    #[test]
    fn no_command_gives_the_help_hint_as_a_command() {
        let failure = failure_of(&[]);
        assert_eq!(failure.message(), "The command line has no command");
        assert_eq!(failure.hints()[0].command(), Some("ncp --help"));
    }

    #[test]
    fn a_command_with_no_subcommand_names_the_command() {
        let failure = failure_of(&["--timeout", "5", "tool"]);
        assert_eq!(
            failure.message(),
            "The command `ncp tool` has no subcommand"
        );
    }

    #[test]
    fn a_missing_argument_gives_the_lines_of_clap_as_one_cause() {
        let failure = failure_of(&["tool", "list"]);
        assert_eq!(failure.message(), "The command line is not valid");
        assert_eq!(
            failure.diagnostic().causes(),
            ["the following required arguments were not provided:\n  <SERVER>"]
        );
        assert_eq!(
            hint_messages(&failure),
            ["To see the commands, run `ncp --help`"]
        );
    }

    #[test]
    fn a_typing_error_gives_the_names_that_clap_suggests() {
        let failure = failure_of(&["server", "lst"]);
        assert_eq!(
            failure.diagnostic().causes(),
            ["unrecognized subcommand 'lst'"]
        );
        assert_eq!(
            hint_messages(&failure),
            [
                "Did you mean `list`?",
                "To see the commands, run `ncp --help`"
            ]
        );
        let failure = failure_of(&["servr", "list"]);
        assert_eq!(
            hint_messages(&failure)[0],
            "Did you mean `secret` or `server`?"
        );
        let failure = failure_of(&["server", "list", "--verbos"]);
        assert_eq!(hint_messages(&failure)[0], "Did you mean `--verbose`?");
    }

    #[test]
    fn the_tip_of_clap_does_not_go_into_the_cause() {
        let failure = failure_of(&["tool", "list", "fx", "--bogus"]);
        assert_eq!(
            failure.diagnostic().causes(),
            ["unexpected argument '--bogus' found"]
        );
    }
}
