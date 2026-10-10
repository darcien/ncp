//! Secrets: the values of `${NAME}`, and the `secret` commands.
//!
//! `ncp` gets the value of a name from the first place that has it: the
//! environment, the native store, then the opt-in credential file. It opens
//! the native store only for a name that the environment does not set, so a
//! server with no `${NAME}` never opens it. Each store call runs on a
//! blocking thread with a time limit, because a locked store can wait for
//! an unlock prompt with no limit. Ctrl-C ends a store call at once.

use std::{collections::BTreeSet, io::IsTerminal, sync::Arc, time::Duration};

use serde_json::json;
use tokio::io::AsyncReadExt;

use super::{
    args::{CredentialPlace, SecretCommand},
    ask,
    outcome::{Exit, Failure},
    render,
    session::{self, Context},
};
use crate::{
    config::{EntryError, Server, Values},
    credential::{CredentialKey, Secret, SecretName},
    expand::{Env, Template},
    store::{self, CredentialStore, FileStore, StoreError},
};

/// Opens the native store of the platform. Each run of a store program has
/// the limit. Tests give a fake.
pub type OpenNative = fn(Duration) -> Result<Arc<dyn CredentialStore>, StoreError>;

/// The production native store: the login Keychain or the Secret Service.
///
/// # Errors
///
/// Returns an error when the platform has no usable store.
pub fn open_native(limit: Duration) -> Result<Arc<dyn CredentialStore>, StoreError> {
    store::NativeStore::open(limit).map(|store| Arc::new(store) as Arc<dyn CredentialStore>)
}

pub(super) type Opened = Result<Arc<dyn CredentialStore>, StoreError>;

/// Opens the native store on a blocking thread. A store program stops
/// before the limit of the call, so no program stays after `ncp`.
pub(super) async fn native(context: &Context<'_>) -> Opened {
    open_within(context, store::CALL_LIMIT).await
}

pub(super) async fn open_within(context: &Context<'_>, limit: Duration) -> Opened {
    let open = context.open_native;
    let program_limit = limit.saturating_sub(PROGRAM_MARGIN);
    interruptible(
        context,
        store::run_blocking(limit, move || open(program_limit)),
    )
    .await
}

/// The time between the end of a store program and the limit of its call.
const PROGRAM_MARGIN: Duration = Duration::from_secs(1);

pub(super) fn file(context: &Context<'_>) -> Arc<dyn CredentialStore> {
    Arc::new(FileStore::new(context.paths.credential_file.clone()))
}

/// Ends the call at Ctrl-C. A store program in the process group of `ncp`
/// gets the same SIGINT and stops, so its error is the interrupt too.
pub(super) async fn interruptible<T>(
    context: &Context<'_>,
    call: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, StoreError> {
    let result = tokio::select! {
        result = call => result,
        () = context.cancel.cancelled() => Err(StoreError::Interrupted),
    };
    match result {
        Err(_) if context.cancel.is_cancelled() => Err(StoreError::Interrupted),
        other => other,
    }
}

pub(super) async fn call<T: Send + 'static>(
    context: &Context<'_>,
    store: &Arc<dyn CredentialStore>,
    action: impl FnOnce(&dyn CredentialStore) -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    let store = store.clone();
    let blocking = store::run_blocking(store::CALL_LIMIT, move || action(store.as_ref()));
    interruptible(context, blocking).await
}

fn key(name: &str) -> Result<CredentialKey, Failure> {
    SecretName::parse(name)
        .map(CredentialKey::Secret)
        .map_err(|error| Failure::usage(error.to_string()))
}

/// The expanded `env` or `headers` values of the server.
///
/// # Errors
///
/// Returns a usage failure for a name that no place has, and an
/// authorization failure when the native store cannot answer and the file
/// does not have the name.
pub(super) async fn resolve(context: &Context<'_>, server: &Server) -> Result<Values, Failure> {
    let name_of_server = server.name.as_str();
    let values = values_of(context, name_of_server, server.value_names()).await?;
    server
        .expand_values(&values)
        .map_err(|error| Failure::usage(error.to_string()).server(name_of_server))
}

