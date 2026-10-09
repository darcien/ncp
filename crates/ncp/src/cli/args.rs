//! The command-line grammar.
//!
//! The server and the tool are two words. The tool word is optional in the
//! grammar, so that `ncp` can give a better error than "missing argument":
//! for example a joined `server.tool` word, or `key=value` in the place of
//! the tool.

use clap::{Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
The normal order of work:
  ncp server list
  ncp server show <SERVER>
  ncp tool list <SERVER>
  ncp tool show <SERVER> <TOOL>
  ncp tool call <SERVER> <TOOL> [key=value...]

A ${NAME} in an env or headers value comes from the environment, the
native store, or the credential file. Store a value with:
  ncp secret set <NAME>

Output is text on a TTY and JSON in a pipe.";

#[derive(Debug, Parser)]
#[command(
    name = "ncp",
    version,
    about = "A command-line MCP client",
    after_help = AFTER_HELP,
    disable_help_subcommand = true,
    propagate_version = true
)]
pub struct Cli {
    #[command(flatten)]
    pub options: GlobalOptions,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args)]
pub struct GlobalOptions {
    /// The time limit for one request, in whole seconds. A timeout sends a
    /// cancel request.
    #[arg(
        long,
        global = true,
        value_name = "SECONDS",
        default_value_t = 300,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub timeout: u64,
    /// Show diagnostics on stderr: where each value comes from (never the
    /// value), the proxy route, the cached era. -vv also shows the JSON-RPC
    /// messages and the HTTP requests. The credentials of the connection
    /// are hidden; the messages show as they are.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
    /// Show the time of each step on stderr: CPU, configuration,
    /// credentials, server start, DNS, connect and each response.
    #[arg(long, global = true)]
    pub timings: bool,
    /// Show no hints and no warnings on stderr. Errors stay.
    #[arg(short, long, global = true, conflicts_with = "verbose", action = clap::ArgAction::Count)]
    pub quiet: u8,
    /// Color in the labels on stderr. Default: auto, which is on for a
    /// TTY unless NO_COLOR is set.
    #[allow(
        clippy::doc_markdown,
        reason = "clap shows this text as help, where backticks show"
    )]
    #[arg(long, global = true, value_enum, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    /// Read and write no cache: no cached era and no cached tool list.
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Read no cache, but write it: find the era and get the tool list
    /// again.
    #[arg(long, global = true, conflicts_with = "no_cache")]
    pub refresh: bool,
    /// The output format. Default: text on a TTY, else JSON.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub output_format: Option<OutputFormat>,
    /// The configuration file in place of the default file, mcp.json in the
    /// ncp folder of the XDG configuration folder. It must exist.
    #[arg(long, global = true, value_name = "PATH")]
    pub config_file: Option<std::path::PathBuf>,
}

/// When to use color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// On for a TTY, unless NO_COLOR is set
    #[allow(
        clippy::doc_markdown,
        reason = "clap shows this text as help, where backticks show"
    )]
    Auto,
    Always,
    Never,
}

