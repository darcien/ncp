//! The login Keychain through the `security` program of macOS.
//!
//! `ncp` has no paid Developer ID, so each update gives the binary a new
//! code identity. The Keychain trusts the program that made an item, so
//! through the native API it asks again for each item after an update. An
//! agent cannot answer that dialog. An item that `/usr/bin/security` makes
//! trusts `/usr/bin/security`, which an update of `ncp` does not change. gh
//! uses the same method. The cost: a process of the same user can read the
//! items of `ncp` through `security` with no prompt.
//!
//! The value goes to `security -i` through stdin and never in a process
//! argument, because each user of the machine can see the arguments of a
//! process. All calls block. An async caller runs them through
//! [`super::run_blocking`].
//!
//! This code has no macOS dependency, so the tests run with a fake program
//! on each platform.

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use base64::{Engine, engine::general_purpose::STANDARD};

use super::{CredentialStore, StoreError};
use crate::credential::{CredentialKey, SERVICE, Secret};

const PROGRAM: &str = "/usr/bin/security";

/// The mark of an encoded value. `security -i` reads one command on each
/// line, and `find-generic-password -w` shows a value that is not printable
/// ASCII as hexadecimal digits. The encoded value has only printable ASCII
/// and no quote, so each value is safe in the command and in the output.
const ENCODED: &str = "ncp-base64:";

/// The longest command line, with its new line, that `security -i` reads
/// as one command. It reads each line with `fgets` into a buffer of 4096
/// bytes, and runs the rest of a longer line as a new command.
const MAX_LINE: usize = 4095;

// `security` exits with the low byte of the status of the Security
// framework. For example errSecItemNotFound (-25300) gives 44.
const ITEM_NOT_FOUND: i32 = 44;
const INTERACTION_NOT_ALLOWED: i32 = 36;
const NO_DEFAULT_KEYCHAIN: i32 = 37;
const NO_SUCH_KEYCHAIN: i32 = 50;
const AUTH_FAILED: i32 = 51;
const USER_CANCELED: i32 = 128;

/// A credential store that runs the `security` program.
#[derive(Debug, Clone)]
pub struct SecurityStore {
    program: PathBuf,
}

impl SecurityStore {
    /// The store that runs `/usr/bin/security`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_program(PROGRAM)
    }

    /// The store that runs a different program with the same interface.
    /// Tests give a fake program.
    #[must_use]
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    fn run(&self, args: &[&str], input: Option<&str>) -> Result<Output, StoreError> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| not_started(&self.program, &error))?;
        // Close stdin after the input, so that `security -i` reads its end.
        let sent = match (input, child.stdin.take()) {
            (Some(text), Some(mut stdin)) => stdin.write_all(text.as_bytes()),
            _ => Ok(()),
        };
        // Wait also when the input failed, so that no stopped process stays.
        let output = child
            .wait_with_output()
            .map_err(|error| StoreError::Failure(format!("{}: {error}", self.program.display())))?;
        sent.map_err(|error| {
            StoreError::Failure(format!(
                "cannot send the command to {}: {error}",
                self.program.display()
            ))
        })?;
        Ok(output)
    }
}

impl Default for SecurityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for SecurityStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        let account = key.account();
        let output = self.run(
            &["find-generic-password", "-s", SERVICE, "-a", &account, "-w"],
            None,
        )?;
        match output.status.code() {
            Some(0) => decode(&output.stdout).map(Some),
            Some(ITEM_NOT_FOUND) => Ok(None),
            code => Err(exit_error(
                "find-generic-password",
                code,
                &diagnostics(&output),
            )),
        }
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        let line = add_command(&key.account(), secret.expose())?;
        let output = self.run(&["-i"], Some(&line))?;
        // The error does not show stderr: after a parse error, it can hold
        // the command, and the command holds the value.
        match output.status.code() {
            Some(0) => Ok(()),
            code => Err(exit_error("add-generic-password", code, "")),
        }
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), StoreError> {
        let account = key.account();
        let output = self.run(
            &["delete-generic-password", "-s", SERVICE, "-a", &account],
            None,
        )?;
        match output.status.code() {
            Some(0 | ITEM_NOT_FOUND) => Ok(()),
            code => Err(exit_error(
                "delete-generic-password",
                code,
                &diagnostics(&output),
            )),
        }
    }
}

fn not_started(program: &Path, error: &io::Error) -> StoreError {
    StoreError::Unavailable(format!("cannot run {}: {error}", program.display()))
}

/// The command for `security -i` that adds or replaces an item.
fn add_command(account: &str, value: &str) -> Result<String, StoreError> {
    let line = format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quote(SERVICE),
        quote(account),
        quote(&encode(value))
    );
    if line.len() <= MAX_LINE {
        Ok(line)
    } else {
        Err(StoreError::Failure(format!(
            "the secret is too long for the security program: the command has {} bytes, and the limit is {MAX_LINE}",
            line.len()
        )))
    }
}

