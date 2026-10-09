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
//! `security -i` reads one command line of at most 4095 bytes, and some
//! token sets are longer. Such a value goes into parts, one item each, and
//! a head item names the parts. One `add-generic-password -U` changes one
//! item atomically, so the head is the commit point of a save.
//!
//! This code has no macOS dependency, so the tests run with a fake program
//! on each platform.

use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

/// The mark of a head: the item that names the parts of a long value.
const PARTS: &str = "ncp-parts:";

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

/// The time that one run of the program gets. It is shorter than the
/// limit of each store call, so that `ncp` stops the program before the
/// caller gives up: a program that stays could show a dialog after the
/// exit of `ncp`, or write an item late.
pub const PROGRAM_LIMIT: Duration = Duration::from_secs(55);

/// A credential store that runs the `security` program.
#[derive(Debug, Clone)]
pub struct SecurityStore {
    program: PathBuf,
    limit: Duration,
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
            limit: PROGRAM_LIMIT,
        }
    }

    /// The same store with another time limit for each run. Tests use a
    /// short limit.
    #[must_use]
    pub fn with_limit(self, limit: Duration) -> Self {
        Self { limit, ..self }
    }

    fn run(
        &self,
        args: &[&str],
        input: Option<&str>,
        deadline: Instant,
    ) -> Result<Output, StoreError> {
        if Instant::now() >= deadline {
            return Err(StoreError::NoAnswer(self.limit));
        }
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
        // Read the output on other threads, so that a full pipe cannot stop
        // the program while this thread waits for it.
        let stdout = read_all(child.stdout.take());
        let stderr = read_all(child.stderr.take());
        let status = self.wait(&mut child, deadline)?;
        sent.map_err(|error| {
            StoreError::Failure(format!(
                "cannot send the command to {}: {error}",
                self.program.display()
            ))
        })?;
        Ok(Output {
            status,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }

    /// Waits for the program until the deadline, then stops it.
    fn wait(&self, child: &mut Child, deadline: Instant) -> Result<ExitStatus, StoreError> {
        let failure =
            |error: io::Error| StoreError::Failure(format!("{}: {error}", self.program.display()));
        loop {
            if let Some(status) = child.try_wait().map_err(failure)? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(StoreError::NoAnswer(self.limit));
            }
            std::thread::sleep(WAIT_STEP);
        }
    }
}

/// The time between two checks for the end of the program.
const WAIT_STEP: Duration = Duration::from_millis(10);

fn read_all(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

impl Default for SecurityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for SecurityStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        let deadline = Instant::now() + self.limit;
        let account = key.account();
        match self.load_value(&account, deadline)? {
            Loaded::Found(secret) => Ok(Some(secret)),
            Loaded::Absent => Ok(None),
            // Readers do not hold the refresh lock. Between the read of the
            // head and the read of a part, another process can save a new
            // value and remove the old parts. The head then names the new
            // parts.
            Loaded::PartMissing => match self.load_value(&account, deadline)? {
                Loaded::Found(secret) => Ok(Some(secret)),
                Loaded::Absent => Ok(None),
                Loaded::PartMissing => Err(StoreError::Failure(format!(
                    "the item {account} is kept in parts, and a part is missing"
                ))),
            },
        }
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        let deadline = Instant::now() + self.limit;
        let account = key.account();
        let plan = save_plan(&account, secret.expose(), &new_generation())?;
        plan.parts
            .iter()
            .try_for_each(|(part, text)| self.add(part, text, deadline))?;
        // The head is the commit point. A head that `ncp` writes after it
        // reports a failure would change the value after the failure, so
        // the save stops while the head can still finish in time.
        if deadline.saturating_duration_since(Instant::now()) < self.limit / HEAD_RESERVE {
            return Err(StoreError::NoAnswer(self.limit));
        }
        self.add(&account, &plan.head, deadline)?;
        self.remove_other_parts(&account, &plan.parts, deadline);
        Ok(())
    }

    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
        let deadline = Instant::now() + self.limit;
        let account = key.account();
        let parts: Vec<String> = self
            .accounts(deadline)?
            .into_iter()
            .filter(|other| is_part_of(&account, other))
            .collect();
        // The head goes first, so that no reader finds a head with a
        // missing part.
        let head = self.remove(&account, deadline)?;
        parts.iter().try_fold(head, |found, part| {
            self.remove(part, deadline).map(|removed| found || removed)
        })
    }

    fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
        let accounts = self.accounts(Instant::now() + self.limit)?;
        Ok(super::keys_of(accounts.iter().map(String::as_str)))
    }
}

