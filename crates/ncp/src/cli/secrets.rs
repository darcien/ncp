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
    outcome::{Exit, Failure, Hint, Warning, chain_text},
    project_file::ProjectFile,
    render,
    session::{self, Context},
};
use crate::{
    binding::{self, Field, Target, TargetError, Use},
    config::{ClientKind, EntryError, PreRegisteredClient, Server, Values},
    credential::{CredentialKey, Destination, Secret, SecretName},
    expand::{self, Env},
    project::Trust,
    server::{Origin, OriginError},
    store::{self, CredentialStore, FileStore, StoreError},
    text::code,
};

/// Tests replace the native store with a fake. The limit applies to each
/// run of a store program.
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

/// A store program stops before the limit of the call, so no program stays
/// after `ncp`.
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

pub(super) fn secret_name(name: &str) -> Result<SecretName, Failure> {
    SecretName::parse(name)
        .map_err(|error| Failure::usage(error.to_string()).hint(super::fixes::secret_name_hint()))
}

// A destination in a message: an origin in backticks, or "stdio servers".
fn destination_text(destination: &Destination) -> String {
    match destination {
        Destination::Origin(origin) => code(origin.as_str()),
        Destination::Stdio => "stdio servers".to_string(),
    }
}

// `${NAME}` in backticks.
fn reference(name: &str) -> String {
    code(&format!("${{{name}}}"))
}

/// The values of an entry, and the names whose values came from a store.
pub(super) struct Resolved {
    pub values: Values,
    pub stored: Vec<SecretName>,
}

pub(super) async fn resolve(context: &Context<'_>, server: &Server) -> Result<Resolved, Failure> {
    let destination = binding::value_destination(server);
    let (values, stored) = values_of(context, server, server.value_names(), destination).await?;
    let values = server
        .expand_values(&values)
        .map_err(|error| entry_failure(server, &error))?;
    Ok(Resolved { values, stored })
}

/// A stored value comes only from the item of the origin of
/// `oauth.issuer`. No error shows the value.
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
            entry_failure(server, &error)
        })
}

// The entry is valid in the file, but a value from a store does not fit
// into it.
fn entry_failure(server: &Server, error: &EntryError) -> Failure {
    let name = server.name.as_str();
    let failure = Failure::usage(format!(
        "The entry of the server {} is not valid",
        code(name)
    ))
    .about(name)
    .caused_by(error);
    super::fixes::with_entry_hints(failure, error)
}

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
    let destination = destination.map_err(|error| not_literal(server, &missing, &error))?;
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
    for (name, secret, place) in found? {
        context.ui.debug(
            Some(name_of_server),
            &format!("{name} from {place}, for {destination}"),
        );
        values.insert(name.to_string(), secret.expose().to_string());
        stored.push(name);
    }
    Ok((values, stored))
}

// The stop for a stored value of an entry with no literal origin. An
// environment value has no binding, so the fix is the URL, not an export.
fn not_literal(server: &Server, names: &[&str], error: &OriginError) -> Failure {
    let name = server.name.as_str();
    let variables: Vec<String> = names.iter().map(|name| code(name)).collect();
    let subject = match variables.as_slice() {
        [one] => format!("The variable {one} of the server {} is not set", code(name)),
        _ => format!(
            "The variables {} of the server {} are not set",
            variables.join(", "),
            code(name)
        ),
    };
    Failure::usage(format!(
        "{subject}, and ncp sends a stored value only to a literal origin"
    ))
    .about(name)
    .caused_by(error)
}

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
            | StoreError::Locked(_)
            | StoreError::NoAnswer(_)
            | StoreError::Interrupted
    )
}

// The value of one key: the native store first, then the file. A native store
// that is down does not stop the search, because the file can have the item.
// Its error replaces the store, so that the next name does not wait for the
// time limit again. An error of one item stops the search, because then a
// value from the file can replace a newer value. No item of another
// destination is a fallback.
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
            Err(error) => return Err(read_failure(server, name, CredentialPlace::Native, &error)),
        }
    }
    if let Err(StoreError::Interrupted) = native {
        return Err(session::interrupted(Some(server.name.as_str())));
    }
    let wanted = key.clone();
    if let Some(secret) = call(context, file, move |store| store.load(&wanted))
        .await
        .map_err(|error| read_failure(server, name, CredentialPlace::File, &error))?
    {
        return Ok((secret, place_text(CredentialPlace::File)));
    }
    Err(miss(server, name, destination, native.as_ref().err()))
}