/// The expanded `oauth.clientSecret`, from the same places as a `headers`
/// value. No error shows the value.
///
/// # Errors
///
/// Returns the failure of [`resolve`] for a name of the template.
pub(super) async fn client_secret(
    context: &Context<'_>,
    server: &str,
    template: &Template,
) -> Result<Secret, Failure> {
    let values = values_of(context, server, template.names().into_iter().collect()).await?;
    template.expand(&values).map(Secret::new).map_err(|error| {
        let error = EntryError::Variable {
            field: "oauth.clientSecret".into(),
            error,
        };
        Failure::usage(error.to_string()).server(server)
    })
}

/// The environment with the value of each name that it does not have,
/// from the stores.
async fn values_of(
    context: &Context<'_>,
    server: &str,
    names: BTreeSet<&str>,
) -> Result<Env, Failure> {
    let (set, missing): (Vec<&str>, Vec<&str>) = names
        .into_iter()
        .partition(|name| context.env.contains_key(*name));
    for name in set {
        context
            .ui
            .debug(Some(server), &format!("{name} from the environment"));
    }
    let mut values: Env = context.env.clone();
    if !missing.is_empty() {
        // The time of the store reads, also when they fail: a slow or
        // locked store is the reason to measure.
        let started = std::time::Instant::now();
        let found = read_stored(context, &missing).await;
        if let Some(timings) = &context.timings {
            timings
                .update(|record| super::timings::add(&mut record.credentials, started.elapsed()));
        }
        for (name, secret, place) in found.map_err(|failure| failure.server(server))? {
            context
                .ui
                .debug(Some(server), &format!("{name} from {place}"));
            values.insert(name.to_string(), secret.expose().to_string());
        }
    }
    Ok(values)
}

/// The values of the names from the stores, with the place of each one.
async fn read_stored<'a>(
    context: &Context<'_>,
    names: &[&'a str],
) -> Result<Vec<(&'a str, Secret, &'static str)>, Failure> {
    let mut native = native(context).await;
    let file = file(context);
    let mut found = Vec::new();
    for name in names {
        let (secret, place) = find(context, name, &mut native, &file).await?;
        found.push((*name, secret, place));
    }
    Ok(found)
}

/// True for an error of the whole store, not of one item.
pub(super) fn store_is_down(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Unavailable(_)
            | StoreError::Denied(_)
            | StoreError::NoAnswer(_)
            | StoreError::Interrupted
    )
}

/// The value of one name: the native store first, then the file. A native
/// store that is down does not stop the search, because the file can have
/// the name. Its error replaces the store, so that the next name does not
/// wait for the time limit again. An error of one item stops the search,
/// because a value from the file would then replace a newer value.
async fn find(
    context: &Context<'_>,
    name: &str,
    native: &mut Opened,
    file: &Arc<dyn CredentialStore>,
) -> Result<(Secret, &'static str), Failure> {
    let key = key(name)?;
    if let Ok(store) = native {
        let wanted = key.clone();
        match call(context, store, move |store| store.load(&wanted)).await {
            Ok(Some(secret)) => return Ok((secret, place_text(CredentialPlace::Native))),
            Ok(None) => {}
            Err(error) if store_is_down(&error) => *native = Err(error),
            Err(error) => return Err(store_failure(&error)),
        }
    }
    if let Err(StoreError::Interrupted) = native {
        return Err(session::interrupted());
    }
    let wanted = key.clone();
    if let Some(secret) = call(context, file, move |store| store.load(&wanted))
        .await
        .map_err(|error| store_failure(&error))?
    {
        return Ok((secret, place_text(CredentialPlace::File)));
    }
    Err(match native {
        Ok(_) => Failure::usage(format!("{name} is not set and not stored"))
            .hint(format!("ncp secret set {name}")),
        Err(error) => Failure::new(
            Exit::Auth,
            format!("{name} is not set, and the native store gave no value: {error}"),
        )
        .hint(format!("ncp secret set {name} --credential-store file")),
    })
}

