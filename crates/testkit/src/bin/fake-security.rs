//! A fake of the macOS `security` program, for the tests of the Keychain
//! store. Tests never use the real Keychain.
//!
//! The fake does only the commands that the store sends, with the exit
//! codes of the real program. It keeps its files in the directory of the
//! path that started it (`argv[0]`). Each test links the fake into its own
//! directory. An environment variable cannot do this: the store starts the
//! program with the environment of the test process, which all tests share.
//!
//! Files in that directory:
//! - `items.json`: the items, as `{service: {account: password}}`.
//! - `calls.log`: the arguments of each call, as one JSON array on each
//!   line.
//! - `delay`: when it exists, each call first waits the number of
//!   milliseconds in this file. This is a program that gives no answer, for
//!   example at an unlock dialog.
//! - `fail`: when it exists, each call copies its input to stderr and exits
//!   with the code in this file. This is a program that fails and shows the
//!   data it received.
//!
//! Commands:
//! - `-i`: read commands from stdin, one on each line, with the quotes and
//!   the line buffer of the real program.
//! - `add-generic-password [-U] -s SERVICE -a ACCOUNT -w PASSWORD`
//! - `find-generic-password -s SERVICE -a ACCOUNT [-w]`
//! - `delete-generic-password -s SERVICE -a ACCOUNT`

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

type Items = BTreeMap<String, BTreeMap<String, String>>;

/// `security` exits with the low byte of the status of the Security
/// framework: errSecItemNotFound (-25300) gives 44, and errSecDuplicateItem
/// (-25299) gives 45.
const ITEM_NOT_FOUND: u8 = 44;
const DUPLICATE_ITEM: u8 = 45;
const USAGE: u8 = 2;
const FAILURE: u8 = 1;

/// The real program reads each line of `-i` with `fgets` into a buffer of
/// 4096 bytes. It runs the rest of a longer line as a new command.
const LINE_BUFFER: usize = 4096;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(dir) = args
        .first()
        .map(Path::new)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    else {
        return ExitCode::from(USAGE);
    };
    let words: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    if record_call(&dir, &words).is_err() {
        return ExitCode::from(FAILURE);
    }
    if let Some(ms) = fs::read_to_string(dir.join("delay"))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    if let Some(code) = failure_code(&dir) {
        let _ = io::copy(&mut io::stdin(), &mut io::stderr());
        return ExitCode::from(code);
    }
    let code = match words.as_slice() {
        ["-i"] => interactive(&dir),
        command => run(&dir, command),
    };
    ExitCode::from(code)
}

fn record_call(dir: &Path, words: &[&str]) -> io::Result<()> {
    let line = serde_json::to_string(words).map_err(io::Error::other)?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("calls.log"))
        .and_then(|mut log| writeln!(log, "{line}"))
}

fn failure_code(dir: &Path) -> Option<u8> {
    fs::read_to_string(dir.join("fail"))
        .ok()
        .map(|text| text.trim().parse().unwrap_or(FAILURE))
}

/// Runs each line of stdin. The exit code is the code of the last command,
/// as in the real program.
fn interactive(dir: &Path) -> u8 {
    let mut input = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        return FAILURE;
    }
    lines(&input)
        .into_iter()
        .map(|line| split_line(&String::from_utf8_lossy(line)))
        .filter(|words| !words.is_empty())
        .fold(0, |_, words| {
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            run(dir, &words)
        })
}

/// Cuts the input as `fgets` does: a line ends after a new line, or when the
/// buffer is full.
fn lines(input: &[u8]) -> Vec<&[u8]> {
    let limit = LINE_BUFFER - 1;
    let mut rest = input;
    let mut lines = Vec::new();
    while !rest.is_empty() {
        let end = rest
            .iter()
            .take(limit)
            .position(|&byte| byte == b'\n')
            .map_or(limit.min(rest.len()), |at| at + 1);
        let (line, tail) = rest.split_at(end);
        lines.push(line.strip_suffix(b"\n").unwrap_or(line));
        rest = tail;
    }
    lines
}

/// Splits a line into words, as the real program does: a double quote at
/// the start of a word quotes it up to the next double quote, and a
/// backslash keeps the next character as it is.
fn split_line(line: &str) -> Vec<String> {
    let mut chars = line.chars().peekable();
    let mut words = Vec::new();
    while let Some(&first) = chars.peek() {
        if first.is_ascii_whitespace() {
            chars.next();
            continue;
        }
        let quoted = first == '"';
        if quoted {
            chars.next();
        }
        let mut word = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => word.extend(chars.next()),
                '"' if quoted => break,
                c if !quoted && c.is_ascii_whitespace() => break,
                c => word.push(c),
            }
        }
        words.push(word);
    }
    words
}

#[derive(Default)]
struct Options {
    service: Option<String>,
    account: Option<String>,
    password: Option<String>,
    update: bool,
    show_password: bool,
}