/// The output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Text for people
    Text,
    /// One JSON value on stdout, and JSON Lines on stderr
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Configured servers
    #[command(subcommand)]
    Server(ServerCommand),
    /// Tools of a server
    #[command(subcommand)]
    Tool(ToolCommand),
    /// Resources of a server
    #[command(subcommand)]
    Resource(ResourceCommand),
    /// Prompts of a server
    #[command(subcommand)]
    Prompt(PromptCommand),
    /// OAuth login of HTTP servers
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Named values for ${NAME} in the configuration
    #[command(subcommand)]
    Secret(SecretCommand),
    /// Print the completion script for a shell. Server names complete from
    /// the configuration.
    GenerateShellCompletion {
        /// The shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

/// A shell with a completion script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Subcommand)]
pub enum ResourceCommand {
    /// List the resources and the resource templates of a server.
    List {
        /// The server name from the configuration
        server: String,
    },
    /// Read one resource. The text goes to stdout as it is; with
    /// --output-format json, the MCP result.
    Read {
        /// The server name from the configuration
        server: String,
        /// The resource URI
        uri: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum PromptCommand {
    /// List the prompts of a server.
    List {
        /// The server name from the configuration
        server: String,
    },
    /// Get one prompt with its arguments: key=value pairs, or key=@path
    /// for the text of a file.
    Get {
        /// The server name from the configuration
        server: String,
        /// The prompt name
        prompt: String,
        /// The prompt arguments
        arguments: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Log in to an HTTP server with OAuth and keep the tokens.
    Login {
        /// The server name from the configuration
        server: String,
        /// Do not open a browser: print the URL, and read the redirect URL
        /// from stdin.
        #[arg(long)]
        no_browser: bool,
        /// The place for the tokens.
        #[arg(long, value_enum, default_value_t = CredentialPlace::Native)]
        credential_store: CredentialPlace,
    },
    /// Remove the OAuth tokens of a server from all places. The client
    /// registration stays.
    Logout {
        /// The server name from the configuration
        server: String,
    },
    /// Show the OAuth login of each server, or of one server. Shows no
    /// token.
    Status {
        /// The server name from the configuration
        server: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SecretCommand {
    /// Read one value from stdin and keep it under the name. On a TTY the
    /// input is not shown.
    Set {
        /// The name, as in ${NAME} in the configuration
        name: String,
        /// The place for the value.
        #[arg(long, value_enum, default_value_t = CredentialPlace::Native)]
        credential_store: CredentialPlace,
    },
    /// List the names of the stored secrets and the place of each one.
    /// Shows no value.
    List,
    /// Remove the secret from all places.
    Delete {
        /// The name, as in ${NAME} in the configuration
        name: String,
    },
}

/// The place for a new value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CredentialPlace {
    /// The macOS Keychain or the Linux Secret Service
    Native,
    /// The opt-in credential file, mode 0600
    File,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// List the servers in the configuration file. Starts no server.
    List,
    /// Show what the server says about itself in its handshake, with its
    /// instructions.
    Show {
        /// The server name from the configuration
        server: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ToolCommand {
    /// List the tools of a server.
    List {
        /// The server name from the configuration
        server: String,
    },
    /// Show one tool: its arguments, output and an example call.
    Show {
        /// The server name from the configuration
        server: String,
        /// The tool name
        tool: Option<String>,
    },
    /// Call a tool.
    ///
    /// Arguments: key=value pairs, one JSON object, or - to read a JSON
    /// object from stdin. A key=value value is text, except when the input
    /// schema gives the property another type: then the value is JSON.
    /// key:=json gives a JSON value, and key=@path gives the text of a file
    /// (@@ at the start is a literal @).
    #[command(disable_help_flag = true)]
    Call(CallArgs),
}

#[derive(Debug, Args)]
pub struct CallArgs {
    /// The server name from the configuration
    pub server: Option<String>,
    /// The tool name
    pub tool: Option<String>,
    /// The tool arguments
    pub arguments: Vec<String>,
    /// Do not check the arguments against the input schema of the tool.
    #[arg(long)]
    pub no_validate: bool,
    /// Show the tool, as `tool show` does. With no tool, show this help.
    #[arg(short, long)]
    pub help: bool,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn grammar_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn tool_call_takes_help_flag_and_arguments_in_any_order() {
        let cli = Cli::try_parse_from([
            "ncp",
            "tool",
            "call",
            "gh",
            "search",
            "q=a b",
            "--no-validate",
            "--help",
            "--timeout",
            "5",
        ])
        .unwrap();
        let Command::Tool(ToolCommand::Call(call)) = cli.command else {
            panic!("not a call")
        };
        assert_eq!(call.server.as_deref(), Some("gh"));
        assert_eq!(call.arguments, ["q=a b"]);
        assert!(call.no_validate && call.help);
        assert_eq!(cli.options.timeout, 5);
    }

    #[test]
    fn secret_set_takes_the_place_of_the_value() {
        let cli = Cli::try_parse_from([
            "ncp",
            "secret",
            "set",
            "TOKEN",
            "--credential-store",
            "file",
        ])
        .unwrap();
        let Command::Secret(SecretCommand::Set {
            name,
            credential_store,
        }) = cli.command
        else {
            panic!("not secret set")
        };
        assert_eq!(name, "TOKEN");
        assert_eq!(credential_store, CredentialPlace::File);
        assert!(
            Cli::try_parse_from(["ncp", "secret", "set", "T", "--credential-store", "x"]).is_err()
        );
    }

    #[test]
    fn rejects_zero_timeout() {
        assert!(Cli::try_parse_from(["ncp", "--timeout", "0", "server", "list"]).is_err());
    }

    #[test]
    fn stdin_marker_is_an_argument_and_double_dash_ends_the_options() {
        let cli = Cli::try_parse_from(["ncp", "tool", "call", "gh", "t", "-"]).unwrap();
        let Command::Tool(ToolCommand::Call(call)) = cli.command else {
            panic!("not a call")
        };
        assert_eq!(call.arguments, ["-"]);
        let cli = Cli::try_parse_from(["ncp", "tool", "call", "--", "gh", "-t"]).unwrap();
        let Command::Tool(ToolCommand::Call(call)) = cli.command else {
            panic!("not a call")
        };
        assert_eq!(call.tool.as_deref(), Some("-t"));
    }
}