/// The error again, for a second use. A store error holds no secret.
fn clone_error(error: &StoreError) -> StoreError {
    match error {
        StoreError::Unavailable(text) => StoreError::Unavailable(text.clone()),
        StoreError::Denied(text) => StoreError::Denied(text.clone()),
        StoreError::NoAnswer(limit) => StoreError::NoAnswer(*limit),
        StoreError::Interrupted => StoreError::Interrupted,
        other => StoreError::Failure(other.to_string()),
    }
}

/// A store error. The native store and its time limit are authorization
/// errors (exit 4). A credential file that `ncp` cannot read or write is a
/// local error (exit 3).
pub(super) fn store_failure(error: &StoreError) -> Failure {
    let exit = match error {
        StoreError::Interrupted => return session::interrupted(),
        StoreError::Io { .. } | StoreError::OpenPermissions(_) | StoreError::Corrupt(_) => {
            Exit::Protocol
        }
        StoreError::Unavailable(_)
        | StoreError::Denied(_)
        | StoreError::Failure(_)
        | StoreError::NoAnswer(_) => Exit::Auth,
    };
    Failure::new(exit, error.to_string())
}

pub(super) async fn command(context: &Context<'_>, command: SecretCommand) -> Result<(), Failure> {
    match command {
        SecretCommand::Set {
            name,
            credential_store,
        } => set(context, &name, credential_store).await,
        SecretCommand::List => list(context).await,
        SecretCommand::Delete { name } => delete(context, &name).await,
    }
}

pub(super) fn place_name(place: CredentialPlace) -> &'static str {
    match place {
        CredentialPlace::Native => "native",
        CredentialPlace::File => "file",
    }
}

pub(super) fn place_text(place: CredentialPlace) -> &'static str {
    match place {
        CredentialPlace::Native => "the native store",
        CredentialPlace::File => "the credential file",
    }
}

async fn set(context: &Context<'_>, name: &str, place: CredentialPlace) -> Result<(), Failure> {
    let key = key(name)?;
    // Check the store before the user types the value.
    let target = match place {
        CredentialPlace::Native => native(context).await.map_err(|error| {
            store_failure(&error).hint(format!("ncp secret set {name} --credential-store file"))
        })?,
        CredentialPlace::File => file(context),
    };
    let value = read_value(context, name).await?;
    let saved = key.clone();
    call(context, &target, move |store| store.save(&saved, &value))
        .await
        .map_err(|error| store_failure(&error))?;
    // A list of a server can depend on the value.
    crate::list_cache::remove_all(&context.paths.cache_dir);
    warn_about_copies(context, name, &key, place).await;
    context.ui.result(
        || format!("Stored {name} in {}.\n", place_text(place)),
        || json!({"name": name, "place": place_name(place)}),
    )
}

/// The time for the check of the other place. The check only gives a
/// warning, so it must not wait for a locked store.
const COPY_CHECK_LIMIT: Duration = Duration::from_secs(5);

