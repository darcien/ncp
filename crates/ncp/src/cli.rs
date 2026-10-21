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
    ui::{Format, Stdout, Ui, Verbosity},
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
/// the TTY checks, a hook before a rename, and the client metadata URL.
/// Tests give fakes, so that a test never uses the real Keychain, the real
/// Secret Service, or a real browser, and a test server can expect its own
/// client ID.
#[derive(Clone, Copy)]
pub struct Platform {
    pub open_native: secrets::OpenNative,
    pub open_browser: auth::OpenBrowser,
    pub interactive: fn() -> bool,
    /// True when stdout is a TTY.
    pub stdout_terminal: fn() -> bool,
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
        stdout_terminal: ui::stdout_is_terminal,
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
    let stdout = Stdout::from_terminal((platform.stdout_terminal)());
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(&error, stdout),
    };
    let env = process_env();
    let ui = Ui::new(
        match cli.options.output_format {
            Some(OutputFormat::Text) | None => Format::Text,
            Some(OutputFormat::Json) => Format::Json,
        },
        Verbosity::from_flags(cli.options.quiet, cli.options.verbose),
        stdout,
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
// error: clap text, or the JSON error object with `--output-format json`.
fn parse_failure(error: &clap::Error, stdout: Stdout) -> ExitCode {
    // `args` stops the program on a word that is not UTF-8.
    let words: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    match (error.kind(), format_in(&words)) {
        (ErrorKind::DisplayHelp | ErrorKind::DisplayVersion, _) | (_, Format::Text) => {
            let _ = error.print();
            if error.use_stderr() {
                Exit::Usage.into()
            } else {
                Exit::Success.into()
            }
        }
        (_, Format::Json) => {
            let failure = usage_failure(error, &subcommands(&words));
            Ui::new(Format::Json, Verbosity::Normal, stdout).error(&failure);
            failure.exit().into()
        }
    }
}

// The format that the words of a command line that is not valid ask for.
// The last `--output-format` wins, so that the error has the format of a
// valid run of the same words.
fn format_in(words: &[String]) -> Format {
    match Scan::of(words).format {
        Some("json") => Format::Json,
        _ => Format::Text,
    }
}

// The words of a command line that clap did not accept, read as clap reads
// them. clap gives no options and no subcommands with its error. The scan
// follows the grammar of the current subcommand. The word after an option
// that takes a value is that value. When the option does not permit values
// that start with `-`, the scan reads such a word again as an option. Any
// other `--` ends the options.
struct Scan<'a> {
    command: clap::Command,
    next: Next,
    subcommands: Vec<String>,
    format: Option<&'a str>,
}

// What the scan expects as the next word.
#[derive(Clone, Copy)]
enum Next {
    Word,
    // The value of an option. `hyphen`: the value can start with `-`.
    // `format`: the option is `--output-format`.
    Value { hyphen: bool, format: bool },
    Arguments,
}

impl<'a> Scan<'a> {
    fn of(words: &'a [String]) -> Self {
        use clap::CommandFactory as _;
        let mut command = Cli::command();
        command.build();
        let start = Self {
            command,
            next: Next::Word,
            subcommands: Vec::new(),
            format: None,
        };
        words.iter().map(String::as_str).fold(start, Self::read)
    }

    fn read(self, word: &'a str) -> Self {
        match self.next {
            Next::Arguments => self,
            Next::Value { hyphen, format } if hyphen || word == "-" || !word.starts_with('-') => {
                Self {
                    next: Next::Word,
                    format: if format { Some(word) } else { self.format },
                    ..self
                }
            }
            Next::Value { .. } | Next::Word => self.read_word(word),
        }
    }