/// The part of the limit that the head of a save keeps for itself.
const HEAD_RESERVE: u32 = 10;

/// The result of one read of a value.
enum Loaded {
    Found(Secret),
    Absent,
    PartMissing,
}

impl SecurityStore {
    fn load_value(&self, account: &str, deadline: Instant) -> Result<Loaded, StoreError> {
        let Some(text) = self.find(account, deadline)? else {
            return Ok(Loaded::Absent);
        };
        match parse_item(account, &text)? {
            Item::Whole(stored) => decode(&stored).map(Loaded::Found),
            Item::Head { generation, count } => {
                let parts = (0..count)
                    .map(|index| self.find(&part_account(account, &generation, index), deadline))
                    .collect::<Result<Vec<_>, _>>()?;
                match parts.into_iter().collect::<Option<String>>() {
                    Some(stored) => decode(&stored).map(Loaded::Found),
                    None => Ok(Loaded::PartMissing),
                }
            }
        }
    }

    /// The stored text of one item.
    fn find(&self, account: &str, deadline: Instant) -> Result<Option<String>, StoreError> {
        let output = self.run(
            &["find-generic-password", "-s", SERVICE, "-a", account, "-w"],
            None,
            deadline,
        )?;
        match output.status.code() {
            Some(0) => {
                let text = String::from_utf8(output.stdout)
                    .map_err(|_| StoreError::Failure("stored item is not UTF-8 text".into()))?;
                Ok(Some(text.strip_suffix('\n').unwrap_or(&text).to_string()))
            }
            Some(ITEM_NOT_FOUND) => Ok(None),
            code => Err(exit_error(
                "find-generic-password",
                code,
                &diagnostics(&output),
            )),
        }
    }

    /// Adds or replaces one item. Each item has its own run, so that each
    /// exit code is known.
    fn add(&self, account: &str, text: &str, deadline: Instant) -> Result<(), StoreError> {
        let output = self.run(&["-i"], Some(&add_line(account, text)), deadline)?;
        // The error does not show stderr: after a parse error, it can hold
        // the command, and the command holds the value.
        match output.status.code() {
            Some(0) => Ok(()),
            code => Err(exit_error("add-generic-password", code, "")),
        }
    }

    fn remove(&self, account: &str, deadline: Instant) -> Result<bool, StoreError> {
        let output = self.run(
            &["delete-generic-password", "-s", SERVICE, "-a", account],
            None,
            deadline,
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(ITEM_NOT_FOUND) => Ok(false),
            code => Err(exit_error(
                "delete-generic-password",
                code,
                &diagnostics(&output),
            )),
        }
    }

    /// The accounts of all `ncp` items. `dump-keychain` with no `-d` shows
    /// the attributes of each item and no value, so it decrypts nothing.
    fn accounts(&self, deadline: Instant) -> Result<Vec<String>, StoreError> {
        let output = self.run(&["dump-keychain"], None, deadline)?;
        match output.status.code() {
            Some(0) => Ok(ncp_accounts(&String::from_utf8_lossy(&output.stdout))),
            code => Err(exit_error("dump-keychain", code, &diagnostics(&output))),
        }
    }

    /// Removes the parts of older saves, and the parts of saves that
    /// stopped before their head, so that no old token data stays. A
    /// generation that is younger than the limit of a save can belong to a
    /// save that still runs, so it stays until a later save. A failure here
    /// keeps the new value, and the next save removes the rest.
    fn remove_other_parts(&self, account: &str, keep: &[(String, String)], deadline: Instant) {
        let Ok(accounts) = self.accounts(deadline) else {
            return;
        };
        let now = nanos_now();
        let limit = self.limit.as_nanos();
        let _ = accounts
            .iter()
            .filter(|other| is_part_of(account, other))
            .filter(|other| !keep.iter().any(|(part, _)| part == *other))
            .filter(|other| {
                generation_time(account, other).is_none_or(|time| now.saturating_sub(time) > limit)
            })
            .try_for_each(|other| self.remove(other, deadline).map(|_| ()));
    }
}

/// What one item holds.
#[derive(Debug, PartialEq, Eq)]
enum Item {
    /// The whole stored value.
    Whole(String),
    /// The head of a value in parts. The parts are the items
    /// `ACCOUNT/GENERATION/0` up to `ACCOUNT/GENERATION/{count - 1}`.
    Head { generation: String, count: usize },
}