/// Quotes a word for `security -i`. In a word that starts with a double
/// quote, a double quote ends the word and a backslash keeps the next
/// character as it is. A new line ends the command: an account has no new
/// line, and an encoded value has none.
fn quote(word: &str) -> String {
    let escaped = word.chars().fold(String::new(), |mut escaped, c| {
        if matches!(c, '"' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
        escaped
    });
    format!("\"{escaped}\"")
}

fn encode(value: &str) -> String {
    format!("{ENCODED}{}", STANDARD.encode(value))
}

/// Reads the output of `find-generic-password -w`. A value with no mark is
/// plain text, for example an item that a person added by hand.
fn decode(stdout: &[u8]) -> Result<Secret, StoreError> {
    let not_text = || StoreError::Failure("stored item is not UTF-8 text".into());
    let text = std::str::from_utf8(stdout).map_err(|_| not_text())?;
    let stored = text.strip_suffix('\n').unwrap_or(text);
    match stored.strip_prefix(ENCODED) {
        Some(encoded) => STANDARD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(Secret::new)
            .ok_or_else(not_text),
        None => Ok(Secret::new(stored.to_string())),
    }
}

/// The first line of stderr. `find-generic-password` and
/// `delete-generic-password` write no value to stderr.
fn diagnostics(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn exit_error(command: &str, code: Option<i32>, diagnostics: &str) -> StoreError {
    let Some(code) = code else {
        return StoreError::Failure(format!("security {command} stopped on a signal"));
    };
    let detail = if diagnostics.is_empty() {
        format!("security {command} exited with code {code}")
    } else {
        format!("security {command} exited with code {code}: {diagnostics}")
    };
    match code {
        INTERACTION_NOT_ALLOWED | AUTH_FAILED | USER_CANCELED => StoreError::Denied(detail),
        NO_DEFAULT_KEYCHAIN | NO_SUCH_KEYCHAIN => StoreError::Unavailable(detail),
        _ => StoreError::Failure(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_a_word_with_quotes_and_backslashes() {
        assert_eq!(quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }

    #[test]
    fn sends_the_value_only_in_encoded_form() {
        let line = add_command("a/oauth/tokens", "he said \"hi\"\n").unwrap();
        assert_eq!(
            line,
            "add-generic-password -U -s \"ncp\" -a \"a/oauth/tokens\" -w \"ncp-base64:aGUgc2FpZCAiaGkiCg==\"\n"
        );
    }

    #[test]
    fn refuses_a_command_longer_than_one_line_of_security() {
        let fits = |len: usize| add_command("a/oauth/tokens", &"x".repeat(len)).is_ok();
        let longest = (0..MAX_LINE).take_while(|&len| fits(len)).last().unwrap();
        let line = add_command("a/oauth/tokens", &"x".repeat(longest)).unwrap();
        assert!(line.len() <= MAX_LINE);
        assert!(line.len() > MAX_LINE - 4);
        let Err(StoreError::Failure(message)) = add_command("a/oauth/tokens", &"x".repeat(4000))
        else {
            panic!("a long value must fail");
        };
        assert!(!message.contains("xxxx"));
    }

    #[test]
    fn decodes_an_encoded_value() {
        let stored = format!("{}\n", encode("v \"q\"\n\u{fc}"));
        assert_eq!(
            decode(stored.as_bytes()).unwrap(),
            Secret::new("v \"q\"\n\u{fc}".into())
        );
    }

    #[test]
    fn reads_a_value_with_no_mark_as_plain_text() {
        assert_eq!(
            decode(b"plain value\n").unwrap(),
            Secret::new("plain value".into())
        );
    }

    #[test]
    fn reports_an_encoded_value_that_is_not_valid_without_the_value() {
        let Err(StoreError::Failure(message)) = decode(b"ncp-base64:@@secret@@\n") else {
            panic!("a bad value must fail");
        };
        assert!(!message.contains("secret@@"));
    }

    #[test]
    fn maps_exit_codes_to_store_errors() {
        assert!(matches!(
            exit_error("find-generic-password", Some(36), ""),
            StoreError::Denied(_)
        ));
        assert!(matches!(
            exit_error("find-generic-password", Some(128), ""),
            StoreError::Denied(_)
        ));
        assert!(matches!(
            exit_error("find-generic-password", Some(50), ""),
            StoreError::Unavailable(_)
        ));
        assert!(matches!(
            exit_error("find-generic-password", Some(1), "x"),
            StoreError::Failure(_)
        ));
        assert!(matches!(
            exit_error("find-generic-password", None, ""),
            StoreError::Failure(_)
        ));
    }
}