// A store that fails for one item. The server and the name are in the
// message, because the store text names neither.
fn read_failure(
    server: &Server,
    name: &SecretName,
    place: CredentialPlace,
    error: &StoreError,
) -> Failure {
    let server = server.name.as_str();
    match error {
        StoreError::Interrupted => session::interrupted(Some(server)),
        _ => store_failure(error)
            .context(format!(
                "ncp cannot read the value of {} for the server {} from {}",
                code(name.as_str()),
                code(server),
                place_text(place)
            ))
            .about(server),
    }
}

// The message names only the current destination, because `ncp` reads no
// other item.
fn miss(
    server: &Server,
    name: &SecretName,
    destination: &Destination,
    native_error: Option<&StoreError>,
) -> Failure {
    let word = render::shell_word(server.name.as_str());
    let set = format!("ncp secret set {name} --server {word}");
    let shown = destination_text(destination);
    let (stop, store) = match destination {
        Destination::Origin(_) => (
            "ncp sent nothing to",
            Hint::text(format!(
                "If the URL is correct, run `{set}` to store the value"
            )),
        ),
        Destination::Stdio => ("ncp did not start", Hint::run_text("store the value", &set)),
    };
    let stop = format!("{stop} the server {}.", code(server.name.as_str()));
    let failure = match native_error {
        None => Failure::usage(format!(
            "The variable {} is not set, and no store has a value for {shown}. {stop}",
            code(name.as_str())
        ))
        .hint(store)
        .hint(list_hint()),
        Some(error) => Failure::new(
            Exit::Auth,
            format!(
                "The variable {} is not set, and the native store gave no value for {shown}. {stop}",
                code(name.as_str())
            ),
        )
        .caused_by(error)
        .with_hints(super::fixes::store_hints(error))
        .hint(Hint::run_text(
            "store the value in the credential file",
            &format!("{set} --credential-store file"),
        )),
    };
    failure
        .about(server.name.as_str())
        .with_destination(destination.clone())
}

fn list_hint() -> Hint {
    Hint::run(
        "see the stored items and their destinations",
        "ncp secret list",
    )
}

// The error again, for a second use. A store error holds no secret.
fn clone_error(error: &StoreError) -> StoreError {
    match error {
        StoreError::Unavailable(detail) => StoreError::Unavailable(detail.clone()),
        StoreError::Denied(detail) => StoreError::Denied(detail.clone()),
        StoreError::Locked(count) => StoreError::Locked(*count),
        StoreError::Ambiguous(count) => StoreError::Ambiguous(*count),
        StoreError::Failure(detail) => StoreError::Failure(detail.clone()),
        StoreError::NoAnswer(limit) => StoreError::NoAnswer(*limit),
        StoreError::Interrupted => StoreError::Interrupted,
        other => StoreError::Failure(chain_text(other).into()),
    }
}

/// A store error. The native store and its time limit are authorization
/// errors (exit 4). A credential file that `ncp` cannot read or write is a
/// local error (exit 3).
/// A store failure with a message that names the work, and the hints of
/// the call site after the hints of the store error. Ctrl-C stays the
/// interrupt, with no other message and no hint.
pub(super) fn store_failure_of(
    error: &StoreError,
    message: String,
    hints: impl IntoIterator<Item = Hint>,
) -> Failure {
    match error {
        StoreError::Interrupted => session::interrupted(None),
        _ => hints
            .into_iter()
            .fold(store_failure(error).context(message), Failure::hint),
    }
}

