//! The stdio server process: start and stop.
//!
//! The server starts in its own process group. On Ctrl-C the terminal sends
//! SIGINT to its foreground group, and a server in that group stops before
//! `ncp` can send the cancel request. Thus `ncp` must stop the server itself,
//! as the spec describes: close stdin, wait, SIGTERM, wait, SIGKILL
//! (MCP 2026-07-28, basic/transports/stdio, "Shutdown").

use std::{collections::BTreeMap, process::Stdio, sync::Arc, time::Duration};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop, ProcessGroup};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
};

use crate::{
    config::{Expanded, StdioServer},
    expand::Env,
};

/// The wait for the last log lines of a server that stopped.
const LOG_WAIT: Duration = Duration::from_millis(500);

/// The wait for the exit status of a server whose handshake failed.
const EXIT_WAIT: Duration = Duration::from_millis(500);

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

#[derive(Clone)]
pub enum ServerLog {
    /// To the stderr of `ncp`, with no change.
    Inherit,
    /// Each line through the function, which gives the text to write to
    /// the stderr of `ncp`. Thus the caller can keep the JSON Lines form of
    /// its own stderr.
    Lines(Arc<dyn Fn(&str) -> String + Send + Sync>),
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
    log_task: Option<tokio::task::JoinHandle<()>>,
}

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
    // The task ends when the server closes its stderr.
    let log_task = match (log, child.stderr().take()) {
        (ServerLog::Lines(write), Some(stderr)) => {
            Some(tokio::spawn(log_lines(stderr, write.clone())))
        }
        _ => None,
    };
    let stdout = child.stdout().take();
    let stdin = child.stdin().take();
    match (stdout, stdin) {
        (Some(stdout), Some(stdin)) => Ok((Process { child, log_task }, stdout, stdin)),
        _ => Err(std::io::Error::other(
            "The server process has no stdio pipes",
        )),
    }
}

// Gives each stderr line of the server to `write`, until the server closes
// stderr. A line that is not UTF-8 must not stop the reader: then the server
// gets an error on its next write. A long line comes in parts of the limit,
// cut between characters.
async fn log_lines(
    stderr: tokio::process::ChildStderr,
    format: Arc<dyn Fn(&str) -> String + Send + Sync>,
) {
    // An async write: when the caller of `ncp` does not read its stderr,
    // this task waits, and the time limits of the runtime still work.
    let mut out = tokio::io::stderr();
    let mut write = async |line: &str| {
        let _ = out.write_all(format(line).as_bytes()).await;
    };
    let mut reader = BufReader::new(stderr);
    let mut part = Vec::new();
    let mut continued = false;
    loop {
        let room = MAX_LOG_LINE.saturating_sub(part.len() as u64).max(1);
        let read = (&mut reader).take(room).read_until(b'\n', &mut part).await;
        if !read.is_ok_and(|read| read > 0) {
            if !part.is_empty() {
                write(&String::from_utf8_lossy(&part)).await;
            }
            let _ = out.flush().await;
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
            write(text).await;
        }
        continued = !ends;
        part.drain(..cut);
    }
}

// The number of bytes at the end that start a character with no end.
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

    /// The exit status, when the process ended or ends within a short
    /// wait. A server that stops at its start closes its stdout first, so
    /// its status can come a moment after the handshake fails.
    pub async fn exit_status(&mut self) -> Option<std::process::ExitStatus> {
        tokio::time::timeout(EXIT_WAIT, self.child.wait())
            .await
            .ok()?
            .ok()
    }

    /// Stops the process group. The caller closes stdin first. Then the
    /// last lines of its log go out: they often say why a server stopped.
    pub async fn stop(mut self) {
        let exited = self.wait_for_exit().await
            || (self.child.signal(SIGTERM).is_ok() && self.wait_for_exit().await);
        if !exited {
            let _ = Box::into_pin(self.child.kill()).await;
        }
        if let Some(task) = self.log_task.take() {
            // A child process of the server can keep stderr open.
            let _ = tokio::time::timeout(LOG_WAIT, task).await;
        }
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
