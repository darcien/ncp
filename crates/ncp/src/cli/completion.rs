//! Shell completion: `generate-shell-completion <bash|zsh|fish>`.
//!
//! clap writes the static part of each script: commands, options and value
//! choices. A server name comes from the configuration at completion time,
//! through the hidden command `ncp __complete-servers`. It reads only the
//! configuration file: it starts no server and reads no credential store.
//! A tool name comes from the list cache only, through `ncp
//! __complete-tools SERVER`: a tab never starts a server.

use clap::CommandFactory;
use clap_complete::Shell;

use super::{
    args::{Cli, CompletionShell},
    outcome::Failure,
    ui,
};
use crate::{config, expand::Env, paths::Paths};

const BIN: &str = "ncp";

/// Writes the completion script for the shell.
///
/// # Errors
///
/// Returns a failure when stdout cannot be written.
pub fn script(shell: CompletionShell) -> Result<(), Failure> {
    let mut command = Cli::command();
    let mut generated = Vec::new();
    let generator = match shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    clap_complete::generate(generator, &mut command, BIN, &mut generated);
    ui::write_stdout(&with_server_names(
        shell,
        &String::from_utf8_lossy(&generated),
    ))
}

/// The first word of the hidden command that lists the server names.
pub const SERVERS_COMMAND: &str = "__complete-servers";

/// The command that the scripts run for the server names. Its errors go
/// nowhere, so that a broken configuration does not break the shell.
const SERVERS: &str = "ncp __complete-servers 2>/dev/null";

/// The first word of the hidden command that lists the tool names of a
/// server from the list cache.
pub const TOOLS_COMMAND: &str = "__complete-tools";

/// The commands with a tool name as their second argument.
const TOOL_COMMANDS: [(&str, &str); 2] = [("tool", "show"), ("tool", "call")];

/// The commands with a server name as their first argument.
const SERVER_COMMANDS: [(&str, &str); 11] = [
    ("server", "show"),
    ("tool", "list"),
    ("tool", "show"),
    ("tool", "call"),
    ("resource", "list"),
    ("resource", "read"),
    ("prompt", "list"),
    ("prompt", "get"),
    ("auth", "login"),
    ("auth", "logout"),
    ("auth", "status"),
];

/// The help text of the server argument, as clap writes it in the zsh
/// script.
const ZSH_SERVER: &str = "server -- The server name from the configuration:";

/// The help text of the tool argument, as clap writes it in the zsh script.
const ZSH_TOOL: &str = "tool -- The tool name:";

