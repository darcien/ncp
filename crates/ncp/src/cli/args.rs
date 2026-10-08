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
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Configured servers
    #[command(subcommand)]
    Server(ServerCommand),
    /// Tools of a server
    #[command(subcommand)]
    Tool(ToolCommand),
    /// OAuth login of HTTP servers
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Named values for ${NAME} in the configuration
    #[command(subcommand)]
    Secret(SecretCommand),
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