pub(super) fn store_failure(error: &StoreError) -> Failure {
    let exit = match error {
        StoreError::Interrupted => return session::interrupted(None),
        StoreError::Io { .. } | StoreError::OpenPermissions(_) | StoreError::Corrupt(_) => {
            Exit::Protocol
        }
        StoreError::Unavailable(_)
        | StoreError::Denied(_)
        | StoreError::Locked(_)
        | StoreError::Ambiguous(_)
        | StoreError::Failure(_)
        | StoreError::NoAnswer(_) => Exit::Auth,
    };
    // Rule 7 drops the error itself from the causes: the message holds it.
    let failure = Failure::new(exit, error.to_string()).caused_by(error);
    super::fixes::store_hints(error)
        .into_iter()
        .fold(failure, Failure::hint)
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

// The entries that a destination is for, as text: "server linear", or
// "the issuer of server acme" for a client secret.
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

// The one destination that `secret set` (and `secret delete --server`)
// uses: the entries that name `${NAME}` in `env`, `headers` or
// `oauth.clientSecret`, or only the entry of `--server`.
fn set_target<'a>(
    context: &Context<'_>,
    loaded: &'a session::Loaded,
    name: &SecretName,
    only: Option<&str>,
    several_hint: Option<Hint>,
) -> Result<Target<'a>, Failure> {
    let servers: Vec<&Server> = match only {
        Some(word) => vec![session::select(loaded, word, context.ui)?],
        None => loaded.config.servers().collect(),
    };
    let uses = binding::uses(servers.iter().copied(), name.as_str());
    binding::target(uses).map_err(|error| match error {
        TargetError::NoUse => no_use(name, only, loaded),
        TargetError::NotLiteral { server, error } => Failure::usage(format!(
            "The server {} names {}, and ncp sends a stored value only to a literal origin",
            code(server.name.as_str()),
            reference(name.as_str())
        ))
        .about(server.name.as_str())
        .caused_by(&error),
        TargetError::Several(targets) => {
            let list = targets
                .iter()
                .map(|target| format!("{} ({})", target.destination, users_text(&target.uses)));
            // One entry with two destinations: `--server` cannot choose, so
            // only a second name helps.
            if let Some(word) = only {
                let failure = Failure::usage(format!(
                    "The server {} names {} for more than one destination:",
                    code(word),
                    reference(name.as_str())
                ))
                .about(word);
                let failure = list
                    .fold(failure, Failure::data)
                    .hint("Use one name for each field");
                return match several_hint {
                    Some(fix) => failure.hint(fix),
                    None => failure,
                };
            }
            let failure = Failure::usage(format!(
                "The entries give {} more than one destination:",
                reference(name.as_str())
            ));
            list.fold(failure, Failure::data).hint(Hint::text(format!(
                "To store the value for one server, run `ncp secret set {name} --server <SERVER>`"
            )))
        }
    })
}

// No entry names `${NAME}` in a field that can read the store. A name in
// `command`, `args`, `cwd` or `url` comes only from the environment (B32),
// and an unset one makes the entry invalid.
fn no_use(name: &SecretName, only: Option<&str>, loaded: &session::Loaded) -> Failure {
    if let Some(failure) = untrusted_use(loaded, name, only) {
        return failure;
    }
    let fields = "`env`, `headers` or `oauth.clientSecret`";
    let failure = match only {
        Some(word) => Failure::usage(format!(
            "The server {} does not name {} in {fields}",
            code(word),
            reference(name.as_str())
        ))
        .about(word),
        None => Failure::usage(format!(
            "No entry names {} in {fields}",
            reference(name.as_str())
        )),
    };
    let unset = loaded.config.invalid().any(|(server, error)| {
        only.is_none_or(|word| word == server)
            && matches!(
                error,
                EntryError::Variable { error: expand::ExpandError::Unset(unset), .. }
                    if unset == name.as_str()
            )
    });
    // An entry that names the variable in another field has it unset, so
    // the fix is the environment, not a new entry.
    match (unset, only) {
        (true, _) => failure.hint(format!(
            "To set the variable, run `export {name}=<value>`. Then run the command again."
        )),
        (false, None) => failure.hint(format!(
            "Add an entry that names {} first. Then run the command again.",
            reference(name.as_str())
        )),
        (false, Some(_)) => failure,
    }
}