/// The script with the server names as the values of the server argument.
/// clap writes no value list for an argument with no fixed choices, so
/// the script runs [`SERVERS`] at that place.
#[must_use]
pub fn with_server_names(shell: CompletionShell, script: &str) -> String {
    match shell {
        // In the block of each command, the words of `opts` complete the
        // first argument.
        CompletionShell::Bash => {
            let script =
                SERVER_COMMANDS
                    .iter()
                    .fold(script.to_string(), |script, (group, command)| {
                        let block = format!(
                            "ncp__subcmd__{group}__subcmd__{command})\n            opts=\""
                        );
                        script.replace(&block, &format!("{block}$({SERVERS}) "))
                    });
            // The word after the server is the tool: its names come from
            // the list cache of that server.
            TOOL_COMMANDS
                .iter()
                .fold(script, |script, (group, command)| {
                    let block = format!("ncp__subcmd__{group}__subcmd__{command})\n");
                    let Some(start) = script.find(&block) else {
                        return script;
                    };
                    let check = "            fi\n";
                    let Some(end) = script[start..].find(check).map(|at| start + at + check.len())
                    else {
                        return script;
                    };
                    let tools = "            if [[ ${COMP_CWORD} -eq 4 ]] ; then\n                COMPREPLY=( $(compgen -W \"$(ncp __complete-tools \"${COMP_WORDS[3]}\" 2>/dev/null)\" -- \"${cur}\") )\n                return 0\n            fi\n";
                    format!("{}{tools}{}", &script[..end], &script[end..])
                })
        }
        // The server argument gets an action that lists the names. The
        // function comes first, because zsh can run the script as an
        // autoload function at its end.
        CompletionShell::Zsh => {
            let script = script
                .replace(
                    &format!("{ZSH_SERVER}_default"),
                    &format!("{ZSH_SERVER}_ncp_servers"),
                )
                .replace(
                    &format!("{ZSH_TOOL}_default"),
                    &format!("{ZSH_TOOL}_ncp_tools"),
                );
            let function = format!(
                "_ncp_servers() {{\n    local -a servers\n    servers=(${{(f)\"$({SERVERS})\"}})\n    _describe 'server' servers\n}}\n\n_ncp_tools() {{\n    local -a tools\n    tools=(${{(f)\"$(ncp __complete-tools \"${{line[1]}}\" 2>/dev/null)\"}})\n    _describe 'tool' tools\n}}\n\n"
            );
            match script.split_once('\n') {
                Some((first, rest)) => format!("{first}\n\n{function}{rest}"),
                None => format!("{function}{script}"),
            }
        }
        // clap writes no line for a positional argument in fish: add one
        // for each command, for the word after the command.
        CompletionShell::Fish => {
            let script = SERVER_COMMANDS
                .iter()
                .fold(script.to_string(), |script, (group, command)| {
                    script
                        + &format!(
                            "complete -c {BIN} -n \"__fish_ncp_using_subcommand {group}; and __fish_seen_subcommand_from {command}; and test (count (commandline -opc)) -eq 3\" -f -a \"({SERVERS})\"\n"
                        )
                });
            TOOL_COMMANDS
                .iter()
                .fold(script, |script, (group, command)| {
                    script
                        + &format!(
                            "complete -c {BIN} -n \"__fish_ncp_using_subcommand {group}; and __fish_seen_subcommand_from {command}; and test (count (commandline -opc)) -eq 4\" -f -a \"(ncp __complete-tools (commandline -opc)[4] 2>/dev/null)\"\n"
                        )
                })
        }
    }
}

/// Writes the configured server names, one on each line. A configuration
/// that cannot be read gives no names and no error.
pub fn servers(env: &Env) {
    let names = Paths::from_env(env)
        .ok()
        .and_then(|paths| std::fs::read_to_string(paths.config_file).ok())
        .and_then(|text| config::parse(&text, env).ok())
        .map(|config| {
            let mut names: Vec<String> = config
                .servers()
                .map(|server| server.name.to_string())
                // An invalid entry can have any key. A shell expands each
                // word of the list again, so only a name that passes the
                // rule of server names (no shell characters) goes out.
                .chain(
                    config
                        .invalid()
                        .filter_map(|(name, _)| crate::server::ServerName::parse(name).ok())
                        .map(|name| name.to_string()),
                )
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    let text = names
        .iter()
        .fold(String::new(), |text, name| text + name + "\n");
    let _ = ui::write_stdout(&text);
}

/// True for a tool name that a shell cannot expand: the characters that
/// the spec names for tool names. A tool name comes from a server, and a
/// shell reads the list of names as words again.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Writes the tool names of the server from the list cache, one on each
/// line, also from an old list: a name is only a proposal. Nothing goes
/// out for an unknown server or with no cache.
pub fn tools(env: &Env, server: &str) {
    let names = Paths::from_env(env)
        .ok()
        .zip(crate::server::ServerName::parse(server).ok())
        .and_then(|(paths, server)| {
            crate::list_cache::load(&crate::list_cache::path(&paths.cache_dir, &server))
        })
        .map(|cached| {
            let mut names: Vec<String> = cached
                .tools
                .iter()
                .map(|tool| tool.name.to_string())
                .filter(|name| is_plain_name(name))
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    let text = names
        .iter()
        .fold(String::new(), |text, name| text + name + "\n");
    let _ = ui::write_stdout(&text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_tool_names_complete() {
        assert!(is_plain_name("search_issues"));
        assert!(is_plain_name("a.b-c"));
        assert!(!is_plain_name("$(touch x)"));
        assert!(!is_plain_name("a b"));
        assert!(!is_plain_name(""));
    }
}
