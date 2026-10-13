//! The `ncp` command line, built in the test kit. Cargo gives the tests of
//! a package only the paths of the binaries of that package. This binary
//! runs the same code as `ncp`, with these changes. Thus a test never uses
//! the real Keychain, the real Secret Service, or a real browser.
//!
//! - The native store is the fake `security` program at the path in
//!   `NCP_TEST_SECURITY_PROGRAM`, or no store when that variable does not
//!   exist.
//! - The browser is a fake. With `NCP_TEST_BROWSER=follow`, it sends a GET
//!   to the URL, then a GET to the `Location` of the answer. A browser does
//!   this after a login that the user approves. With no value, no browser
//!   is available.
//! - With `NCP_TEST_TTY=1`, a person answers questions on stdin, as on a
//!   TTY.
//! - With `NCP_TEST_CHANGE_BEFORE_RENAME=1`, the configuration file changes
//!   immediately before `ncp` renames its new copy over it.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use ncp::{
    cli::Platform,
    store::{CredentialStore, SecurityStore, StoreError},
};

fn fake_native(limit: Duration) -> Result<Arc<dyn CredentialStore>, StoreError> {
    match std::env::var_os("NCP_TEST_SECURITY_PROGRAM") {
        Some(program) => Ok(Arc::new(
            SecurityStore::with_program(program).with_limit(limit),
        )),
        None => Err(StoreError::Unavailable(
            "the test has no native store".into(),
        )),
    }
}

/// One plain HTTP GET. Returns the `Location` header of the answer.
fn get(url: &str) -> std::io::Result<Option<String>> {
    let url = url::Url::parse(url).map_err(std::io::Error::other)?;
    let host = url.host_str().unwrap_or_default();
    let port = url.port_or_known_default().unwrap_or(80);
    let mut stream = TcpStream::connect((host, port))?;
    let target = &url[url::Position::BeforePath..];
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nhost: {host}:{port}\r\nconnection: close\r\n\r\n"
    )?;
    let location = BufReader::new(stream)
        .lines()
        .map_while(Result::ok)
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("location")
                .then(|| value.trim().to_string())
        });
    Ok(location)
}

fn fake_browser(url: &str) -> std::io::Result<()> {
    match std::env::var("NCP_TEST_BROWSER").as_deref() {
        Ok("follow") => {
            let url = url.to_string();
            std::thread::spawn(move || {
                if let Ok(Some(location)) = get(&url) {
                    let _ = get(&location);
                }
            });
            Ok(())
        }
        _ => Err(std::io::Error::other("the test has no browser")),
    }
}

fn fake_tty() -> bool {
    std::env::var_os("NCP_TEST_TTY").is_some()
}

/// With `NCP_TEST_CHANGE_BEFORE_RENAME=1`, another tool changes the
/// configuration file immediately before `ncp` renames its new copy over it.
fn change_before_rename(path: &std::path::Path) {
    if std::env::var_os("NCP_TEST_CHANGE_BEFORE_RENAME").is_some()
        && let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path)
    {
        let _ = file.write_all(b" ");
    }
}

fn main() -> std::process::ExitCode {
    ncp::cli::main_with(Platform {
        open_native: fake_native,
        open_browser: fake_browser,
        interactive: fake_tty,
        before_rename: change_before_rename,
    })
}
