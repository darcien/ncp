//! The stdio server process: start and stop.
//!
//! The server starts in its own process group. On Ctrl-C the terminal sends
//! SIGINT to its foreground group, and a server in that group stops before
//! `ncp` can send the cancel request. Thus `ncp` must stop the server itself,
//! as the spec describes: close stdin, wait, SIGTERM, wait, SIGKILL
//! (MCP 2026-07-28, basic/lifecycle, "Shutdown", stdio).

use std::{collections::BTreeMap, process::Stdio, time::Duration};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop, ProcessGroup};
use tokio::process::{ChildStdin, ChildStdout};

use crate::{
    config::StdioServer,
    credential::{EnvName, Secret},
    expand::Env,
};

/// Wait time after each stop step.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// SIGTERM has the number 15 on Linux and macOS.
const SIGTERM: i32 = 15;

/// The environment of the server process. The server gets the environment
/// of `ncp` (with the proxy variables), then the `env` of its entry, then
/// its secrets from the credential store.
#[must_use]
pub fn child_env(
    base: &Env,
    config: &BTreeMap<String, String>,
    secrets: &BTreeMap<EnvName, Secret>,
) -> BTreeMap<String, String> {
    base.iter()
        .chain(config.iter())
        .map(|(name, value)| (name.clone(), value.clone()))
        .chain(
            secrets
                .iter()
                .map(|(name, secret)| (name.as_str().to_string(), secret.expose().to_string())),
        )
        .collect()
}

pub struct Process {
    child: Box<dyn ChildWrapper>,
}

/// Starts the server. Returns the process and its stdout and stdin.
///
/// # Errors
///
/// Returns an error when the process cannot start.
pub fn spawn(
    server: &StdioServer,
    env: &BTreeMap<String, String>,
) -> std::io::Result<(Process, ChildStdout, ChildStdin)> {
    let mut command = tokio::process::Command::new(&server.command);
    command
        .args(&server.args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(cwd) = &server.cwd {
        command.current_dir(cwd);
    }
    let mut child = CommandWrap::from(command)
        .wrap(ProcessGroup::leader())
        .wrap(KillOnDrop)
        .spawn()?;
    let stdout = child.stdout().take();
    let stdin = child.stdin().take();
    match (stdout, stdin) {
        (Some(stdout), Some(stdin)) => Ok((Process { child }, stdout, stdin)),
        _ => Err(std::io::Error::other("server process has no stdio pipes")),
    }
}

impl Process {
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Stops the process group. The caller closes stdin first.
    pub async fn stop(mut self) {
        if self.wait_for_exit().await {
            return;
        }
        if self.child.signal(SIGTERM).is_ok() && self.wait_for_exit().await {
            return;
        }
        let _ = Box::into_pin(self.child.kill()).await;
    }

    async fn wait_for_exit(&mut self) -> bool {
        tokio::time::timeout(STOP_WAIT, self.child.wait())
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_env_puts_secrets_over_entry_env_over_base_env() {
        let base = Env::from([
            ("HTTPS_PROXY".to_string(), "http://proxy:3128".to_string()),
            ("A".to_string(), "base".to_string()),
            ("B".to_string(), "base".to_string()),
        ]);
        let config = BTreeMap::from([
            ("A".to_string(), "entry".to_string()),
            ("B".to_string(), "entry".to_string()),
        ]);
        let secrets =
            BTreeMap::from([(EnvName::parse("B").unwrap(), Secret::new("secret".into()))]);
        let env = child_env(&base, &config, &secrets);
        assert_eq!(env["HTTPS_PROXY"], "http://proxy:3128");
        assert_eq!(env["A"], "entry");
        assert_eq!(env["B"], "secret");
    }
}