async fn set(
    context: &Context<'_>,
    name: &str,
    only: Option<&str>,
    place: CredentialPlace,
) -> Result<(), Failure> {
    let name = secret_name(name)?;
    let loaded = session::load(context, only).await?;
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
            store_failure_of(
                &error,
                "ncp cannot open the native store".to_string(),
                [Hint::run_text(
                    "store the value in the credential file",
                    &format!("ncp secret set {name} --server {word} --credential-store file"),
                )],
            )
        })?,
        CredentialPlace::File => file(context),
    };
    if let Destination::Origin(origin) = destination
        && let Some(host) = url::Url::parse(origin.as_str())
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .filter(|host| ask::has_punycode_label(host))
    {
        context.ui.warn(&Warning::new(format!(
            "The host {} has a Punycode label (`xn--`), so its name can look like another name",
            code(&host)
        )));
    }
    let users = users_text(&target.uses);
    let prompt = format!("Value of {name} for {destination} ({users}): ");
    let pipe = format!("ncp secret set {name} < FILE");
    let value = read_value(context, name.as_str(), &prompt, &pipe).await?;
    let saved = key.clone();
    call(context, &store, move |store| store.save(&saved, &value))
        .await
        .map_err(|error| {
            let message = format!(
                "ncp cannot store the value of {} in {}",
                code(name.as_str()),
                place_text(place)
            );
            store_failure_of(&error, message, [])
        })?;
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
                "credentialStore": place_name(place),
                "destination": destination_json(destination),
                "servers": server_names(&target.uses),
            })
        },
    )
}

/// The copy check only gives a warning, so it must not wait for a locked
/// store.
const COPY_CHECK_LIMIT: Duration = Duration::from_secs(5);

// Warnings when another place has a value for the key: `ncp` reads the
// environment first, then the native store, then the file.
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
        context.ui.warn(&Warning::new(format!(
            "The variable {} is also set in the environment. ncp uses the value of the environment first.",
            code(name.as_str())
        )));
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
            // `ncp` knows no copy, so no command can fix it.
            let message = format!(
                "ncp cannot check {} for a copy of {}",
                place_text(other),
                code(name.as_str())
            );
            context
                .ui
                .warn(&Warning::from(store_failure_of(&error, message, [])));
            return;
        }
        (Ok(true), CredentialPlace::File) => format!(
            "The credential file also has an item of {} for {}. ncp reads the native store first.",
            code(name.as_str()),
            destination_text(destination)
        ),
        (Ok(true), CredentialPlace::Native) => format!(
            "The native store also has an item of {} for {}. ncp reads the native store first and does not use the new value.",
            code(name.as_str()),
            destination_text(destination)
        ),
    };
    // The command removes the copies of this destination. Then the user
    // stores the value again in one place.
    context.ui.warn(&Warning::new(message).hint(format!(
        "To delete the items of this destination, run `ncp secret delete {name} --server {}`. Then store the value again.",
        render::shell_word(server.name.as_str())
    )));
}

enum Unread {
    Interrupted,
    TimedOut,
    Failed(std::io::Error),
}

// Reads the value from stdin. When a person can answer (stdin and stderr are a
// TTY), `ncp` does not show the input. A TTY on stdin with no TTY on stderr is
// an error. In that case the question does not show, and the terminal echoes
// the value. Each read has the limit of `--timeout`. One new line at the end
// is not part of the value, because `echo` and a pasted line add it.
async fn read_value(
    context: &Context<'_>,
    name: &str,
    prompt: &str,
    pipe: &str,
) -> Result<Secret, Failure> {
    let text = if ask::interactive() {
        read_hidden_line(context, prompt).await
    } else if std::io::stdin().is_terminal() {
        return Err(Failure::usage(format!(
            "ncp cannot ask for the value of {}, because stderr is not a terminal",
            code(name)
        ))
        .hint(Hint::run_text("give the value from a file", pipe)));
    } else {
        read_piped(context).await
    };
    let text = text.map_err(|unread| match unread {
        Unread::Interrupted => session::interrupted(None),
        Unread::TimedOut => Failure::protocol(format!(
            "No value of {} came in {} s",
            code(name),
            context.request_limit.as_secs()
        ))
        .hint("To wait longer, run the command again with `--timeout <SECONDS>`"),
        Unread::Failed(error) => Failure::usage(format!(
            "ncp cannot read the value of {} from stdin",
            code(name)
        ))
        .caused_by(&error),
    })?;
    let line = text.strip_suffix('\n').unwrap_or(&text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return Err(Failure::usage(format!(
            "The value of {} is empty",
            code(name)
        )));
    }
    Ok(Secret::new(line.to_string()))
}

// The read runs on its own thread, not on the runtime. A read on the runtime
// cannot stop, and the exit after the time limit waits for it.
async fn read_piped(context: &Context<'_>) -> Result<String, Unread> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut text);
        let _ = sender.send(read.map(|_| text));
    });
    tokio::select! {
        read = receiver => read
            .map_err(|_| Unread::Failed(std::io::Error::other("The reader stopped")))
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
            "{} failed",
            code(&format!("{STTY} {}", args.join(" ")))
        )))
    }
}

