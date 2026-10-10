//! Secrets: the values of `${NAME}`, and the `secret` commands.
//!
//! `ncp` gets the value of a name from the first place that has it: the
//! environment, the native store, then the opt-in credential file. A store
//! gives only the item of the destination of the field, so a value
//! never follows its name to another origin. `ncp` opens the native store
//! only for a name that the environment does not set, so a server with no
//! `${NAME}` never opens it. Each store call runs on a
//! blocking thread with a time limit, because a locked store can wait for
//! an unlock prompt with no limit. Ctrl-C ends a store call at once.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::IsTerminal,
    sync::Arc,
    time::Duration,
};

use serde_json::json;

use super::{
    args::{CredentialPlace, SecretCommand},
    ask,
    outcome::{Exit, Failure},
    render,
    session::{self, Context},
};
use crate::{
    binding::{self, Field, Target, TargetError, Use},
    config::{ClientKind, EntryError, PreRegisteredClient, Server, Values},
    credential::{CredentialKey, Destination, Secret, SecretName},
    expand::{self, Env},
    server::{Origin, OriginError},
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

fn secret_name(name: &str) -> Result<SecretName, Failure> {
    SecretName::parse(name).map_err(|error| Failure::usage(error.to_string()))
}

/// The values of an entry, and the names whose values came from a store.
pub(super) struct Resolved {
    pub values: Values,
    pub stored: Vec<SecretName>,
}

/// The expanded `env` or `headers` values of the server.
///
/// # Errors
///
/// Returns a usage failure for a name that no place has for the
/// destination of the entry, and an authorization failure when the native
/// store cannot answer and the file does not have the item.
pub(super) async fn resolve(context: &Context<'_>, server: &Server) -> Result<Resolved, Failure> {
    let destination = binding::value_destination(server);
    let (values, stored) = values_of(context, server, server.value_names(), destination).await?;
    let values = server
        .expand_values(&values)
        .map_err(|error| Failure::usage(error.to_string()).server(server.name.as_str()))?;
    Ok(Resolved { values, stored })
}

/// The expanded `oauth.clientSecret`, or `None` for a client with no
/// secret. A stored value comes only from the item of the origin of
/// `oauth.issuer`. No error shows the value.
///
/// # Errors
///
/// Returns the failure of [`resolve`] for a name of the template.
pub(super) async fn client_secret(
    context: &Context<'_>,
    server: &Server,
    client: &PreRegisteredClient,
) -> Result<Option<Secret>, Failure> {
    let ClientKind::Confidential { secret, issuer } = &client.kind else {
        return Ok(None);
    };
    let names = secret.names().into_iter().collect();
    let (values, _) =
        values_of(context, server, names, binding::issuer_destination(issuer)).await?;
    secret
        .expand(&values)
        .map(|value| Some(Secret::new(value)))
        .map_err(|error| {
            let error = EntryError::Variable {
                field: "oauth.clientSecret".into(),
                error,
            };
            Failure::usage(error.to_string()).server(server.name.as_str())
        })
}

/// The environment with the value of each name that it does not have,
/// from the items of the destination in the stores, and the names that
/// came from a store.
async fn values_of(
    context: &Context<'_>,
    server: &Server,
    names: BTreeSet<&str>,
    destination: Result<Destination, OriginError>,
) -> Result<(Env, Vec<SecretName>), Failure> {
    let name_of_server = server.name.as_str();
    let (set, missing): (Vec<&str>, Vec<&str>) = names
        .into_iter()
        .partition(|name| context.env.contains_key(*name));
    for name in set {
        context.ui.debug(
            Some(name_of_server),
            &format!("{name} from the environment"),
        );
    }
    let mut values: Env = context.env.clone();
    if missing.is_empty() {
        return Ok((values, Vec::new()));
    }
    let destination =
        destination.map_err(|error| not_literal(&missing, &error).server(name_of_server))?;
    let names = missing
        .iter()
        .map(|name| secret_name(name))
        .collect::<Result<Vec<_>, Failure>>()?;
    // The time of the store reads, also when they fail: a slow or locked
    // store is the reason to measure.
    let started = std::time::Instant::now();
    let found = read_stored(context, server, &destination, names).await;
    if let Some(timings) = &context.timings {
        timings.update(|record| super::timings::add(&mut record.credentials, started.elapsed()));
    }
    let mut stored = Vec::new();
    for (name, secret, place) in found.map_err(|failure| failure.server(name_of_server))? {
        context.ui.debug(
            Some(name_of_server),
            &format!("{name} from {place}, for {destination}"),
        );
        values.insert(name.to_string(), secret.expose().to_string());
        stored.push(name);
    }
    Ok((values, stored))
}

/// The stop for a stored value of an entry with no literal origin. An
/// environment value has no binding, so the fix is the URL, not an export.
fn not_literal(names: &[&str], error: &OriginError) -> Failure {
    Failure::usage(format!(
        "{} is not set, and ncp sends a stored value only to a literal origin: {error}",
        names.join(", ")
    ))
}

/// The values of the names from the items of the destination, with the
/// place of each one.
async fn read_stored(
    context: &Context<'_>,
    server: &Server,
    destination: &Destination,
    names: Vec<SecretName>,
) -> Result<Vec<(SecretName, Secret, &'static str)>, Failure> {
    let mut native = native(context).await;
    let file = file(context);
    let mut found = Vec::new();
    for name in names {
        let (secret, place) =
            find(context, server, (destination, &name), &mut native, &file).await?;
        found.push((name, secret, place));
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

/// The value of one key: the native store first, then the file. A native
/// store that is down does not stop the search, because the file can have
/// the item. Its error replaces the store, so that the next name does not
/// wait for the time limit again. An error of one item stops the search,
/// because a value from the file would then replace a newer value. No item
/// of another destination is a fallback.
async fn find(
    context: &Context<'_>,
    server: &Server,
    (destination, name): (&Destination, &SecretName),
    native: &mut Opened,
    file: &Arc<dyn CredentialStore>,
) -> Result<(Secret, &'static str), Failure> {
    let key = CredentialKey::Secret {
        destination: destination.clone(),
        name: name.clone(),
    };
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
    Err(miss(server, name, destination, native.as_ref().err()))
}

/// The stop when no place has the item of the destination. The
/// message names only the current destination: `ncp` reads no other item.
fn miss(
    server: &Server,
    name: &SecretName,
    destination: &Destination,
    native_error: Option<&StoreError>,
) -> Failure {
    let word = render::shell_word(server.name.as_str());
    let set = format!("ncp secret set {name} --server {word}");
    let failure = match native_error {
        None => {
            let stop = match destination {
                Destination::Origin(_) => format!(
                    "ncp sent nothing to server {}. If this URL is correct, store the value for it.",
                    server.name.as_str()
                ),
                Destination::Stdio => {
                    format!("ncp did not start server {}.", server.name.as_str())
                }
            };
            Failure::usage(format!(
                "{name} is not set, and not stored for {destination}. {stop}"
            ))
            .hint(set)
            .hint("ncp secret list")
        }
        Some(error) => Failure::new(
            Exit::Auth,
            format!(
                "{name} is not set, and the native store gave no value for {destination}: {error}"
            ),
        )
        .hint(format!("{set} --credential-store file")),
    };
    failure.destination(destination.clone())
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
            server,
            credential_store,
        } => set(context, &name, server.as_deref(), credential_store).await,
        SecretCommand::List => list(context).await,
        SecretCommand::Delete {
            name,
            server,
            unused,
            all,
        } => {
            let removal = match (server, unused, all) {
                (Some(server), _, _) => Removal::Server(server),
                (None, true, _) => Removal::Listed(Listed::Unused),
                (None, false, true) => Removal::Listed(Listed::All),
                (None, false, false) => Removal::Listed(Listed::One),
            };
            delete(context, &name, removal).await
        }
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

/// The entries that a destination is for, as text: "server linear", or
/// "the issuer of server acme" for a client secret.
fn users_text(uses: &[Use<'_>]) -> String {
    uses.iter()
        .map(|item| match item.field {
            Field::Value => format!("server {}", item.server.name.as_str()),
            Field::ClientSecret => format!("the issuer of server {}", item.server.name.as_str()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn server_names(uses: &[Use<'_>]) -> Vec<String> {
    uses.iter()
        .map(|item| item.server.name.as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The JSON of a destination: the origin, or `null` for stdio.
pub(super) fn destination_json(destination: &Destination) -> serde_json::Value {
    json!(destination.origin().map(Origin::as_str))
}

/// The one destination that `secret set` (and `secret delete --server`)
/// uses: the entries that name `${NAME}` in `env`, `headers` or
/// `oauth.clientSecret`, or only the entry of `--server`.
fn set_target<'a>(
    context: &Context<'_>,
    loaded: &'a session::Loaded,
    name: &SecretName,
    only: Option<&str>,
    several_hint: Option<String>,
) -> Result<Target<'a>, Failure> {
    let servers: Vec<&Server> = match only {
        Some(word) => vec![session::select(loaded, word, context.ui)?],
        None => loaded.config.servers().collect(),
    };
    let uses = binding::uses(servers.iter().copied(), name.as_str());
    binding::target(uses).map_err(|error| match error {
        TargetError::NoUse => no_use(name, only, loaded),
        TargetError::NotLiteral { server, error } => Failure::usage(format!(
            "server {} names ${{{name}}}, and ncp sends a stored value only to a literal origin: {error}",
            server.name.as_str()
        )),
        TargetError::Several(targets) => {
            let list: Vec<String> = targets
                .iter()
                .map(|target| format!("{} ({})", target.destination, users_text(&target.uses)))
                .collect();
            // One entry with two destinations: `--server` cannot choose, so
            // only a second name helps.
            if let Some(word) = only {
                let failure = Failure::usage(format!(
                    "server {word} names ${{{name}}} for more than one destination: {}; use one name for each field",
                    list.join("; ")
                ));
                return match several_hint {
                    Some(fix) => failure.hint(fix),
                    None => failure,
                };
            }
            Failure::usage(format!(
                "{name} has more than one destination: {}; choose one entry",
                list.join("; ")
            ))
            .hint(format!("ncp secret set {name} --server <SERVER>"))
        }
    })
}

/// No entry names `${NAME}` in a field that can read the store. A name in
/// `command`, `args`, `cwd` or `url` comes only from the environment (B32),
/// and an unset one makes the entry invalid.
fn no_use(name: &SecretName, only: Option<&str>, loaded: &session::Loaded) -> Failure {
    let fields = "env, headers or oauth.clientSecret";
    let message = match only {
        Some(word) => format!("server {word} does not name ${{{name}}} in {fields}"),
        None => format!("no entry names ${{{name}}} in {fields}; add the entry first"),
    };
    let unset = loaded.config.invalid().any(|(server, error)| {
        only.is_none_or(|word| word == server)
            && matches!(
                error,
                EntryError::Variable { error: expand::ExpandError::Unset(unset), .. }
                    if unset == name.as_str()
            )
    });
    let failure = Failure::usage(message);
    if unset {
        failure.hint(format!("export {name}=<value>"))
    } else {
        failure
    }
}

async fn set(
    context: &Context<'_>,
    name: &str,
    only: Option<&str>,
    place: CredentialPlace,
) -> Result<(), Failure> {
    let name = secret_name(name)?;
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let target = set_target(context, &loaded, &name, only, None)?;
    let destination = &target.destination;
    let key = CredentialKey::Secret {
        destination: destination.clone(),
        name: name.clone(),
    };
    // Check the store before the user types the value.
    let store = match place {
        CredentialPlace::Native => native(context).await.map_err(|error| {
            let word = render::shell_word(target.uses[0].server.name.as_str());
            store_failure(&error).hint(format!(
                "ncp secret set {name} --server {word} --credential-store file"
            ))
        })?,
        CredentialPlace::File => file(context),
    };
    if let Destination::Origin(origin) = destination
        && let Some(host) = url::Url::parse(origin.as_str())
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .filter(|host| ask::has_punycode_label(host))
    {
        context.ui.warning(
            None,
            &format!(
                "the host {host} has a Punycode label (xn--): its name can look like another name"
            ),
        );
    }
    let users = users_text(&target.uses);
    let prompt = format!("Value of {name} for {destination} ({users}): ");
    let value = read_value(context, name.as_str(), &prompt).await?;
    let saved = key.clone();
    call(context, &store, move |store| store.save(&saved, &value))
        .await
        .map_err(|error| store_failure(&error))?;
    // A list of a server can depend on the value.
    crate::list_cache::remove_all(&context.paths.cache_dir);
    warn_about_copies(context, &key, place, target.uses[0].server).await;
    context.ui.result(
        || {
            format!(
                "Stored {name} in {} for {destination} ({users}).\n",
                place_text(place)
            )
        },
        || {
            json!({
                "name": name.as_str(),
                "place": place_name(place),
                "destination": destination_json(destination),
                "servers": server_names(&target.uses),
            })
        },
    )
}

/// The time for the check of the other place. The check only gives a
/// warning, so it must not wait for a locked store.
const COPY_CHECK_LIMIT: Duration = Duration::from_secs(5);

/// Warnings when another place has a value for the key: `ncp` reads the
/// environment first, then the native store, then the file.
async fn warn_about_copies(
    context: &Context<'_>,
    key: &CredentialKey,
    place: CredentialPlace,
    server: &Server,
) {
    let CredentialKey::Secret { destination, name } = key else {
        return;
    };
    if context.env.contains_key(name.as_str()) {
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
        (Ok(true), CredentialPlace::File) => format!(
            "the credential file also has {name} for {destination}; ncp reads the native store first"
        ),
        (Ok(true), CredentialPlace::Native) => format!(
            "the native store also has {name} for {destination}, and ncp reads it first: the new value is not used"
        ),
    };
    context.ui.warning(None, &message);
    // The design gives this command: it removes the copies of this
    // destination, and then the user stores the value again in one place.
    context.ui.hint(
        None,
        &format!(
            "ncp secret delete {name} --server {}",
            render::shell_word(server.name.as_str())
        ),
    );
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
async fn read_value(context: &Context<'_>, name: &str, prompt: &str) -> Result<Secret, Failure> {
    let text = if ask::interactive() {
        read_hidden_line(context, prompt).await
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

/// All of stdin, for a value that a pipe or a file gives. The read runs on
/// a thread of its own, not of the runtime: a read of the runtime cannot
/// stop, and the exit after the time limit would wait for it.
async fn read_piped(context: &Context<'_>) -> Result<String, Unread> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut text);
        let _ = sender.send(read.map(|_| text));
    });
    tokio::select! {
        read = receiver => read
            .map_err(|_| Unread::Failed(std::io::Error::other("the reader stopped")))
            .and_then(|text| text.map_err(Unread::Failed)),
        () = tokio::time::sleep(context.request_limit) => Err(Unread::TimedOut),
        () = context.cancel.cancelled() => Err(Unread::Interrupted),
    }
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
async fn read_hidden_line(context: &Context<'_>, prompt: &str) -> Result<String, Unread> {
    let saved = stty(&["-g"]).map_err(Unread::Failed)?;
    eprint!("{prompt}");
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
    // A terminal with no echo is the larger problem, so its error comes
    // first, also after a failed read.
    restored.map_err(Unread::Failed)?;
    read
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

/// The secret keys of a store, as (name, destination), and the error of a
/// store that cannot answer. A host with no native store gives no error,
/// because the user then keeps all values in the file.
async fn secrets_in(
    context: &Context<'_>,
    store: &Opened,
) -> (Vec<(SecretName, Destination)>, Option<StoreError>) {
    let keys = match store {
        Ok(store) => call(context, store, |store| store.keys()).await,
        Err(StoreError::Unavailable(_)) => return (Vec::new(), None),
        Err(error) => Err(clone_error(error)),
    };
    match keys {
        Ok(keys) => (
            keys.into_iter()
                .filter_map(|key| match key {
                    CredentialKey::Secret { destination, name } => Some((name, destination)),
                    _ => None,
                })
                .collect(),
            None,
        ),
        Err(error) => (Vec::new(), Some(error)),
    }
}

/// The stored items with their places. `failed` has each place that could
/// not answer: the list can then miss items.
struct Stored {
    items: BTreeMap<(SecretName, Destination), Vec<&'static str>>,
    failed: Vec<(CredentialPlace, StoreError)>,
}

/// A warning for each place that could not answer (B30).
fn warn_unlisted(context: &Context<'_>, failed: &[(CredentialPlace, StoreError)]) {
    for (place, error) in failed {
        context.ui.warning(
            None,
            &format!("ncp cannot list {}: {error}", place_text(*place)),
        );
    }
}

/// The places of each stored item, from the keys of the two places.
fn items_by_place(
    native: Vec<(SecretName, Destination)>,
    file: Vec<(SecretName, Destination)>,
) -> BTreeMap<(SecretName, Destination), Vec<&'static str>> {
    let mut items: BTreeMap<(SecretName, Destination), Vec<&'static str>> = BTreeMap::new();
    let tagged = native
        .into_iter()
        .map(|item| (item, "native"))
        .chain(file.into_iter().map(|item| (item, "file")));
    for (item, place) in tagged {
        let places = items.entry(item).or_default();
        if !places.contains(&place) {
            places.push(place);
        }
    }
    items
}

async fn stored_items(context: &Context<'_>, native: &Opened) -> Result<Stored, Failure> {
    let (native_keys, native_failed) = secrets_in(context, native).await;
    if context.cancel.is_cancelled() {
        return Err(session::interrupted());
    }
    let (file_keys, file_failed) = secrets_in(context, &Ok(file(context))).await;
    if context.cancel.is_cancelled() {
        return Err(session::interrupted());
    }
    let failed = [
        native_failed.map(|error| (CredentialPlace::Native, error)),
        file_failed.map(|error| (CredentialPlace::File, error)),
    ];
    Ok(Stored {
        items: items_by_place(native_keys, file_keys),
        failed: failed.into_iter().flatten().collect(),
    })
}

/// One row of `secret list`: an item and the servers that use its key.
struct SecretRow {
    name: SecretName,
    destination: Destination,
    places: Vec<&'static str>,
    servers: Vec<String>,
}

fn secret_rows(
    items: BTreeMap<(SecretName, Destination), Vec<&'static str>>,
    servers: &[&Server],
) -> Vec<SecretRow> {
    items
        .into_iter()
        .map(|((name, destination), places)| {
            let uses: Vec<Use<'_>> = binding::uses(servers.iter().copied(), name.as_str())
                .into_iter()
                .filter(|item| item.destination.as_ref() == Ok(&destination))
                .collect();
            SecretRow {
                servers: server_names(&uses),
                name,
                destination,
                places,
            }
        })
        .collect()
}

async fn list(context: &Context<'_>) -> Result<(), Failure> {
    // The items come from the stores. A configuration file that does not
    // load only takes away the "used" mark.
    let loaded = match session::load_config(&context.paths.config_file, context.env) {
        Ok(loaded) => Some(loaded),
        Err(failure) => {
            context.ui.warning(
                None,
                &format!("{}; no secret shows as used", failure.message),
            );
            None
        }
    };
    let servers: Vec<&Server> = loaded
        .iter()
        .flat_map(|loaded| loaded.config.servers())
        .collect();
    let stored = stored_items(context, &native(context).await).await?;
    warn_unlisted(context, &stored.failed);
    let rows = secret_rows(stored.items, &servers);
    context.ui.result(
        || {
            render::table(
                &rows
                    .iter()
                    .map(|row| {
                        vec![
                            row.name.to_string(),
                            row.destination.to_string(),
                            row.places.join(", "),
                            if row.servers.is_empty() {
                                "not used".to_string()
                            } else {
                                row.servers.join(", ")
                            },
                        ]
                    })
                    .collect::<Vec<_>>(),
            )
        },
        || {
            json!({"secrets": rows.iter().map(|row| json!({
                "name": row.name.as_str(),
                "destination": destination_json(&row.destination),
                "places": row.places,
                "used": !row.servers.is_empty(),
                "servers": row.servers,
            })).collect::<Vec<_>>()})
        },
    )
}

/// The items that `secret delete` removes.
enum Removal {
    /// The item of the destination of this entry.
    Server(String),
    /// A choice from the list of the stored items.
    Listed(Listed),
}

/// The choice of `chosen`, with the destinations that the configuration
/// uses for `--unused`.
enum Pick {
    One,
    All,
    Unused {
        used: Vec<Destination>,
        config_file: std::path::PathBuf,
    },
}

enum Listed {
    /// The items that no entry of the loaded file uses.
    Unused,
    All,
    /// The item of the name when it has only one destination.
    One,
}

/// What `secret delete` removes: the destinations, the configuration file
/// that `ncp` read to choose them, and the native store for the removal.
/// `unlisted` has the places that could not list their items: they gave
/// their warning, and `ncp` does not ask them again.
struct Choice {
    destinations: Vec<Destination>,
    config_file: Option<std::path::PathBuf>,
    native: Opened,
    unlisted: Vec<CredentialPlace>,
}

/// The destinations to remove. With no flag, a place that cannot answer
/// stops the delete: an item there could be a second destination. A
/// native store that cannot list is not asked again, so that the delete
/// does not wait for its limit once more.
async fn chosen(
    context: &Context<'_>,
    name: &SecretName,
    removal: Removal,
) -> Result<Choice, Failure> {
    let not_stored = || Failure::usage(format!("{name} is not stored")).hint("ncp secret list");
    let listed = match removal {
        Removal::Server(word) => {
            // The configuration first: a usage error must not wait for the
            // native store.
            let loaded = session::load_config(&context.paths.config_file, context.env)?;
            let several_hint = format!("ncp secret delete {name} --all");
            let target = set_target(context, &loaded, name, Some(&word), Some(several_hint))?;
            return Ok(Choice {
                destinations: vec![target.destination],
                config_file: Some(loaded.path),
                native: native(context).await,
                unlisted: Vec::new(),
            });
        }
        Removal::Listed(listed) => listed,
    };
    // The configuration first: its usage error must not wait for the store.
    let pick = match listed {
        Listed::One => Pick::One,
        Listed::All => Pick::All,
        Listed::Unused => {
            let loaded = strict_config(context)?;
            let used =
                binding::by_destination(binding::uses(loaded.config.servers(), name.as_str()))
                    .into_iter()
                    .map(|target| target.destination)
                    .collect();
            Pick::Unused {
                used,
                config_file: loaded.path,
            }
        }
    };
    let native = native(context).await;
    let Stored { items, failed } = stored_items(context, &native).await?;
    let unlisted: Vec<CredentialPlace> = failed.iter().map(|(place, _)| *place).collect();
    let stored: Vec<Destination> = items
        .into_keys()
        .filter(|(item, _)| item == name)
        .map(|(_, destination)| destination)
        .collect();
    let choice = |destinations, config_file| Choice {
        destinations,
        config_file,
        native,
        unlisted,
    };
    match (pick, failed.first()) {
        (Pick::One, Some((place, error))) => {
            let failure = store_failure(error);
            Err(Failure {
                message: format!(
                    "ncp cannot list {}: {}; ncp removed nothing",
                    place_text(*place),
                    failure.message
                ),
                ..failure
            }
            .hint(format!("ncp secret delete {name} --server <SERVER>")))
        }
        (Pick::All | Pick::Unused { .. }, Some((_, error))) if stored.is_empty() => {
            Err(store_failure(error))
        }
        (Pick::One, None) => match stored.as_slice() {
            [] => Err(not_stored()),
            [_] => Ok(choice(stored, None)),
            _ => {
                let list: Vec<String> = stored.iter().map(ToString::to_string).collect();
                Err(Failure::usage(format!(
                    "{name} is stored for more than one destination: {}; ncp removed nothing",
                    list.join(", ")
                ))
                .hint(format!("ncp secret delete {name} --server <SERVER>")))
            }
        },
        (Pick::All, _) if stored.is_empty() => Err(not_stored()),
        (Pick::All, _) => {
            warn_unlisted(context, &failed);
            Ok(choice(stored, None))
        }
        (Pick::Unused { used, config_file }, first) => {
            let unused: Vec<Destination> = stored
                .into_iter()
                .filter(|destination| !used.contains(destination))
                .collect();
            match (unused.is_empty(), first) {
                // A place that cannot list can hold an unused item.
                (true, Some((_, error))) => Err(store_failure(error)),
                (true, None) => Err(
                    Failure::usage(format!("no stored item of {name} is unused"))
                        .hint("ncp secret list"),
                ),
                (false, _) => {
                    warn_unlisted(context, &failed);
                    Ok(choice(unused, Some(config_file)))
                }
            }
        }
    }
}

/// The configuration for `--unused`: an invalid entry can name the secret,
/// so `ncp` cannot know that an item is unused.
fn strict_config(context: &Context<'_>) -> Result<session::Loaded, Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)
        .map_err(|failure| Failure::usage(failure.message))?;
    if let Some((server, error)) = loaded.config.invalid().next() {
        return Err(Failure::usage(format!(
            "server {server}: {error}; ncp cannot know which items it uses"
        )));
    }
    Ok(loaded)
}

async fn delete(context: &Context<'_>, name: &str, removal: Removal) -> Result<(), Failure> {
    let name = secret_name(name)?;
    let Choice {
        destinations,
        config_file,
        native,
        unlisted,
    } = chosen(context, &name, removal).await?;
    // A place that could not list, or that is down, gives one warning, and
    // the delete does not ask it again.
    let mut stores = [
        (CredentialPlace::Native, Some(native)),
        (CredentialPlace::File, Some(Ok(file(context)))),
    ]
    .map(|(place, store)| (place, store.filter(|_| !unlisted.contains(&place))));
    let mut removed: Vec<(Destination, Vec<CredentialPlace>)> = Vec::new();
    let mut failed = None;
    for destination in destinations {
        let key = CredentialKey::Secret {
            destination: destination.clone(),
            name: name.clone(),
        };
        let mut places = Vec::new();
        for (place, slot) in &mut stores {
            let Some(store) = slot else {
                continue;
            };
            // One call that removes and tells if the item was there: a
            // second call could pass the limit and remove the item after
            // the exit.
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
                Ok(true) => places.push(*place),
                Ok(false) => {}
                Err(StoreError::Interrupted) => return Err(session::interrupted()),
                Err(error) => {
                    context.ui.warning(
                        None,
                        &format!("ncp cannot change {}: {error}", place_text(*place)),
                    );
                    if store_is_down(&error) {
                        *slot = None;
                    }
                    failed = Some(error);
                }
            }
        }
        if !places.is_empty() {
            removed.push((destination, places));
        }
    }
    match (removed.is_empty(), failed) {
        (true, Some(error)) => Err(store_failure(&error)),
        (true, None) => {
            Err(Failure::usage(format!("{name} is not stored")).hint("ncp secret list"))
        }
        (false, _) => {
            crate::list_cache::remove_all(&context.paths.cache_dir);
            context.ui.result(
                || {
                    removed
                        .iter()
                        .map(|(destination, places)| {
                            let text: Vec<&str> =
                                places.iter().map(|place| place_text(*place)).collect();
                            format!(
                                "Removed {name} for {destination} from {}.\n",
                                text.join(" and ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .concat()
                },
                || {
                    json!({
                        "name": name.as_str(),
                        "file": config_file.as_ref().map(|path| path.display().to_string()),
                        "removed": removed.iter().map(|(destination, places)| json!({
                            "destination": destination_json(destination),
                            "places": places.iter().map(|place| place_name(*place)).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    })
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, destination: &str) -> (SecretName, Destination) {
        let destination = match destination {
            "stdio" => Destination::Stdio,
            origin => Destination::Origin(crate::server::Origin::parse(origin).unwrap()),
        };
        (SecretName::parse(name).unwrap(), destination)
    }

    #[test]
    fn secret_rows_merge_places_and_name_the_servers_of_each_key() {
        let config = crate::config::parse(
            r#"{"mcpServers": {
                "a": {"type": "http", "url": "https://h/mcp", "headers": {"X": "${K}"}},
                "b": {"type": "http", "url": "https://other/mcp", "headers": {"X": "${K}"}},
                "s": {"command": "s", "env": {"K": "${K}"}}
            }}"#,
            &Env::new(),
        )
        .unwrap();
        let servers: Vec<&Server> = config.servers().collect();
        let items = items_by_place(
            vec![item("K", "https://h"), item("K", "stdio")],
            vec![
                item("K", "https://h"),
                item("K", "https://old"),
                item("C", "stdio"),
            ],
        );
        let rows = secret_rows(items, &servers);
        let view: Vec<(String, String, Vec<&str>, Vec<String>)> = rows
            .into_iter()
            .map(|row| {
                (
                    row.name.to_string(),
                    row.destination.to_string(),
                    row.places,
                    row.servers,
                )
            })
            .collect();
        let row = |name: &str, destination: &str, places: &[&'static str], servers: &[&str]| {
            (
                name.to_string(),
                destination.to_string(),
                places.to_vec(),
                servers.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            view,
            [
                row("C", "stdio servers", &["file"], &[]),
                row("K", "https://h", &["native", "file"], &["a"]),
                row("K", "https://old", &["file"], &[]),
                row("K", "stdio servers", &["native"], &["s"]),
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