fn parse_item(account: &str, text: &str) -> Result<Item, StoreError> {
    let Some(head) = text.strip_prefix(PARTS) else {
        return Ok(Item::Whole(text.to_string()));
    };
    head.split_once(':')
        .and_then(|(generation, count)| {
            let count = count.parse().ok()?;
            Some(Item::Head {
                generation: generation.to_string(),
                count,
            })
        })
        .ok_or_else(|| StoreError::Failure(format!("the item {account} is not valid")))
}

/// The items that one save writes: the parts in order, then the head.
#[derive(Debug, PartialEq, Eq)]
struct SavePlan {
    parts: Vec<(String, String)>,
    head: String,
}

/// Cuts the encoded value into parts when its command does not fit one
/// line. The parts and the head have no second encoding: the encoded text
/// has only printable ASCII and no quote, and a head starts with a mark
/// that no encoded value has.
fn save_plan(account: &str, value: &str, generation: &str) -> Result<SavePlan, StoreError> {
    let encoded = encode(value);
    if add_line(account, &encoded).len() <= MAX_LINE {
        return Ok(SavePlan {
            parts: Vec::new(),
            head: encoded,
        });
    }
    // The longest part account has the largest index.
    let room = MAX_LINE
        .checked_sub(add_line(&part_account(account, generation, usize::MAX), "").len())
        .filter(|&room| room > 0)
        .ok_or_else(|| StoreError::Failure(format!("the account {account} is too long")))?;
    let parts: Vec<(String, String)> = encoded
        .as_bytes()
        .chunks(room)
        .map(String::from_utf8_lossy)
        .enumerate()
        .map(|(index, text)| (part_account(account, generation, index), text.into_owned()))
        .collect();
    let head = format!("{PARTS}{generation}:{}", parts.len());
    Ok(SavePlan { parts, head })
}

fn part_account(account: &str, generation: &str, index: usize) -> String {
    format!("{account}/{generation}/{index}")
}

fn is_part_of(account: &str, other: &str) -> bool {
    other
        .strip_prefix(account)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// A new generation for each save, so that the parts of two saves never
/// mix: the time in nanoseconds and the process id.
fn new_generation() -> String {
    format!("{:x}-{:x}", nanos_now(), std::process::id())
}

fn nanos_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos())
}

/// The time of the generation of a part account `ACCOUNT/GEN/INDEX`, or
/// `None` when the account has no generation of `ncp`.
fn generation_time(account: &str, part: &str) -> Option<u128> {
    let (generation, _) = part
        .strip_prefix(account)?
        .strip_prefix('/')?
        .split_once('/')?;
    let (time, _) = generation.split_once('-')?;
    u128::from_str_radix(time, 16).ok()
}

/// The accounts of the generic password items of service `ncp` in the
/// output of `dump-keychain`. Each item starts with a `keychain:` line, and
/// has one attribute on each line, for example `"acct"<blob>="secret/A"`.
/// A value with bytes that are not printable shows as hexadecimal digits;
/// `ncp` makes no such account, so such a value is left out.
fn ncp_accounts(dump: &str) -> Vec<String> {
    let attribute = |line: &str, name: &str| {
        line.trim_start()
            .strip_prefix(&format!("\"{name}\"<blob>=\""))
            .and_then(|rest| rest.strip_suffix('"'))
            .map(str::to_string)
    };
    dump.split("keychain: ")
        .filter_map(|item| {
            let service = item.lines().find_map(|line| attribute(line, "svce"))?;
            let account = item.lines().find_map(|line| attribute(line, "acct"))?;
            (service == SERVICE).then_some(account)
        })
        .collect()
}

fn not_started(program: &Path, error: &io::Error) -> StoreError {
    StoreError::Unavailable(format!("cannot run {}: {error}", program.display()))
}

/// The command for `security -i` that adds or replaces an item.
fn add_line(account: &str, text: &str) -> String {
    format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quote(SERVICE),
        quote(account),
        quote(text)
    )
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