// One line from the terminal, with the echo off. The terminal settings
// come back after each end: the line, an error, the time limit or Ctrl-C.
// Thus the shell of the user works again.
//
// The terminal is in non-canonical mode: in canonical mode macOS keeps only
// 1024 bytes of a line, and a longer token never ends. Thus `ncp` does the
// backspace itself.
async fn read_hidden_line(context: &Context<'_>, prompt: &str) -> Result<String, Unread> {
    let saved = stty(&["-g"]).map_err(Unread::Failed)?;
    eprint!("{prompt}");
    // Fail closed: with no echo control, the value shows.
    stty(&["-echo", "-icanon", "min", "1", "time", "0"]).map_err(Unread::Failed)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    // The read runs on its own thread, so that the runtime does not wait
    // for it at the exit after Ctrl-C or the time limit.
    std::thread::spawn(move || {
        let _ = sender.send(read_terminal_line(&mut std::io::stdin().lock()));
    });
    let read = tokio::select! {
        read = receiver => read
            .map_err(|_| Unread::Failed(std::io::Error::other("The reader stopped")))
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

// Reads bytes until a new line, a carriage return or Ctrl-D. A backspace
// or DEL removes the last character.
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
    String::from_utf8(line).map_err(|_| std::io::Error::other("The value is not UTF-8 text"))
}

// The secret keys of a store, as (name, destination), and the error of a
// store that cannot answer. A host with no native store gives no error,
// because the user then keeps all values in the file.
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

/// The stored items with their places. `failed` has each place that did not
/// answer: the list can then miss items.
struct Stored {
    items: BTreeMap<(SecretName, Destination), Vec<&'static str>>,
    failed: Vec<(CredentialPlace, StoreError)>,
}

// A warning for each place that did not answer (B30).
fn warn_unlisted(context: &Context<'_>, failed: &[(CredentialPlace, StoreError)]) {
    for (place, error) in failed {
        context
            .ui
            .warn(&Warning::from(unlisted_failure(*place, error)));
    }
}

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
        return Err(session::interrupted(None));
    }
    let (file_keys, file_failed) = secrets_in(context, &Ok(file(context))).await;
    if context.cancel.is_cancelled() {
        return Err(session::interrupted(None));
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

/// One row of `secret list`: a name and its destination, the places that
/// have it, and the servers that use it.
pub(super) struct SecretRow {
    pub name: SecretName,
    pub destination: Destination,
    pub places: Vec<&'static str>,
    pub servers: Vec<String>,
}

impl SecretRow {
    pub(super) fn json(&self) -> serde_json::Value {
        json!({
            "name": self.name.as_str(),
            "destination": destination_json(&self.destination),
            "credentialStores": self.places,
            "used": !self.servers.is_empty(),
            "servers": self.servers,
        })
    }

    /// The places as words: "the native store and the credential file".
    pub(super) fn places_text(&self) -> String {
        self.places
            .iter()
            .map(|place| match *place {
                "native" => place_text(CredentialPlace::Native),
                _ => place_text(CredentialPlace::File),
            })
            .collect::<Vec<_>>()
            .join(" and ")
    }
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

/// `server add --secret NAME` (docs/design/server-add.md rules 11 and 13).
/// Before the read, `ncp` finds the stores that have the item, so that a
/// store that cannot answer stops before the user types. After the read,
/// a different stored value is an error, because other entries of the
/// origin use it. The same value passes, so that a retry after a failure
/// works.
pub(super) async fn store_for_add(
    context: &Context<'_>,
    name: &SecretName,
    server: &Server,
    place: CredentialPlace,
) -> Result<(), Failure> {
    let destination = binding::value_destination(server).map_err(|error| {
        Failure::usage(format!(
            "ncp cannot store a value of {} for the server {}",
            code(name.as_str()),
            code(server.name.as_str())
        ))
        .about(server.name.as_str())
        .caused_by(&error)
    })?;
    let key = CredentialKey::Secret {
        destination: destination.clone(),
        name: name.clone(),
    };
    let word = render::shell_word(server.name.as_str());
    let places = places(context).await;
    let chosen = match (place, &places[0].1) {
        (CredentialPlace::Native, Ok(store)) => store.clone(),
        (CredentialPlace::Native, Err(error)) => {
            return Err(store_failure_of(
                error,
                "ncp cannot open the native store".to_string(),
                [file_flag_hint()],
            ));
        }
        (CredentialPlace::File, _) => file(context),
    };
    let mut stored = Vec::new();
    for (other, opened) in places {
        let read_failure = |error: &StoreError| {
            let message = format!(
                "ncp cannot read {} for a value of {}",
                place_text(other),
                code(name.as_str())
            );
            store_failure_of(error, message, [])
        };
        let store = match opened {
            Ok(store) => store,
            Err(StoreError::Unavailable(_)) => continue,
            Err(error) => return Err(read_failure(&error)),
        };
        let wanted = key.clone();
        match call(context, &store, move |store| store.load(&wanted)).await {
            Ok(Some(value)) => stored.push((other, value)),
            Ok(None) => {}
            Err(error) => return Err(read_failure(&error)),
        }
    }
    let prompt = format!("Value of {name} for {destination}: ");
    let pipe = format!("ncp server add {word} ... --secret {name} < FILE");
    let value = read_value(context, name.as_str(), &prompt, &pipe).await?;
    if let Some((other, _)) = stored.iter().find(|(_, old)| *old != value) {
        return Err(Failure::usage(format!(
            "{} already has another value of {} for {}. Other entries with this origin use it.",
            capitalized(place_text(*other)),
            code(name.as_str()),
            destination_text(&destination)
        ))
        .hint(Hint::run_text(
            &format!(
                "store a value for the server {} only",
                code(server.name.as_str())
            ),
            &format!("ncp secret set {name} --server {word}"),
        )));
    }
    if stored.iter().any(|(other, _)| *other == place) {
        return Ok(());
    }
    call(context, &chosen, move |store| store.save(&key, &value))
        .await
        .map_err(|error| {
            let message = format!(
                "ncp cannot store the value of {} in {}",
                code(name.as_str()),
                place_text(place)
            );
            // The file is the fix only for a failure of the native store.
            let hints = (place == CredentialPlace::Native).then(file_flag_hint);
            store_failure_of(&error, message, hints)
        })?;
    crate::list_cache::remove_all(&context.paths.cache_dir);
    Ok(())
}

fn file_flag_hint() -> Hint {
    Hint::text(
        "To store the value in the credential file, run the command again with `--credential-store file`",
    )
}

/// The two places of stored items, the native store first.
pub(super) async fn places(context: &Context<'_>) -> [(CredentialPlace, Opened); 2] {
    [
        (CredentialPlace::Native, native(context).await),
        (CredentialPlace::File, Ok(file(context))),
    ]
}

/// The rows of `secret list` for each `${NAME}` of the new entry. A reused
/// value shows, so that the user sees that it now also goes to this entry.
pub(super) async fn rows_of_entry(
    context: &Context<'_>,
    entry: &Server,
    servers: &[&Server],
) -> Vec<SecretRow> {
    // An entry with no `${NAME}` needs no list of the stores, which can
    // show a prompt of the native store.
    let names: Vec<SecretName> = entry
        .value_names()
        .into_iter()
        .filter_map(|name| SecretName::parse(name).ok())
        .collect();
    let Ok(destination) = binding::value_destination(entry) else {
        return Vec::new();
    };
    if names.is_empty() {
        return Vec::new();
    }
    let stored = match stored_items(context, &native(context).await).await {
        Ok(stored) => {
            warn_unlisted(context, &stored.failed);
            stored.items
        }
        Err(failure) => {
            context.ui.warn(&Warning::from(failure));
            BTreeMap::new()
        }
    };
    let items = names
        .into_iter()
        .map(|name| {
            let key = (name, destination.clone());
            let places = stored.get(&key).cloned().unwrap_or_default();
            (key, places)
        })
        .collect();
    secret_rows(items, servers)
}

/// `ncp` knows only the global file and the project file of the current
/// folder. A list of the project files, as `mise prune` keeps, comes after
/// the first release.
const UNUSED_SCOPE: &str = "ncp examined only the global file and the project file of this folder. A project in another folder can still use the item.";

async fn list(context: &Context<'_>) -> Result<(), Failure> {
    // The items come from the stores. A configuration file that does not
    // load only takes away the "used" mark.
    let loaded = match session::load(context, None).await {
        Ok(loaded) => Some(loaded),
        Err(failure) => {
            context.ui.warn(&Warning::from(
                failure.context("ncp cannot read the configuration. No secret shows as used."),
            ));
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
    if rows.iter().any(|row| row.servers.is_empty()) {
        context.ui.hint(None, UNUSED_SCOPE);
    }
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
        || json!({"secrets": rows.iter().map(SecretRow::json).collect::<Vec<_>>()}),
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

/// What `secret delete` removes: the destinations, the configuration file that
/// `ncp` read to choose them, and the native store for the removal. `unlisted`
/// has the places that did not list their items: they gave their warning, and
/// `ncp` does not ask them again.
struct Choice {
    destinations: Vec<Destination>,
    config_file: Option<std::path::PathBuf>,
    native: Opened,
    unlisted: Vec<CredentialPlace>,
}

// The destinations to remove. With no flag, a place that cannot answer stops
// the delete: an item there can be a second destination. `ncp` does not ask a
// native store that cannot list again, so that the delete does not wait for
// its limit once more.
async fn chosen(
    context: &Context<'_>,
    name: &SecretName,
    removal: Removal,
) -> Result<Choice, Failure> {
    let not_stored = || not_stored(name);
    let listed = match removal {
        Removal::Server(word) => {
            // The configuration first: a usage error must not wait for the
            // native store.
            let loaded = session::load(context, Some(&word)).await?;
            let several_hint = Hint::run_text(
                &format!("delete all items of {}", code(name.as_str())),
                &format!("ncp secret delete {name} --all"),
            );
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
            let loaded = strict_config(context, name).await?;
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
        (Pick::One, Some((place, error))) => Err(one_unlisted(name, *place, error)),
        (Pick::All | Pick::Unused { .. }, Some((place, error))) if stored.is_empty() => {
            Err(unlisted_failure(*place, error))
        }
        (Pick::One, None) => match stored.as_slice() {
            [] => Err(not_stored()),
            [_] => Ok(choice(stored, None)),
            _ => {
                let failure = Failure::usage(format!(
                    "ncp deleted nothing, because the stores have items of {} for more than one destination:",
                    code(name.as_str())
                ));
                Err(stored
                    .iter()
                    .map(ToString::to_string)
                    .fold(failure, Failure::data)
                    .hint(one_server_hint(name)))
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
                (true, Some((place, error))) => Err(unlisted_failure(*place, error)),
                (true, None) => Err(Failure::usage(format!(
                    "No stored item of {} is unused",
                    code(name.as_str())
                ))
                .hint(list_hint())),
                (false, _) => {
                    warn_unlisted(context, &failed);
                    context.ui.hint(None, UNUSED_SCOPE);
                    Ok(choice(unused, Some(config_file)))
                }
            }
        }
    }
}

fn not_stored(name: &SecretName) -> Failure {
    Failure::usage(format!("No store has an item of {}", code(name.as_str()))).hint(list_hint())
}

fn one_server_hint(name: &SecretName) -> Hint {
    Hint::text(format!(
        "To delete only the item of one server, run `ncp secret delete {name} --server <SERVER>`"
    ))
}

// With no flag, a place that cannot list stops the delete: it can hold an
// item of a second destination.
fn one_unlisted(name: &SecretName, place: CredentialPlace, error: &StoreError) -> Failure {
    let message = format!(
        "ncp cannot list the items in {}. ncp deleted nothing.",
        place_text(place)
    );
    let hint = Hint::text(format!(
        "{} can have an item of {} for another destination. {}.",
        capitalized(place_text(place)),
        code(name.as_str()),
        one_server_hint(name).message()
    ));
    store_failure_of(error, message, [hint])
}

// A place that cannot list can hold the item.
fn unlisted_failure(place: CredentialPlace, error: &StoreError) -> Failure {
    let message = format!("ncp cannot list the items in {}", place_text(place));
    store_failure_of(error, message, [])
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

// An invalid entry, or an entry of an untrusted project file, can name the
// secret. Then `ncp` cannot know that no entry uses an item, so `--unused`
// stops.
async fn strict_config(
    context: &Context<'_>,
    name: &SecretName,
) -> Result<session::Loaded, Failure> {
    let unknown = " ncp cannot know which items it uses.";
    let loaded = session::load(context, None).await.map_err(|failure| {
        failure
            .context("ncp cannot know which items the configuration uses")
            .with_exit(Exit::Usage)
    })?;
    // The file names the place, because a global entry and a project
    // entry can have the same name.
    let invalid_entry = |server: &str, error: &EntryError, file: &std::path::Path| {
        let failure = Failure::usage(format!(
            "The entry of the server {} in {} is not valid.{unknown}",
            code(server),
            code(&file.display().to_string())
        ))
        .about(server)
        .caused_by(error);
        super::fixes::with_entry_hints(failure, error)
    };
    if let Some((server, error)) = loaded.config.invalid().next() {
        return Err(invalid_entry(server, error, loaded.file_of(server)));
    }
    let waiting = match &loaded.project {
        ProjectFile::Unusable { file, reason } => {
            return Err(Failure::usage(format!(
                "ncp does not use {}.{unknown}",
                code(&file.display().to_string())
            ))
            .cause(reason.as_str()));
        }
        ProjectFile::Read(project) if project.trust != Trust::Trusted => Some(project),
        _ => None,
    };
    let invalid = waiting.and_then(|project| {
        project
            .entries
            .iter()
            .find_map(|entry| match &entry.shape {
                Err(error) => Some((&entry.name, error)),
                Ok(shape) => shape.problem.as_ref().map(|problem| (&entry.name, problem)),
            })
            .map(|(server, error)| (server, error, &project.file))
    });
    if let Some((server, error, file)) = invalid {
        return Err(invalid_entry(server, error, file).hint(trust_hint()));
    }
    if let Some(failure) = untrusted_use(&loaded, name, None) {
        let message = format!("{}.{unknown}", failure.message());
        return Err(failure.context(message).with_hints([trust_hint()]));
    }
    Ok(loaded)
}

fn trust_hint() -> Hint {
    Hint::run_text("trust the file", "ncp trust")
}

// The failure when only an entry of an untrusted project file names
// `${NAME}`: the entry, the file and its state.
fn untrusted_use(
    loaded: &session::Loaded,
    name: &SecretName,
    only: Option<&str>,
) -> Option<Failure> {
    let project = loaded.project.project()?;
    let state = match project.trust {
        Trust::Trusted => return None,
        Trust::Ignored => "which you ignored",
        Trust::Untrusted { .. } => "which you did not trust",
    };
    let entry = project.entries.iter().find(|entry| {
        only.is_none_or(|word| word == entry.name)
            && entry
                .shape
                .as_ref()
                .is_ok_and(|shape| shape.variables.contains(name.as_str()))
    })?;
    Some(
        Failure::usage(format!(
            "The server {} names {} in {}, {state}",
            code(&entry.name),
            reference(name.as_str()),
            code(&project.file.display().to_string())
        ))
        .about(entry.name.as_str())
        .hint(trust_hint()),
    )
}

async fn delete(context: &Context<'_>, name: &str, removal: Removal) -> Result<(), Failure> {
    let name = secret_name(name)?;
    let Choice {
        destinations,
        config_file,
        native,
        unlisted,
    } = chosen(context, &name, removal).await?;
    // A place that did not list, or that is down, gives one warning, and the
    // delete does not ask it again.
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
            // One call that removes and tells if the item was there: a second
            // call can pass the limit and remove the item after the exit.
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
                Err(StoreError::Interrupted) => return Err(session::interrupted(None)),
                Err(error) => {
                    context
                        .ui
                        .warn(&Warning::from(store_failure(&error).context(format!(
                            "ncp cannot delete the item of {} in {}",
                            code(name.as_str()),
                            place_text(*place)
                        ))));
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
        (true, Some(error)) => Err(store_failure_of(
            &error,
            format!("ncp cannot delete the items of {}", code(name.as_str())),
            [],
        )),
        (true, None) => Err(not_stored(&name)),
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
                                "Deleted {name} for {destination} from {}.\n",
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
                        "deleted": removed.iter().map(|(destination, places)| json!({
                            "destination": destination_json(destination),
                            "credentialStores": places.iter().map(|place| place_name(*place)).collect::<Vec<_>>(),
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
            store_failure(&StoreError::NoAnswer(std::time::Duration::from_secs(60))).exit(),
            Exit::Auth
        );
        assert_eq!(
            store_failure(&StoreError::Corrupt("/x".into())).exit(),
            Exit::Protocol
        );
    }
}