/// Warnings when another place has a value for the name: `ncp` reads the
/// environment first, then the native store, then the file.
async fn warn_about_copies(
    context: &Context<'_>,
    name: &str,
    key: &CredentialKey,
    place: CredentialPlace,
) {
    if context.env.contains_key(name) {
        context.ui.warning(
            None,
            &format!("{name} is also set in the environment, and ncp uses that value first"),
        );
    }
    let other = match place {
        CredentialPlace::Native => CredentialPlace::File,
        CredentialPlace::File => CredentialPlace::Native,
    };
    let store = match other {
        CredentialPlace::Native => open_within(context, COPY_CHECK_LIMIT).await,
        CredentialPlace::File => Ok(file(context)),
    };
    let wanted = key.clone();
    let has_copy = match store {
        Ok(store) => {
            let check = store::run_blocking(COPY_CHECK_LIMIT, move || {
                Ok(store.keys()?.contains(&wanted))
            });
            interruptible(context, check).await
        }
        Err(error) => Err(error),
    };
    let message = match (has_copy, other) {
        (Ok(false) | Err(StoreError::Interrupted), _)
        | (Err(StoreError::Unavailable(_)), CredentialPlace::Native) => return,
        (Err(error), _) => {
            // No copy is known, so no command can fix it.
            context.ui.warning(
                None,
                &format!("ncp cannot check {} for a copy: {error}", place_text(other)),
            );
            return;
        }
        (Ok(true), CredentialPlace::File) => {
            format!("the credential file also has {name}; ncp reads the native store first")
        }
        (Ok(true), CredentialPlace::Native) => format!(
            "the native store also has {name}, and ncp reads it first: the new value is not used"
        ),
    };
    context.ui.warning(None, &message);
    // The design gives this command: it removes all copies, and then the
    // user stores the value again in one place.
    context.ui.hint(None, &format!("ncp secret delete {name}"));
}

/// Why `secret set` got no value.
enum Unread {
    Interrupted,
    TimedOut,
    Failed(std::io::Error),
}

/// Reads the value from stdin. When a person can answer (stdin and stderr
/// are a TTY), the input is not shown. A TTY on stdin with no TTY on
/// stderr is an error: the question would not show, and the terminal
/// would echo the value. Each read has the limit of `--timeout`. One new
/// line at the end is not part of the value, because `echo` and a pasted
/// line add it.
async fn read_value(context: &Context<'_>, name: &str) -> Result<Secret, Failure> {
    let text = if ask::interactive() {
        read_hidden_line(context, name).await
    } else if std::io::stdin().is_terminal() {
        return Err(Failure::usage(format!(
            "cannot ask for the value of {name}: stderr is not a terminal"
        ))
        .hint(format!(
            "give the value on stdin: ncp secret set {name} < FILE"
        )));
    } else {
        read_piped(context).await
    };
    let text = text.map_err(|unread| match unread {
        Unread::Interrupted => session::interrupted(),
        Unread::TimedOut => Failure::protocol(format!(
            "no value of {name} came in {} s",
            context.request_limit.as_secs()
        ))
        .hint("give a longer limit with --timeout <SECONDS>"),
        Unread::Failed(error) => Failure::usage(format!(
            "cannot read the value of {name} from stdin: {error}"
        )),
    })?;
    let line = text.strip_suffix('\n').unwrap_or(&text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return Err(Failure::usage(format!("the value of {name} is empty")));
    }
    Ok(Secret::new(line.to_string()))
}

/// All of stdin, for a value that a pipe or a file gives.
async fn read_piped(context: &Context<'_>) -> Result<String, Unread> {
    let mut text = String::new();
    let mut stdin = tokio::io::stdin();
    tokio::select! {
        read = stdin.read_to_string(&mut text) => read.map_err(Unread::Failed)?,
        () = tokio::time::sleep(context.request_limit) => return Err(Unread::TimedOut),
        () = context.cancel.cancelled() => return Err(Unread::Interrupted),
    };
    Ok(text)
}

/// The program that changes the settings of the terminal. Both macOS and
/// Linux have it at this path.
const STTY: &str = "/bin/stty";

fn stty(args: &[&str]) -> std::io::Result<String> {
    let output = std::process::Command::new(STTY)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(std::io::Error::other(format!(
            "{STTY} {} failed",
            args.join(" ")
        )))
    }
}

