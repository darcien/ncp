//! The command-line grammar.
//!
//! The server and the tool are two words. The tool word is optional in the
//! grammar, so that `ncp` can give a better error than "missing argument".
//! Examples are a joined `server.tool` word, or `key=value` in the place of
//! the tool.

use clap::{Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
The usual steps:
  ncp server add <SERVER> <URL>
  ncp server list
  ncp server show <SERVER>
  ncp tool list <SERVER>
  ncp tool show <SERVER> <TOOL>
  ncp tool call <SERVER> <TOOL> [key=value...]

A ${NAME} in an env, headers or oauth.clientSecret value comes from the
environment, the native store or the credential file. To store a value,
run:
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
    /// The time limit in whole seconds for one request, a login or an input.
    /// After a timeout, ncp cancels the request or stops the connection.
    #[arg(
        long,
        global = true,
        value_name = "SECONDS",
        default_value_t = 300,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub timeout: u64,
    /// Show log lines on stderr: where each value comes from (never the
    /// value), the proxy route and the cached protocol version. -vv also shows
    /// the JSON-RPC messages and the HTTP requests. ncp hides the credentials
    /// of the connection, and shows all other text with no change.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
    /// Show the time of each step on stderr: CPU, configuration,
    /// credentials, server start, DNS, connect and each response.
    #[arg(long, global = true)]
    pub timings: bool,
    /// Show no hints, no warnings and no progress on stderr. ncp still shows
    /// errors.
    #[arg(short, long, global = true, conflicts_with = "verbose", action = clap::ArgAction::Count)]
    pub quiet: u8,
    /// Do not read or write the cache: the protocol version and the tool
    /// list of each server.
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Do not read the cache, but write it: ncp finds the protocol version
    /// and gets the tool list again. --no-cache has priority over --refresh.
    #[arg(long, global = true)]
    pub refresh: bool,
    /// The output format. Default: text on a TTY, JSON in a pipe.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub output_format: Option<OutputFormat>,
    /// Use this configuration file in place of the global file: ncp/mcp.json in
    /// the XDG configuration folder, by default ~/.config/ncp/mcp.json.
    /// The file must exist. ncp then does not search for a project .mcp.json,
    /// and runs the servers of this file with no trust question.
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
    /// Secrets for ${NAME} in the configuration
    #[command(subcommand)]
    Secret(SecretCommand),
    /// Trust the project .mcp.json of the current folder
    ///
    /// ncp shows the entries of the file. The servers of a project file do
    /// not run until you trust the file. After each change of the file, you
    /// must trust it again.
    Trust(TrustArgs),
    /// Show the completion script for a shell
    ///
    /// The script completes server names from the configuration.
    GenerateShellCompletion {
        /// The shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

/// The options of ncp trust. Only one can be given.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub struct TrustArgs {
    /// Show the entries and the trust state, and change nothing
    #[arg(long)]
    pub show: bool,
    /// Remove the trust of the project file
    #[arg(long)]
    pub untrust: bool,
    /// Ignore the project file: ncp does not ask again until you run ncp trust
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
    /// Read one resource
    ///
    /// On a TTY, ncp shows the contents for people. With --output-format text
    /// in a pipe, ncp writes the contents with no change: the text, or the
    /// bytes of a blob. With --output-format json, ncp writes the MCP result.
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
    /// Get one prompt
    ///
    /// Give the arguments as key=value pairs. Each value is text, because MCP
    /// gives prompt arguments as text.
    Get {
        /// The server name from the configuration
        server: String,
        /// The prompt name
        prompt: String,
        /// The prompt arguments: key=value pairs
        arguments: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Log in to an HTTP server with OAuth and store the tokens.
    Login {
        /// The server name from the configuration
        server: String,
        /// Do not open a browser: show the URL, and read the redirect URL from
        /// stdin.
        #[arg(long)]
        no_browser: bool,
        /// The credential store for the tokens.
        #[arg(long, value_enum, default_value_t = CredentialPlace::Native)]
        credential_store: CredentialPlace,
    },
    /// Delete the OAuth tokens of a server from all credential stores
    ///
    /// The OAuth client registration stays.
    Logout {
        /// The server name from the configuration
        server: String,
    },
    /// Show the OAuth login of each server, or of one server
    ///
    /// ncp shows no token.
    Status {
        /// The server name from the configuration
        server: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SecretCommand {
    /// Store a value for ${NAME}
    ///
    /// ncp reads one value from stdin. On a TTY, ncp does not show the input.
    /// The value goes only to the destination of the entries that name
    /// ${NAME}.
    Set {
        /// The name, as in ${NAME} in the configuration
        name: String,
        /// Use only the destination of this server.
        #[arg(long, value_name = "SERVER")]
        server: Option<String>,
        /// The credential store for the value.
        #[arg(long, value_enum, default_value_t = CredentialPlace::Native)]
        credential_store: CredentialPlace,
    },
    /// List the stored secrets
    ///
    /// For each secret, ncp shows the name, the destination, the credential
    /// store and the servers that use it. ncp shows no value. The servers come
    /// from the global file and the project file of the current folder. A
    /// project in another folder can still use a secret that no server uses
    /// here.
    List,
    /// Delete a stored secret from all credential stores
    ///
    /// With no option, ncp deletes it only when the name has one destination.
    Delete {
        /// The name, as in ${NAME} in the configuration
        name: String,
        /// Delete only the item of the destination of this server.
        #[arg(long, value_name = "SERVER", group = "removal")]
        server: Option<String>,
        /// Delete the items that no server of the global file or of the
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

A secret in one command. ncp asks for the value (hidden on a TTY) or
reads it from a pipe. ncp stores the value, and the file keeps ${LINEAR_KEY}:
  ncp server add linear https://mcp.linear.app/mcp \\
    --header 'Authorization: Bearer ${LINEAR_KEY}' --secret LINEAR_KEY
The same in two commands:
  ncp server add linear https://mcp.linear.app/mcp \\
    --header 'Authorization: Bearer ${LINEAR_KEY}'
  ncp secret set LINEAR_KEY

The single quotes stop the shell from expanding ${NAME}. A stored value
goes only to its destination: the origin of the URL (the scheme, host and
port), or the stdio servers. The entries with the same destination share
it. --env NAME writes ${NAME}. --env NAME=VALUE writes VALUE into the file
with no change.";

#[derive(Debug, Args)]
pub struct AddArgs {
    /// The server name: letters, digits, - and _
    pub server: String,
    /// The URL of an HTTP server
    pub url: Option<String>,
    /// Add the entry to the global file
    #[arg(long)]
    pub global: bool,
    /// A header of an HTTP server. The value of an Authorization or
    /// Proxy-Authorization header must be '<scheme> ${NAME}' or '${NAME}'.
    // A value that starts with "-" is a value, so that clap does not show
    // it in an error: the text can be a secret.
    #[arg(
        long = "header",
        value_name = "'NAME: VALUE'",
        allow_hyphen_values = true
    )]
    pub headers: Vec<String>,
    /// An environment variable of a stdio server: NAME writes ${NAME}, and
    /// NAME=VALUE writes VALUE with no change
    #[arg(long = "env", value_name = "NAME[=VALUE]", allow_hyphen_values = true)]
    pub env: Vec<String>,
    /// Read the value of ${NAME} from the terminal or stdin, and store it for
    /// the destination of the entry
    #[arg(long, value_name = "NAME", allow_hyphen_values = true)]
    pub secret: Option<String>,
    /// The credential store for the value of --secret. Default: native.
    #[arg(long, value_enum, requires = "secret")]
    pub credential_store: Option<CredentialPlace>,
    /// The command of a stdio server and its arguments, after --
    #[arg(last = true, value_name = "COMMAND")]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CredentialPlace {
    /// The native store: the macOS Keychain or the Linux Secret Service
    Native,
    /// The credential file, with mode 0600
    File,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Add a server to the project .mcp.json or the global file
    ///
    /// In the home folder, or with --global, ncp adds it to the global file.
    /// With --config-file, ncp adds it to that file. ncp changes no other
    /// part of the file, and does not connect to the server.
    #[command(after_help = ADD_HELP)]
    Add(AddArgs),
    /// Remove a server from the file that gives it
    ///
    /// With --global, ncp removes it from the global file. ncp also deletes
    /// the OAuth tokens of the entry, when no other entry of the name takes
    /// effect. Stored ${NAME} values stay, because other entries of the same
    /// destination can use them. A project in another folder with an entry of
    /// the same name and URL then needs a new login.
    Remove {
        /// The server name from the configuration
        server: String,
        /// Remove the entry from the global file
        #[arg(long)]
        global: bool,
    },
    /// List the servers of the global file and of the project file
    ///
    /// ncp starts no server.
    List,
    /// Show the information and the instructions that the server gives in
    /// its handshake.
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
    /// Call a tool
    ///
    /// Arguments: key=value pairs, one JSON object, or - to read a JSON
    /// object from stdin. A key=value value is text. When the input schema
    /// gives the property another type, the value is JSON. To give the text
    /// of a file, use the shell: key="$(cat FILE)".
    #[command(disable_help_flag = true)]
    Call(CallArgs),
}

#[derive(Debug, Args)]
pub struct CallArgs {
    /// The server name from the configuration
    pub server: Option<String>,
    /// The tool name
    pub tool: Option<String>,
    /// The tool arguments: key=value pairs, one JSON object, or -
    pub arguments: Vec<String>,
    /// Do not check the arguments against the input schema of the tool.
    #[arg(long)]
    pub no_validate: bool,
    /// Show the tool, as ncp tool show does. With no tool, show this help.
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
