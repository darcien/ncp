//! The command-line grammar.
//!
//! The server and the tool are two words. The tool word is optional in the
//! grammar, so that `ncp` can give a better error than "missing argument".
//! Examples are a joined `server.tool` word, or `key=value` in the place of
//! the tool.

use clap::{Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
The normal order of work:
  ncp server add <SERVER> <URL>
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
    /// value), the proxy route, the cached protocol version. -vv also shows the
    /// JSON-RPC messages and the HTTP requests. ncp hides the credentials of
    /// the connection, and shows the messages as they are.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
    /// Show the time of each step on stderr: CPU, configuration,
    /// credentials, server start, DNS, connect and each response.
    #[arg(long, global = true)]
    pub timings: bool,
    /// Show no hints and no warnings on stderr. Errors stay.
    #[arg(short, long, global = true, conflicts_with = "verbose", action = clap::ArgAction::Count)]
    pub quiet: u8,
    /// Read and write no cache: no cached protocol version and no cached
    /// tool list.
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Read no cache, but write it: find the protocol version and get the
    /// tool list again. --no-cache wins over it, as in uv.
    #[arg(long, global = true)]
    pub refresh: bool,
    /// The output format. Default: text on a TTY, else JSON.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub output_format: Option<OutputFormat>,
    /// The configuration file in place of the default file, mcp.json in the
    /// ncp folder of the XDG configuration folder. It must exist. ncp then
    /// does not search for a project .mcp.json, and runs the servers of this
    /// file with no trust question.
    #[arg(long, global = true, value_name = "PATH")]
    pub config_file: Option<std::path::PathBuf>,
}

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
    /// Trust the project .mcp.json of the current folder, and show its
    /// entries. Before the trust, its servers do not run. Each change of
    /// the file needs a new trust.
    Trust(TrustArgs),
    /// Show the completion script for a shell. Server names complete from the
    /// configuration.
    GenerateShellCompletion {
        /// The shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

/// The options of `ncp trust`, as `mise trust`: only one of them.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub struct TrustArgs {
    /// Show the entries and the trust state, and change nothing
    #[arg(long)]
    pub show: bool,
    /// Remove the trust
    #[arg(long)]
    pub untrust: bool,
    /// Ignore the file: ncp does not ask again until ncp trust
    #[arg(long)]
    pub ignore: bool,
}

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
    /// Get one prompt with its arguments: key=value pairs. The spec gives
    /// prompt arguments as text.
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
        /// Do not open a browser: show the URL, and read the redirect URL from
        /// stdin.
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
    /// Read one value from stdin and keep it under the name, for the
    /// destination of the entries that name ${NAME}. On a TTY, ncp does not
    /// show the input.
    Set {
        /// The name, as in ${NAME} in the configuration
        name: String,
        /// Use only the destination of this entry.
        #[arg(long, value_name = "SERVER")]
        server: Option<String>,
        /// The place for the value.
        #[arg(long, value_enum, default_value_t = CredentialPlace::Native)]
        credential_store: CredentialPlace,
    },
    /// List the stored secrets: the name, the destination, the place and
    /// the servers that use each one. Shows no value. The servers come
    /// from the global file and the project file of the current folder. A
    /// project in another folder can still use a secret that no server
    /// uses here.
    List,
    /// Delete the stored secret from all places. With no flag, only when
    /// the name has one destination.
    Delete {
        /// The name, as in ${NAME} in the configuration
        name: String,
        /// Delete only the item of the destination of this entry.
        #[arg(long, value_name = "SERVER", group = "removal")]
        server: Option<String>,
        /// Delete the items that no entry of the global file or of the
        /// project file of the current folder uses. A project in another
        /// folder can still use them.
        #[arg(long, group = "removal")]
        unused: bool,
        /// Delete the items of all destinations.
        #[arg(long, group = "removal")]
        all: bool,
    },
}

const ADD_HELP: &str = "\
Examples:
  ncp server add docs https://docs.example.com/mcp
  ncp server add github --env GITHUB_TOKEN --secret GITHUB_TOKEN -- github-mcp-server stdio

A secret in one command: ncp asks for the value (hidden on a TTY), or
reads it from a pipe, stores it, and the file keeps ${LINEAR_KEY}:
  ncp server add linear https://mcp.linear.app/mcp \\
    --header 'Authorization: Bearer ${LINEAR_KEY}' --secret LINEAR_KEY
The same in two commands:
  ncp server add linear https://mcp.linear.app/mcp \\
    --header 'Authorization: Bearer ${LINEAR_KEY}'
  ncp secret set LINEAR_KEY

The single quotes keep ${NAME} from the shell. A stored value goes only to
its origin, and the entries with the same origin share it. --env NAME
writes ${NAME}; --env NAME=VALUE writes VALUE into the file as it is.";

#[derive(Debug, Args)]
pub struct AddArgs {
    /// The server name: letters, digits, - and _
    pub server: String,
    /// The URL of an http server
    pub url: Option<String>,
    /// Add the entry to the global file
    #[arg(long)]
    pub global: bool,
    /// A header of an http server. An Authorization header must be
    /// '<scheme> ${NAME}' or '${NAME}'.
    // A value that starts with "-" is a value, so that clap does not show
    // it in an error: the text can be a secret.
    #[arg(
        long = "header",
        value_name = "'NAME: VALUE'",
        allow_hyphen_values = true
    )]
    pub headers: Vec<String>,
    /// An environment variable of a stdio server: NAME writes ${NAME}, and
    /// NAME=VALUE writes VALUE as it is
    #[arg(long = "env", value_name = "NAME[=VALUE]", allow_hyphen_values = true)]
    pub env: Vec<String>,
    /// Read the value of ${NAME} and store it for the origin of the entry
    #[arg(long, value_name = "NAME", allow_hyphen_values = true)]
    pub secret: Option<String>,
    /// The place for the value of --secret. Default: native.
    #[arg(long, value_enum, requires = "secret")]
    pub credential_store: Option<CredentialPlace>,
    /// The command of a stdio server and its arguments
    #[arg(last = true, value_name = "COMMAND")]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CredentialPlace {
    /// The macOS Keychain or the Linux Secret Service
    Native,
    /// The opt-in credential file, mode 0600
    File,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Add a server to the project .mcp.json, or with --global to
    /// ~/.config/ncp/mcp.json. ncp keeps every other byte of the file, and
    /// does not contact the server.
    #[command(after_help = ADD_HELP)]
    Add(AddArgs),
    /// Remove a server from the file that gives it, or with --global from
    /// ~/.config/ncp/mcp.json, and its OAuth tokens. Stored ${NAME} values
    /// stay: other entries of the same origin can use them. A project in
    /// another folder with an entry of the same name and URL then needs a
    /// new login.
    Remove {
        /// The server name from the configuration
        server: String,
        /// Remove the entry of the global file
        #[arg(long)]
        global: bool,
    },
    /// List the servers of the global file and of the project file. Starts
    /// no server.
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
    /// For the text of a file, use the shell: key="$(cat FILE)".
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
            ..
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
    fn secret_delete_takes_one_removal_flag() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(["ncp", "secret", "delete", "K"].iter().chain(args))
        };
        assert!(parse(&[]).is_ok());
        assert!(parse(&["--server", "a"]).is_ok());
        assert!(parse(&["--unused"]).is_ok());
        assert!(parse(&["--all"]).is_ok());
        assert!(parse(&["--all", "--unused"]).is_err());
        assert!(parse(&["--server", "a", "--all"]).is_err());
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