/// Reads the options of a command. `-w` takes a value only for
/// `add-generic-password`.
fn parse_options(words: &[&str], password_has_value: bool) -> Option<Options> {
    match words {
        [] => Some(Options::default()),
        ["-s", value, rest @ ..] => {
            parse_options(rest, password_has_value).map(|options| Options {
                service: Some((*value).to_string()),
                ..options
            })
        }
        ["-a", value, rest @ ..] => {
            parse_options(rest, password_has_value).map(|options| Options {
                account: Some((*value).to_string()),
                ..options
            })
        }
        ["-w", value, rest @ ..] if password_has_value => parse_options(rest, password_has_value)
            .map(|options| Options {
                password: Some((*value).to_string()),
                ..options
            }),
        ["-w", rest @ ..] if !password_has_value => {
            parse_options(rest, password_has_value).map(|options| Options {
                show_password: true,
                ..options
            })
        }
        ["-U", rest @ ..] => parse_options(rest, password_has_value).map(|options| Options {
            update: true,
            ..options
        }),
        _ => None,
    }
}

fn run(dir: &Path, words: &[&str]) -> u8 {
    let Some((command, options)) = words.split_first() else {
        return 0;
    };
    let parsed = parse_options(options, *command == "add-generic-password");
    let result = match (*command, parsed) {
        ("add-generic-password", Some(options)) => add(&items_path(dir), &options),
        ("find-generic-password", Some(options)) => find(&items_path(dir), &options),
        ("delete-generic-password", Some(options)) => delete(&items_path(dir), &options),
        (_, None) => Err(Failed::Usage),
        (other, Some(_)) => Err(Failed::Unknown(other.to_string())),
    };
    match result {
        Ok(()) => 0,
        Err(failed) => {
            eprintln!("security: {}", failed.message());
            failed.code()
        }
    }
}

enum Failed {
    Usage,
    Unknown(String),
    NotFound,
    Duplicate,
    Io(io::Error),
}

impl Failed {
    fn message(&self) -> String {
        match self {
            Self::Usage => "the options are not valid".into(),
            Self::Unknown(command) => format!("no such command: {command}"),
            Self::NotFound => {
                "SecKeychainSearchCopyNext: The specified item could not be found in the keychain."
                    .into()
            }
            Self::Duplicate => {
                "SecKeychainItemCreateFromContent (<default>): The specified item already exists in the keychain."
                    .into()
            }
            Self::Io(error) => format!("fake items file: {error}"),
        }
    }

    fn code(&self) -> u8 {
        match self {
            Self::Usage => USAGE,
            Self::Unknown(_) | Self::Io(_) => FAILURE,
            Self::NotFound => ITEM_NOT_FOUND,
            Self::Duplicate => DUPLICATE_ITEM,
        }
    }
}

fn items_path(dir: &Path) -> PathBuf {
    dir.join("items.json")
}

fn read_items(path: &Path) -> Result<Items, Failed> {
    match fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|error| Failed::Io(io::Error::other(error)))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Items::new()),
        Err(error) => Err(Failed::Io(error)),
    }
}

fn write_items(path: &Path, items: &Items) -> Result<(), Failed> {
    serde_json::to_string_pretty(items)
        .map_err(io::Error::other)
        .and_then(|text| fs::write(path, text))
        .map_err(Failed::Io)
}

fn key(options: &Options) -> Result<(&str, &str), Failed> {
    options
        .service
        .as_deref()
        .zip(options.account.as_deref())
        .ok_or(Failed::Usage)
}

fn add(path: &Path, options: &Options) -> Result<(), Failed> {
    let (service, account) = key(options)?;
    let password = options.password.clone().ok_or(Failed::Usage)?;
    let mut items = read_items(path)?;
    let accounts = items.entry(service.to_string()).or_default();
    if accounts.contains_key(account) && !options.update {
        return Err(Failed::Duplicate);
    }
    accounts.insert(account.to_string(), password);
    write_items(path, &items)
}

fn find(path: &Path, options: &Options) -> Result<(), Failed> {
    let (service, account) = key(options)?;
    let items = read_items(path)?;
    let password = items
        .get(service)
        .and_then(|accounts| accounts.get(account))
        .ok_or(Failed::NotFound)?;
    if options.show_password {
        println!("{}", shown_password(password));
    } else {
        println!(
            "keychain: \"fake\"\n    \"acct\"<blob>=\"{account}\"\n    \"svce\"<blob>=\"{service}\""
        );
    }
    Ok(())
}

/// The real program shows a password with a byte that is not printable
/// ASCII as hexadecimal digits.
fn shown_password(password: &str) -> String {
    if password
        .bytes()
        .all(|byte| byte == b' ' || byte.is_ascii_graphic())
    {
        password.to_string()
    } else {
        password.bytes().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }
}

fn delete(path: &Path, options: &Options) -> Result<(), Failed> {
    let (service, account) = key(options)?;
    let mut items = read_items(path)?;
    items
        .get_mut(service)
        .and_then(|accounts| accounts.remove(account))
        .ok_or(Failed::NotFound)?;
    write_items(path, &items)?;
    println!("password has been deleted.");
    Ok(())
}