/// One line from the terminal, with the echo off. The terminal settings
/// come back after the line, after an error, after the time limit and
/// after Ctrl-C, so the shell of the user works again.
///
/// The terminal is in non-canonical mode: in canonical mode macOS keeps
/// only 1024 bytes of a line, and a longer token would never end. Thus
/// `ncp` does the backspace itself.
async fn read_hidden_line(context: &Context<'_>, name: &str) -> Result<String, Unread> {
    let saved = stty(&["-g"]).map_err(Unread::Failed)?;
    eprint!("Value of {name}: ");
    // Fail closed: with no echo control, the value would show.
    stty(&["-echo", "-icanon", "min", "1", "time", "0"]).map_err(Unread::Failed)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    // A thread of its own, not of the runtime, so that the runtime does not
    // wait for it at the exit after Ctrl-C or the time limit.
    std::thread::spawn(move || {
        let _ = sender.send(read_terminal_line(&mut std::io::stdin().lock()));
    });
    let read = tokio::select! {
        read = receiver => read
            .map_err(|_| Unread::Failed(std::io::Error::other("the reader stopped")))
            .and_then(|line| line.map_err(Unread::Failed)),
        () = tokio::time::sleep(context.request_limit) => Err(Unread::TimedOut),
        () = context.cancel.cancelled() => Err(Unread::Interrupted),
    };
    let restored = stty(&[saved.as_str()]);
    // The new line that the echo did not show.
    eprintln!();
    let line = read?;
    restored.map_err(Unread::Failed)?;
    Ok(line)
}

/// Reads bytes until a new line, a carriage return or Ctrl-D. A backspace
/// or DEL removes the last character.
fn read_terminal_line(input: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut line: Vec<u8> = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        if input.read(&mut byte)? == 0 {
            break;
        }
        match byte[0] {
            b'\n' | b'\r' | 0x04 => break,
            0x08 | 0x7f => {
                // Remove the last character, with all of its UTF-8 bytes.
                while let Some(last) = line.pop() {
                    if last & 0xc0 != 0x80 {
                        break;
                    }
                }
            }
            other => line.push(other),
        }
    }
    String::from_utf8(line).map_err(|_| std::io::Error::other("the value is not UTF-8 text"))
}

/// The secret names of a store. A store that cannot answer gives a
/// warning and no names. A host with no native store gives no warning,
/// because the user then keeps all values in the file.
async fn names_in(
    context: &Context<'_>,
    store: &Opened,
    place: CredentialPlace,
) -> Vec<SecretName> {
    let keys = match store {
        Ok(store) => call(context, store, |store| store.keys()).await,
        Err(StoreError::Unavailable(_)) => return Vec::new(),
        Err(error) => Err(clone_error(error)),
    };
    match keys {
        Ok(keys) => keys
            .into_iter()
            .filter_map(|key| match key {
                CredentialKey::Secret(name) => Some(name),
                _ => None,
            })
            .collect(),
        Err(error) => {
            context.ui.warning(
                None,
                &format!("ncp cannot list {}: {error}", place_text(place)),
            );
            Vec::new()
        }
    }
}

async fn list(context: &Context<'_>) -> Result<(), Failure> {
    // The names come from the stores. A configuration file that does not
    // load only takes away the "used" mark.
    let used = match session::load_config(&context.paths.config_file, context.env) {
        Ok(loaded) => loaded.config.variables().clone(),
        Err(failure) => {
            context.ui.warning(
                None,
                &format!("{}; no secret shows as used", failure.message),
            );
            BTreeSet::new()
        }
    };
    let native_names = names_in(context, &native(context).await, CredentialPlace::Native).await;
    if context.cancel.is_cancelled() {
        return Err(session::interrupted());
    }
    let file_names = names_in(context, &Ok(file(context)), CredentialPlace::File).await;
    let rows = secret_rows(&native_names, &file_names, &used);
    context.ui.result(
        || {
            render::table(
                &rows
                    .iter()
                    .map(|row| {
                        vec![
                            row.name.clone(),
                            row.places.join(", "),
                            if row.used { "" } else { "not used" }.to_string(),
                        ]
                    })
                    .collect::<Vec<_>>(),
            )
        },
        || {
            json!({"secrets": rows.iter().map(|row| json!({
                "name": row.name, "places": row.places, "used": row.used,
            })).collect::<Vec<_>>()})
        },
    )
}

