//! The stdio server process: start and stop.
//!
//! The server starts in its own process group. On Ctrl-C the terminal sends
//! SIGINT to its foreground group, and a server in that group stops before
//! `ncp` can send the cancel request. Thus `ncp` must stop the server itself,
//! as the spec describes: close stdin, wait, SIGTERM, wait, SIGKILL
//! (MCP 2026-07-28, basic/lifecycle, "Shutdown", stdio).

use std::{collections::BTreeMap, process::Stdio, sync::Arc, time::Duration};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop, ProcessGroup};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{ChildStdin, ChildStdout},
};

use crate::{
    config::{Expanded, StdioServer},
    expand::Env,
};

/// Wait time after each stop step.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// The longest part of a stderr line of a server that goes out as one line.
const MAX_LOG_LINE: u64 = 64 * 1024;

/// SIGTERM has the number 15 on Linux and macOS.
const SIGTERM: i32 = 15;

/// The environment of the server process. The server gets the environment
/// of `ncp` (with the proxy variables), then the expanded `env` of its
/// entry. A later value replaces an earlier value with the same name.
#[must_use]
pub fn child_env(base: &Env, env: &BTreeMap<String, Expanded>) -> BTreeMap<String, String> {
    base.iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .chain(
            env.iter()
                .map(|(name, expanded)| (name.clone(), expanded.value.clone())),
        )
        .collect()
}

/// Where the stderr of a server goes.
#[derive(Clone)]
pub enum ServerLog {
    /// To the stderr of `ncp`, with no change.
    Inherit,
    /// Each line to the function. Thus the caller can keep the JSON Lines
    /// form of its own stderr.
    Lines(Arc<dyn Fn(&str) + Send + Sync>),
}

impl std::fmt::Debug for ServerLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inherit => f.write_str("Inherit"),
            Self::Lines(_) => f.write_str("Lines"),
        }
    }
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
    log: &ServerLog,
) -> std::io::Result<(Process, ChildStdout, ChildStdin)> {
    let mut command = tokio::process::Command::new(&server.command);
    command
        .args(&server.args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(match log {
            ServerLog::Inherit => Stdio::inherit(),
            ServerLog::Lines(_) => Stdio::piped(),
        });
    if let Some(cwd) = &server.cwd {
        command.current_dir(cwd);
    }
    let mut child = CommandWrap::from(command)
        .wrap(ProcessGroup::leader())
        .wrap(KillOnDrop)
        .spawn()?;
    if let (ServerLog::Lines(write), Some(stderr)) = (log, child.stderr().take()) {
        // The task ends when the server closes its stderr.
        tokio::spawn(log_lines(stderr, write.clone()));
    }
    let stdout = child.stdout().take();
    let stdin = child.stdin().take();
    match (stdout, stdin) {
        (Some(stdout), Some(stdin)) => Ok((Process { child }, stdout, stdin)),
        _ => Err(std::io::Error::other("server process has no stdio pipes")),
    }
}

/// Gives each stderr line of the server to `write`, until the server closes
/// stderr. A line that is not UTF-8 must not stop the reader: the server
/// would then get an error on its next write. A long line comes in parts of
/// the limit, cut between characters.
async fn log_lines(stderr: tokio::process::ChildStderr, write: Arc<dyn Fn(&str) + Send + Sync>) {
    let mut reader = BufReader::new(stderr);
    let mut part = Vec::new();
    let mut continued = false;
    loop {
        let room = MAX_LOG_LINE.saturating_sub(part.len() as u64).max(1);
        let read = (&mut reader).take(room).read_until(b'\n', &mut part).await;
        if !read.is_ok_and(|read| read > 0) {
            if !part.is_empty() {
                write(&String::from_utf8_lossy(&part));
            }
            return;
        }
        let ends = part.last() == Some(&b'\n');
        let cut = if ends {
            part.len()
        } else {
            part.len() - cut_character(&part)
        };
        let text = String::from_utf8_lossy(&part[..cut]);
        let text = text.strip_suffix('\n').unwrap_or(&text);
        // The new line after a part of exactly the limit is not a line.
        if !(continued && ends && text.is_empty()) {
            write(text);
        }
        continued = !ends;
        part.drain(..cut);
    }
}

/// The number of bytes at the end that start a character with no end.
fn cut_character(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Err(error) if error.error_len().is_none() => bytes.len() - error.valid_up_to(),
        _ => 0,
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
    fn finds_the_bytes_of_a_character_that_a_cut_split() {
        assert_eq!(cut_character(b"abc"), 0);
        assert_eq!(cut_character("aé".as_bytes()), 0);
        assert_eq!(cut_character(&"aé".as_bytes()[..2]), 1);
        assert_eq!(cut_character(b"a\xffb"), 0);
    }

    #[test]
    fn child_env_puts_entry_env_over_base_env() {
        let base = Env::from([
            ("HTTPS_PROXY".to_string(), "http://proxy:3128".to_string()),
            ("A".to_string(), "base".to_string()),
        ]);
        let env = BTreeMap::from([(
            "A".to_string(),
            Expanded {
                value: "entry".into(),
                from_variable: true,
            },
        )]);
        let child = child_env(&base, &env);
        assert_eq!(child["HTTPS_PROXY"], "http://proxy:3128");
        assert_eq!(child["A"], "entry");
    }
}