    // One word where an option, a subcommand or an argument can come.
    fn read_word(self, word: &'a str) -> Self {
        if word == "--" {
            return Self {
                next: Next::Arguments,
                ..self
            };
        }
        let Some(long) = word.strip_prefix("--") else {
            return match self.command.find_subcommand(word).cloned() {
                Some(command) => Self {
                    command,
                    subcommands: [self.subcommands, vec![word.to_string()]].concat(),
                    ..self
                },
                None => Self {
                    next: Next::Word,
                    ..self
                },
            };
        };
        let (name, inline) = match long.split_once('=') {
            Some((name, inline)) => (name, Some(inline)),
            None => (long, None),
        };
        let option = self
            .command
            .get_arguments()
            .find(|arg| arg.get_long() == Some(name))
            .map(|arg| {
                (
                    arg.get_id() == "output_format",
                    arg.get_action().takes_values(),
                    arg.is_allow_hyphen_values_set(),
                )
            });
        match (option, inline) {
            (Some((true, _, _)), Some(inline)) => Self {
                next: Next::Word,
                format: Some(inline),
                ..self
            },
            (Some((format, true, hyphen)), None) => Self {
                next: Next::Value { hyphen, format },
                ..self
            },
            _ => Self {
                next: Next::Word,
                ..self
            },
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
    Scan::of(words).subcommands
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
        // `server` is the value of `--config-file`, not a command.
        let failure = failure_of(&["--config-file", "server", "tool"]);
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
    fn a_usage_error_has_json_only_when_the_option_before_double_dash_asks() {
        let format =
            |words: &[&str]| format_in(&words.iter().map(ToString::to_string).collect::<Vec<_>>());
        assert_eq!(format(&["tool", "frob"]), Format::Text);
        assert_eq!(format(&["--output-format", "json", "tool"]), Format::Json);
        assert_eq!(format(&["tool", "--output-format=json"]), Format::Json);
        assert_eq!(format(&["--output-format", "text"]), Format::Text);
        assert_eq!(format(&["--output-format"]), Format::Text);
        assert_eq!(
            format(&["--output-format", "text", "tool", "--output-format=json"]),
            Format::Json
        );
        assert_eq!(
            format(&["server", "add", "x", "--", "s", "--output-format", "json"]),
            Format::Text
        );
    }

    #[test]
    fn the_value_of_an_option_is_not_an_option() {
        let format =
            |words: &[&str]| format_in(&words.iter().map(ToString::to_string).collect::<Vec<_>>());
        let add = ["server", "add", "y"];
        let with = |words: &[&str]| format(&[&add[..], words].concat());
        // clap takes `--output-format` as the value of `--env`.
        assert_eq!(
            with(&["--env", "--output-format", "json", "--bogus"]),
            Format::Text
        );
        // clap takes `--` as the value of `--env`, and then the option.
        assert_eq!(
            with(&["--env", "--", "--output-format", "json", "--bogus"]),
            Format::Json
        );
        assert_eq!(
            with(&["--env", "--env", "--output-format", "json", "--bogus"]),
            Format::Json
        );
        assert_eq!(
            with(&["--env=--output-format", "json", "--bogus"]),
            Format::Text
        );
        // `tool list` has no `--env`. clap stops there, and the scan reads
        // on, so that the last `--output-format` wins.
        assert_eq!(
            format(&["tool", "list", "x", "--env", "--output-format", "json"]),
            Format::Json
        );
        // `--timeout` does not permit a value that starts with `-`.
        assert_eq!(
            format(&["--timeout", "--output-format", "json", "tool", "frob"]),
            Format::Json
        );
        // The value of `--config-file` is not a subcommand.
        assert_eq!(
            format(&[
                "--config-file",
                "server",
                "tool",
                "add",
                "y",
                "--env",
                "--output-format",
                "json"
            ]),
            Format::Json
        );
        assert_eq!(
            format(&["tool", "--output-format=json", "frob"]),
            Format::Json
        );
    }

    // The scan of a usage error knows an option only by its full long name,
    // with one value or none, and a subcommand only by its full name. Before
    // `--`, an argument that is not an option takes no word that starts with
    // `-`, other than `-` alone.
    #[test]
    fn the_scan_of_a_usage_error_agrees_with_the_grammar() {
        use clap::CommandFactory as _;
        fn check(command: &clap::Command, path: &[&str]) {
            let name = command.get_name();
            assert!(!command.is_allow_external_subcommands_set(), "{name}");
            assert!(!command.is_subcommand_precedence_over_arg_set(), "{name}");
            assert!(!command.is_args_conflicts_with_subcommands_set(), "{name}");
            assert_eq!(command.get_long_flag(), None, "{name}");
            assert_eq!(command.get_short_flag(), None, "{name}");
            for arg in command.get_arguments() {
                let id = arg.get_id();
                assert!(!arg.is_require_equals_set(), "{name} {id}");
                assert!(arg.get_value_terminator().is_none(), "{name} {id}");
                if arg.is_positional() {
                    // A positional word would hide the name of a subcommand.
                    assert!(!command.has_subcommands(), "{name} {id}");
                    assert!(
                        !arg.is_allow_hyphen_values_set() && !arg.is_trailing_var_arg_set(),
                        "{name} {id}"
                    );
                } else if arg.get_action().takes_values() {
                    assert!(arg.get_long().is_some(), "{name} {id}");
                    assert_eq!(arg.get_short(), None, "{name} {id}");
                    assert_eq!(arg.get_all_short_aliases(), None, "{name} {id}");
                    assert_eq!(arg.get_all_aliases(), None, "{name} {id}");
                    let values = arg.get_num_args().map(|range| range.max_values());
                    assert_eq!(values, Some(1), "{name} {id}");
                }
            }
            check_abbreviations(command, path);
            for subcommand in command.get_subcommands() {
                check(subcommand, &[path, &[subcommand.get_name()]].concat());
            }
        }
        // clap does not show the settings that accept a part of a long name
        // or of a subcommand name, so their effect is the check. A command
        // inherits them, so each command gets the check.
        fn check_abbreviations(command: &clap::Command, path: &[&str]) {
            let parse = |word: &str| {
                let words = std::iter::once("ncp").chain(path.iter().copied());
                Cli::try_parse_from(words.chain([word]))
                    .map(|_| ())
                    .map_err(|error| error.kind())
            };
            let longs: Vec<&str> = command
                .get_arguments()
                .filter_map(clap::Arg::get_long)
                .collect();
            for long in &longs {
                let part = &long[..long.len() - 1];
                if !part.is_empty() && !longs.contains(&part) {
                    let kind = parse(&format!("--{part}"));
                    assert_eq!(kind, Err(ErrorKind::UnknownArgument), "{path:?} --{part}");
                }
            }
            for subcommand in command.get_subcommands() {
                let name = subcommand.get_name();
                let part = &name[..name.len() - 1];
                if command.find_subcommand(part).is_none() {
                    assert_eq!(
                        parse(part),
                        Err(ErrorKind::InvalidSubcommand),
                        "{path:?} {part}"
                    );
                }
            }
        }
        let mut root = Cli::command();
        root.build();
        check(&root, &[]);
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