struct SecretRow {
    name: String,
    places: Vec<&'static str>,
    used: bool,
}

fn secret_rows(
    native: &[SecretName],
    file: &[SecretName],
    used: &BTreeSet<String>,
) -> Vec<SecretRow> {
    let names: BTreeSet<&SecretName> = native.iter().chain(file).collect();
    names
        .into_iter()
        .map(|name| SecretRow {
            name: name.to_string(),
            places: [(native, "native"), (file, "file")]
                .into_iter()
                .filter(|(names, _)| names.contains(name))
                .map(|(_, place)| place)
                .collect(),
            used: used.contains(name.as_str()),
        })
        .collect()
}

async fn delete(context: &Context<'_>, name: &str) -> Result<(), Failure> {
    let key = key(name)?;
    let stores = [
        (CredentialPlace::Native, native(context).await),
        (CredentialPlace::File, Ok(file(context))),
    ];
    let mut removed = Vec::new();
    let mut failed = None;
    for (place, store) in &stores {
        // One call that removes and tells if the item was there: a second
        // call could pass the limit and remove the item after the exit.
        let result = match store {
            Ok(store) => {
                let wanted = key.clone();
                call(context, store, move |store| store.delete(&wanted)).await
            }
            // A host with no native store has no copy there.
            Err(StoreError::Unavailable(_)) => Ok(false),
            Err(error) => Err(clone_error(error)),
        };
        match result {
            Ok(true) => removed.push(*place),
            Ok(false) => {}
            Err(StoreError::Interrupted) => return Err(session::interrupted()),
            Err(error) => {
                context.ui.warning(
                    None,
                    &format!("ncp cannot change {}: {error}", place_text(*place)),
                );
                failed = Some(error);
            }
        }
    }
    match (removed.is_empty(), failed) {
        (true, Some(error)) => Err(store_failure(&error)),
        (true, None) => {
            Err(Failure::usage(format!("{name} is not stored")).hint("ncp secret list"))
        }
        (false, _) => {
            crate::list_cache::remove_all(&context.paths.cache_dir);
            let places: Vec<&str> = removed.iter().map(|place| place_name(*place)).collect();
            let text: Vec<&str> = removed.iter().map(|place| place_text(*place)).collect();
            context.ui.result(
                || format!("Removed {name} from {}.\n", text.join(" and ")),
                || json!({"name": name, "places": places}),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<SecretName> {
        list.iter()
            .map(|name| SecretName::parse(name).unwrap())
            .collect()
    }

    #[test]
    fn secret_rows_merge_places_and_mark_unused_names() {
        let used = ["A".to_string(), "C".to_string()].into_iter().collect();
        let rows = secret_rows(&names(&["A", "B"]), &names(&["B", "C"]), &used);
        let view: Vec<(&str, Vec<&str>, bool)> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.places.clone(), row.used))
            .collect();
        assert_eq!(
            view,
            [
                ("A", vec!["native"], true),
                ("B", vec!["native", "file"], false),
                ("C", vec!["file"], true),
            ]
        );
    }

    #[test]
    fn terminal_line_ends_at_new_line_and_does_backspace() {
        let mut input: &[u8] = b"ab\x7fc\xc3\xa9\x08d\re";
        assert_eq!(read_terminal_line(&mut input).unwrap(), "acd");
        let long = "x".repeat(3000) + "\n";
        assert_eq!(
            read_terminal_line(&mut long.as_bytes()).unwrap().len(),
            3000
        );
    }

    #[test]
    fn store_errors_map_to_auth_or_local_exit_codes() {
        assert_eq!(
            store_failure(&StoreError::NoAnswer(std::time::Duration::from_secs(60))).exit,
            Exit::Auth
        );
        assert_eq!(
            store_failure(&StoreError::Corrupt("/x".into())).exit,
            Exit::Protocol
        );
    }
}