/// Reads a stored value. A value with no mark is plain text, for example an
/// item that a person added by hand.
fn decode(stored: &str) -> Result<Secret, StoreError> {
    let not_text = || StoreError::Failure("stored item is not UTF-8 text".into());
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
        let plan = save_plan("oauth/a/tokens", "he said \"hi\"\n", "g").unwrap();
        assert_eq!(plan.parts, []);
        assert_eq!(
            add_line("oauth/a/tokens", &plan.head),
            "add-generic-password -U -s \"ncp\" -a \"oauth/a/tokens\" -w \"ncp-base64:aGUgc2FpZCAiaGkiCg==\"\n"
        );
    }

    #[test]
    fn keeps_a_value_in_one_item_while_its_command_fits_one_line() {
        let fits = |len: usize| {
            save_plan("oauth/a/tokens", &"x".repeat(len), "g")
                .unwrap()
                .parts
                .is_empty()
        };
        let longest = (0..MAX_LINE).take_while(|&len| fits(len)).last().unwrap();
        let plan = save_plan("oauth/a/tokens", &"x".repeat(longest), "g").unwrap();
        let line = add_line("oauth/a/tokens", &plan.head);
        assert!(line.len() <= MAX_LINE);
        assert!(line.len() > MAX_LINE - 4);
    }

    #[test]
    fn cuts_a_long_value_into_parts_that_each_fit_one_line() {
        let value = "x".repeat(10_000);
        let plan = save_plan("oauth/a/tokens", &value, "g1").unwrap();
        assert_eq!(plan.parts.len(), 4);
        assert_eq!(plan.head, "ncp-parts:g1:4");
        for (index, (account, text)) in plan.parts.iter().enumerate() {
            assert_eq!(account, &format!("oauth/a/tokens/g1/{index}"));
            assert!(add_line(account, text).len() <= MAX_LINE);
        }
        let joined: String = plan.parts.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, encode(&value));
    }

    #[test]
    fn reads_a_head_and_refuses_a_head_that_is_not_valid() {
        assert_eq!(
            parse_item("a", "ncp-parts:g1:3").unwrap(),
            Item::Head {
                generation: "g1".into(),
                count: 3
            }
        );
        assert_eq!(
            parse_item("a", "ncp-base64:eA==").unwrap(),
            Item::Whole("ncp-base64:eA==".into())
        );
        assert!(parse_item("a", "ncp-parts:g1").is_err());
        assert!(parse_item("a", "ncp-parts:g1:x").is_err());
    }

    #[test]
    fn finds_the_parts_of_an_account_only() {
        assert!(is_part_of("secret/A", "secret/A/g/0"));
        assert!(!is_part_of("secret/A", "secret/A"));
        assert!(!is_part_of("secret/A", "secret/AB/g/0"));
    }

    #[test]
    fn reads_the_time_of_a_generation_from_a_part_account() {
        let generation = new_generation();
        let part = part_account("oauth/a/tokens", &generation, 3);
        let time = generation_time("oauth/a/tokens", &part).unwrap();
        assert!(nanos_now() >= time);
        assert_eq!(
            generation_time("oauth/a/tokens", "oauth/a/tokens/x/0"),
            None
        );
    }

    #[test]
    fn gives_a_new_generation_for_each_save() {
        let first = new_generation();
        std::thread::sleep(Duration::from_millis(1));
        assert_ne!(first, new_generation());
    }

    #[test]
    fn decodes_an_encoded_value() {
        let stored = encode("v \"q\"\n\u{fc}");
        assert_eq!(
            decode(&stored).unwrap(),
            Secret::new("v \"q\"\n\u{fc}".into())
        );
    }

    #[test]
    fn reads_a_value_with_no_mark_as_plain_text() {
        assert_eq!(
            decode("plain value").unwrap(),
            Secret::new("plain value".into())
        );
    }

    #[test]
    fn reports_an_encoded_value_that_is_not_valid_without_the_value() {
        let Err(StoreError::Failure(message)) = decode("ncp-base64:@@secret@@") else {
            panic!("a bad value must fail");
        };
        assert!(!message.contains("secret@@"));
    }

    #[test]
    fn finds_the_accounts_of_ncp_in_a_keychain_dump() {
        let dump = r#"keychain: "/Users/u/Library/Keychains/login.keychain-db"
version: 512
class: "genp"
attributes:
    0x00000007 <blob>="ncp"
    "acct"<blob>="secret/GITHUB_TOKEN"
    "svce"<blob>="ncp"
keychain: "/Users/u/Library/Keychains/login.keychain-db"
version: 512
class: "genp"
attributes:
    "acct"<blob>="secret/OTHER"
    "svce"<blob>="other-app"
keychain: "/Users/u/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "acct"<blob>=0x00FF  " ÿ"
    "svce"<blob>="ncp"
keychain: "/Users/u/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "svce"<blob>="ncp"
    "acct"<blob>="oauth/gh/tokens"
"#;
        assert_eq!(
            ncp_accounts(dump),
            ["secret/GITHUB_TOKEN", "oauth/gh/tokens"]
        );
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
