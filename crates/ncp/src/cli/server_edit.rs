//! `server add` and `server remove`: the shell around `add_entry` and
//! `config_edit`.

use std::{
    fs::{self, DirBuilder},
    io::ErrorKind,
    os::unix::fs::{DirBuilderExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    time::Duration,
};

use nix::unistd::AccessFlags;
use serde_json::{Value, json};

use super::{
    args::{AddArgs, CredentialPlace},
    ask,
    auth::{self, TokenRemoval},
    outcome::{Failure, Hint, Warning},
    project_file::{self, Change, ProjectFile},
    render,
    secrets::{self, SecretRow},
    session::{self, Context, Loaded},
};
use crate::{
    add_entry::{self, AddError, Entry, Form, Request},
    config::{self, Server, Transport},
    config_edit::{self, EditError},
    private_file,
    project::{self, Project, Trust},
    server::ServerName,
    store::StoreError,
    text::{code, path_code},
};

/// The wait for the lock of the configuration files. Agents run commands
/// in parallel, so a change waits for the change of another process. The
/// limit is the same as the limit of the refresh lock.
const LOCK_LIMIT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// `named`: `--config-file` names the global file.
    Global {
        named: bool,
    },
    Project,
}

impl Scope {
    fn word(self) -> &'static str {
        match self {
            Self::Global { .. } => "global",
            Self::Project => "project",
        }
    }

    /// Only for the default global file, `ncp` makes the missing directories.
    fn makes_dirs(self) -> bool {
        self == Self::Global { named: false }
    }
}

/// The file that a command changes, with the bytes that the user checked:
/// the base of the edit of a project file.
struct Target {
    /// The path where `ncp` found the file, or the path that it names. `ncp`
    /// does not resolve a symlink, because the trust record keys this path.
    file: PathBuf,
    scope: Scope,
    /// `None` for a new file.
    base: Option<String>,
}

impl Target {
    fn base_text(&self) -> &str {
        self.base.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The file has the same entry already. This is not an error, so that a
    /// retry after a failure passes.
    Unchanged,
    Changed,
    Created,
}

/// A step that is left after `server add`.
enum Step {
    Command {
        command: String,
        when: Option<&'static str>,
    },
    /// A `${NAME}` that only the environment gives.
    SetEnv(String),
}

impl Step {
    fn json(&self) -> Value {
        match self {
            Self::Command {
                command,
                when: Some(when),
            } => json!({"command": command, "when": when}),
            Self::Command {
                command,
                when: None,
            } => json!({"command": command}),
            Self::SetEnv(name) => json!({"setEnv": name}),
        }
    }

    fn line(&self) -> String {
        match self {
            Self::Command {
                command,
                when: Some(when),
            } => format!("  {command}   (if {when})\n"),
            Self::Command {
                command,
                when: None,
            } => format!("  {command}\n"),
            Self::SetEnv(name) => format!("  export {name}=<value>\n"),
        }
    }
}

/// `ncp server add`. The usage checks come first, so that a piped value is
/// not read and lost. Each question comes before the lock, so that no
/// person waits inside it.
///
/// # Errors
///
/// Returns a usage failure for the rules of the entry and of the target
/// file, and the failures of the secret and of the write.
pub async fn add(context: &Context<'_>, args: AddArgs) -> Result<(), Failure> {
    if args.global && context.named_config {
        return Err(two_files());
    }
    let name = args.server.as_str();
    let request = request(&args)?;
    let entry = add_entry::build(name, &request).map_err(|error| add_failure(name, &error))?;
    let loaded = session::load(context, None).await?;
    let target = add_target(context, &loaded, args.global).await?;
    // Another entry of the name stops the add before `ncp` reads a value.
    if config_edit::entry(target.base_text(), name)
        .map_err(|error| edit_failure(&target, &error))?
        .is_some_and(|old| old != entry.value())
    {
        return Err(edit_failure(&target, &EditError::Exists(name.to_string())));
    }
    if let Some(secret) = &request.secret {
        let place = args.credential_store.unwrap_or(CredentialPlace::Native);
        secrets::store_for_add(context, secret, &entry.server, place).await?;
    }
    let outcome = change(context, &target, |text| {
        config_edit::add(text, name, &entry.fields)
    })
    .await?;
    report_added(context, &target, name, &entry, outcome).await
}

// The form of the entry: one word after the name is a URL, and the words
// after `--` are a command.
fn request(args: &AddArgs) -> Result<Request, Failure> {
    let form = match (&args.url, args.command.split_first()) {
        (Some(url), None) => Form::Remote { url: url.clone() },
        (None, Some((command, rest))) => Form::Stdio {
            command: command.clone(),
            args: rest.to_vec(),
        },
        _ => {
            return Err(Failure::usage(
                "`ncp server add` needs a URL or a command after `--`, but not both",
            )
            .hint(Hint::run_text(
                "add an HTTP server",
                "ncp server add <SERVER> <URL>",
            ))
            .hint(Hint::run_text(
                "add a stdio server",
                "ncp server add <SERVER> -- <COMMAND> [ARG...]",
            )));
        }
    };
    Ok(Request {
        form,
        headers: args.headers.clone(),
        env: args.env.clone(),
        // The error does not show the word after --secret: a user can type
        // the value there by mistake.
        secret: args
            .secret
            .as_deref()
            .map(|word| {
                crate::credential::SecretName::parse(word).map_err(|_| {
                    Failure::usage("The word after `--secret` is not a valid secret name")
                        .hint(super::fixes::secret_name_hint())
                })
            })
            .transpose()?,
    })
}

fn add_failure(name: &str, error: &AddError) -> Failure {
    let failure = Failure::usage(format!("ncp cannot add the server {}", code(name)))
        .about(name)
        .caused_by(error);
    match error {
        AddError::CredentialHeader(header) => failure.hint(format!(
            "Give the header as {}",
            code(&format!(
                "--header '{header}: Bearer ${{NAME}}' --secret NAME"
            ))
        )),
        AddError::Entry(error) => super::fixes::with_entry_hints(failure, error),
        _ => failure,
    }
}

fn two_files() -> Failure {
    Failure::usage("`--global` and `--config-file` name two files")
        .hint("Use `--global` or `--config-file`, but not both")
}

// The command must change the same file, so a named file keeps its
// `--config-file`.
fn remove_hint(name: &str, target: &Target) -> Hint {
    let word = render::shell_word(name);
    let command = match target.scope {
        Scope::Global { named: true } => format!(
            "ncp --config-file {} server remove {word}",
            render::shell_word(&target.file.display().to_string())
        ),
        Scope::Global { named: false } => format!("ncp server remove {word} --global"),
        Scope::Project => format!("ncp server remove {word}"),
    };
    Hint::run_text("remove the entry", &command)
}

fn global_hint() -> Hint {
    Hint::text("To change the global file, run the command again with `--global`")
}

// `server add` writes to the project file. It writes to the global file
// with `--global`, with `--config-file`, or in the home directory. With no
// project file, it makes `.mcp.json` in the current directory, because the
// project is the place that a team shares.
async fn add_target(
    context: &Context<'_>,
    loaded: &Loaded,
    global: bool,
) -> Result<Target, Failure> {
    if global {
        return global_target(context);
    }
    match &loaded.project {
        ProjectFile::NotSearched => global_target(context),
        ProjectFile::Read(project) => {
            trusted_for_write(context, project).await?;
            Ok(project_target(project))
        }
        ProjectFile::Unusable { file, reasons } => {
            Err(
                project_file::unusable(file, reasons, Some("ncp does not change it."))
                    .hint(global_hint()),
            )
        }
        ProjectFile::Absent(_) => {
            let dir = std::env::current_dir()
                .and_then(fs::canonicalize)
                .map_err(|error| {
                    Failure::protocol("ncp cannot find the current directory").caused_by(&error)
                })?;
            let home = context
                .env
                .get("HOME")
                .and_then(|home| fs::canonicalize(home).ok());
            if home.as_deref() == Some(dir.as_path()) {
                return global_target(context);
            }
            Ok(Target {
                file: dir.join(project::FILE_NAME),
                scope: Scope::Project,
                base: None,
            })
        }
    }
}

fn global_target(context: &Context<'_>) -> Result<Target, Failure> {
    let file = context.paths.config_file.clone();
    Ok(Target {
        base: read(&file)?,
        scope: Scope::Global {
            named: context.named_config,
        },
        file,
    })
}

fn project_target(project: &Project) -> Target {
    Target {
        file: project.file.clone(),
        scope: Scope::Project,
        base: Some(project.text.clone()),
    }
}

fn read(file: &Path) -> Result<Option<String>, Failure> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(Failure::protocol(format!("ncp cannot read {}", path_code(file))).caused_by(&error))
        }
    }
}

// `ncp` writes only a trusted project file, because a change to a file that
// the user did not check can add a command that `ncp` then starts. With a
// TTY, an untrusted file gets the question: a Yes trusts it, and a No writes
// the ignored record and stops.
async fn trusted_for_write(context: &Context<'_>, project: &Project) -> Result<(), Failure> {
    let file = path_code(&project.file);
    match project.trust {
        Trust::Trusted => Ok(()),
        Trust::Ignored => Err(Failure::usage(format!(
            "You ignored {file}, so ncp does not change it"
        ))
        .hint(global_hint())),
        Trust::Untrusted { .. } if context.interactive => {
            context
                .ui
                .diagnostic(&format!("{}\n", render::project_question(project)));
            match ask::confirm(context, "Trust this file? [y/N] ").await? {
                Some(true) => {
                    project_file::change(&context.paths, project, Change::Trust).map(|_| ())
                }
                Some(false) => {
                    project_file::change(&context.paths, project, Change::Ignore)?;
                    Err(Failure::usage(format!(
                        "You did not trust {file}, so ncp did not change it"
                    ))
                    .hint(global_hint()))
                }
                None => Err(untrusted(project)),
            }
        }
        Trust::Untrusted { .. } => Err(untrusted(project)),
    }
}

fn untrusted(project: &Project) -> Failure {
    let file = path_code(&project.file);
    let state = match project.trust {
        Trust::Untrusted { changed: true } => format!("{file} changed after you trusted it"),
        // `trusted_for_write` gives no trusted or ignored file.
        Trust::Untrusted { changed: false } | Trust::Trusted | Trust::Ignored => {
            format!("you did not trust {file}")
        }
    };
    render::project_entries(&project.entries)
        .into_iter()
        .fold(
            Failure::usage(format!(
                "ncp changes only a trusted project file, and {state}:"
            )),
            Failure::data,
        )
        .hint(Hint::run("see the entries of the file", "ncp trust --show"))
}

// The steps under the lock:
// - read the base
// - make the edit
// - replace the file atomically, after a last check
// - write the trust record of a project file
//
// Each question comes before the lock, so no person waits inside it.
async fn change(
    context: &Context<'_>,
    target: &Target,
    edit: impl FnOnce(&str) -> Result<Option<String>, EditError>,
) -> Result<Outcome, Failure> {
    let _lock = lock(context).await?;
    let real = real_path(&target.file)?;
    // The base of a project file is the bytes that the user trusted. The
    // global file needs no trust, so its base is the bytes under the lock.
    // Thus two adds of agents at the same time both pass.
    let base = match target.scope {
        Scope::Project => {
            unchanged(target, &real, target.base.as_ref())?;
            target.base.clone()
        }
        Scope::Global { .. } => read(&real)?,
    };
    let Some(text) =
        edit(base.as_deref().unwrap_or("")).map_err(|error| edit_failure(target, &error))?
    else {
        return Ok(Outcome::Unchanged);
    };
    let mode = mode_for_write(target, &real)?;
    if target.scope.makes_dirs() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(real.parent().unwrap_or(Path::new(".")))
            .map_err(|error| cannot_write(target, &error))?;
    }
    private_file::replace(&real, text.as_bytes(), mode, || {
        (context.before_rename)(&real);
        unchanged(target, &real, base.as_ref()).map_err(Stop::Failure)
    })
    .map_err(|stop| match stop {
        Stop::Io(error) => cannot_write(target, &error),
        Stop::Failure(failure) => failure,
    })?;
    trust_written(context, target, &text);
    Ok(if base.is_none() {
        Outcome::Created
    } else {
        Outcome::Changed
    })
}

enum Stop {
    Io(std::io::Error),
    Failure(Failure),
}

impl From<std::io::Error> for Stop {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

// The wait runs on its own thread, so that Ctrl-C stops it.
async fn lock(context: &Context<'_>) -> Result<private_file::Lock, Failure> {
    let path = context.paths.config_lock.clone();
    let shown = path.display().to_string();
    let wait = crate::store::run_blocking(LOCK_LIMIT + Duration::from_secs(1), move || {
        Ok(private_file::lock(&path, LOCK_LIMIT))
    });
    let failed = |error: &dyn std::error::Error| {
        Failure::protocol(format!("ncp cannot lock {}", code(&shown))).caused_by(error)
    };
    match secrets::interruptible(context, wait).await {
        Ok(Ok(lock)) => Ok(lock),
        Err(StoreError::Interrupted) => Err(session::interrupted(None)),
        Ok(Err(error)) => Err(failed(&error)),
        Err(error) => Err(failed(&error)),
    }
}

// The file that the new bytes go to. A symlink stays: its target gets them. A
// symlink to a missing file is an error, because a rename puts a file in the
// place of the link.
fn real_path(file: &Path) -> Result<PathBuf, Failure> {
    match fs::canonicalize(file) {
        Ok(real) => Ok(real),
        Err(_) if file.symlink_metadata().is_ok() => Err(Failure::protocol(format!(
            "The file {} is a symlink to a file that does not exist",
            path_code(file)
        ))),
        Err(_) => Ok(file.to_path_buf()),
    }
}

// The mode of the old file, or `0600` for a new one. A rename needs only
// the access to the directory, so a read-only file gets its own check.
fn mode_for_write(target: &Target, real: &Path) -> Result<u32, Failure> {
    match fs::metadata(real) {
        Ok(metadata) => {
            nix::unistd::access(real, AccessFlags::W_OK)
                .map_err(|error| cannot_write(target, &error))?;
            Ok(metadata.permissions().mode() & 0o7777)
        }
        Err(_) => Ok(0o600),
    }
}

fn cannot_write(target: &Target, error: &dyn std::error::Error) -> Failure {
    Failure::protocol(format!("ncp cannot write {}", path_code(&target.file))).caused_by(error)
}

// The file has the bytes that the user checked. The lock cannot stop an
// editor or another tool.
fn unchanged(target: &Target, real: &Path, base: Option<&String>) -> Result<(), Failure> {
    if read(real)?.as_ref() == base {
        Ok(())
    } else {
        Err(Failure::protocol(format!(
            "The file {} changed after ncp read it, so ncp did not change it",
            path_code(&target.file)
        ))
        .hint(format!(
            "Another process changed {}. To apply your change to the new file, run the command again.",
            path_code(&target.file)
        )))
    }
}

fn edit_failure(target: &Target, error: &EditError) -> Failure {
    let file = path_code(&target.file);
    match error {
        EditError::SelfCheck => Failure::protocol(format!(
            "The edit of {file} failed its check. ncp did not change the file."
        ))
        .caused_by(error),
        EditError::Exists(name) => Failure::usage(format!("ncp cannot change {file}"))
            .caused_by(error)
            .hint(remove_hint(name, target)),
        EditError::Duplicate(key) => Failure::usage(format!("ncp cannot change {file}"))
            .caused_by(error)
            .hint(format!(
                "Remove the copies of the key {} from the file. Then run the command again.",
                code(key)
            )),
        EditError::Config(_) | EditError::NotFound(_) => {
            Failure::usage(format!("ncp cannot change {file}")).caused_by(error)
        }
    }
}

// After a write, `ncp` trusts the new bytes of a project file: the user saw
// the old bytes and gave the change. A record that fails gives a warning,
// because the change is complete.
fn trust_written(context: &Context<'_>, target: &Target, text: &str) {
    if target.scope == Scope::Project
        && let Err(failure) = project_file::trust_written(&context.paths, &target.file, text)
    {
        context
            .ui
            .warn(&Warning::from(failure).hint(Hint::run_text("trust the file", "ncp trust")));
    }
}

async fn report_added(
    context: &Context<'_>,
    target: &Target,
    name: &str,
    entry: &Entry,
    outcome: Outcome,
) -> Result<(), Failure> {
    // The view after the write: the rows name each entry that uses a
    // value, also the entries of a project file that the add trusted.
    let view = session::load_config(&context.paths, context.env, !context.named_config).ok();
    warn_about_other_file(context, view.as_ref(), target, name);
    let servers: Vec<&Server> = view
        .iter()
        .flat_map(|view| view.config.servers())
        .filter(|other| other.name != entry.server.name)
        .chain(std::iter::once(&entry.server))
        .collect();
    let rows = secrets::rows_of_entry(context, &entry.server, &servers).await;
    let steps = next_steps(context, name, entry, &rows);
    let file = target.file.display();
    context.ui.result(
        || {
            let head = match outcome {
                Outcome::Unchanged => format!("{name} is already in {file}.\n"),
                Outcome::Created => format!("Created {file}.\nAdded {name} to {file}.\n"),
                Outcome::Changed => format!("Added {name} to {file}.\n"),
            };
            let values: String = rows.iter().map(|row| value_line(name, row)).collect();
            let steps: String = steps.iter().map(Step::line).collect();
            head + &values + "Next:\n" + &steps
        },
        || {
            json!({
                "server": name,
                "transport": entry.shape.transport,
                "target": render::target_text(&entry.shape.target),
                "scope": target.scope.word(),
                "file": target.file,
                "created": outcome == Outcome::Created,
                "changed": outcome != Outcome::Unchanged,
                "secrets": rows.iter().map(SecretRow::json).collect::<Vec<_>>(),
                "nextSteps": steps.iter().map(Step::json).collect::<Vec<_>>(),
            })
        },
    )
}

// One line for each stored value: the stores, and the other servers that
// use it, so that a reused value shows.
fn value_line(name: &str, row: &SecretRow) -> String {
    let stored = if row.places.is_empty() {
        "not stored".to_string()
    } else {
        format!("in {}", row.places_text())
    };
    let others: Vec<&str> = row
        .servers
        .iter()
        .map(String::as_str)
        .filter(|other| *other != name)
        .collect();
    let used = if others.is_empty() {
        "used by no other server".to_string()
    } else {
        format!("also used by {}", others.join(", "))
    };
    format!("{}: {stored}, {used}.\n", row.name)
}

// The steps that are left:
// - a value that no store and no variable has
// - a variable of the command or the URL
// - the login of an http server with no credential header
// - a check of the connection
fn next_steps(context: &Context<'_>, name: &str, entry: &Entry, rows: &[SecretRow]) -> Vec<Step> {
    let word = render::shell_word(name);
    let unset = |variable: &str| !context.env.contains_key(variable);
    let values = rows
        .iter()
        .filter(|row| row.places.is_empty() && unset(row.name.as_str()))
        .map(|row| Step::Command {
            command: format!("ncp secret set {} --server {word}", row.name),
            when: None,
        });
    let environment = add_entry::environment_names(&entry.shape)
        .into_iter()
        .filter(|variable| unset(variable))
        .map(Step::SetEnv);
    let login = (matches!(entry.server.transport, Transport::Http(_))
        && !entry.has_credential_header())
    .then(|| Step::Command {
        command: format!("ncp auth login {word}"),
        when: Some("the server needs a login"),
    });
    let check = Step::Command {
        command: format!("ncp server show {word}"),
        when: None,
    };
    values
        .chain(environment)
        .chain(login)
        .chain(std::iter::once(check))
        .collect()
}

// A warning when the other file has the name: it says which entry takes
// effect in this directory.
fn warn_about_other_file(
    context: &Context<'_>,
    view: Option<&Loaded>,
    target: &Target,
    name: &str,
) {
    let global = &context.paths.config_file;
    let message = match target.scope {
        Scope::Project => read(global)
            .ok()
            .flatten()
            .and_then(|text| config_edit::entry(&text, name).ok().flatten())
            .map(|_| {
                format!(
                    "The global file {} also has an entry of the server {}. The entry in {} takes effect in this directory.",
                    path_code(global),
                    code(name),
                    path_code(&target.file)
                )
            }),
        Scope::Global { .. } => view.and_then(|view| view.project_of(name)).map(|project| {
            format!(
                "The project file {} also has an entry of the server {}. Its entry takes effect in this directory.",
                path_code(&project.file),
                code(name)
            )
        }),
    };
    if let Some(message) = message {
        context.ui.warn(&Warning::new(message).about(name));
    }
}

/// Where `server remove` removes the entry. The trust state of the project
/// file and the files that have the name decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Global,
    Project,
    /// Only an ignored project file has the name.
    Ignored,
    /// `--global`, and only the project file has the name.
    OnlyInProject,
    Missing,
}

fn remove_place(project: Option<Trust>, in_global: bool, global_only: bool) -> Place {
    match (project, in_global, global_only) {
        (_, true, true) | (None | Some(Trust::Ignored), true, false) => Place::Global,
        (Some(_), false, true) => Place::OnlyInProject,
        (None, false, _) => Place::Missing,
        (Some(Trust::Ignored), false, false) => Place::Ignored,
        (Some(_), _, false) => Place::Project,
    }
}

/// `ncp server remove`: the entry that runs for the name, or the global
/// entry with `--global`.
///
/// # Errors
///
/// Returns a usage failure for an unknown name and for the trust state of
/// the project file, and the failures of the write.
pub async fn remove(context: &Context<'_>, name: &str, global: bool) -> Result<(), Failure> {
    if global && context.named_config {
        return Err(two_files());
    }
    let loaded = session::load(context, None).await?;
    let global_file = global_target(context)?;
    let project = loaded.project.project().filter(|project| project.has(name));
    // The rule of the loader: the last copy of a key. A duplicate key
    // stops only an edit of the global file.
    // A missing global file has no servers.
    let in_global = global_file
        .base
        .as_deref()
        .map(config::raw_servers)
        .transpose()
        .map_err(|error| edit_failure(&global_file, &EditError::Config(error)))?
        .is_some_and(|servers| servers.contains_key(name));
    let place = remove_place(
        project.map(|project| project.trust),
        in_global,
        global || context.named_config,
    );
    let target = match (place, project) {
        (Place::Global, _) => global_file,
        (Place::Project, Some(project)) => {
            trusted_for_write(context, project).await?;
            project_target(project)
        }
        (Place::Ignored, Some(project)) => {
            return Err(Failure::usage(format!(
                "The server {} is in {}, which you ignored",
                code(name),
                path_code(&project.file)
            ))
            .about(name)
            .hint(Hint::run_text("trust the file", "ncp trust")));
        }
        (Place::OnlyInProject, Some(project)) => {
            let hint = match project.trust {
                Trust::Ignored => Hint::text(format!(
                    "To trust the file, run `ncp trust`. Then run `ncp server remove {}`.",
                    render::shell_word(name)
                )),
                Trust::Trusted | Trust::Untrusted { .. } => Hint::run_text(
                    "remove the entry of the project file",
                    &format!("ncp server remove {}", render::shell_word(name)),
                ),
            };
            return Err(Failure::usage(format!(
                "The global file has no entry of the server {}, but {} has one",
                code(name),
                path_code(&project.file)
            ))
            .about(name)
            .hint(hint));
        }
        _ => return Err(not_found(context, &loaded, name)),
    };
    let entry = config_edit::entry(target.base_text(), name)
        .map_err(|error| edit_failure(&target, &error))?;
    change(context, &target, |text| {
        config_edit::remove(text, name).map(Some)
    })
    .await?;
    let still = match target.scope {
        Scope::Project => in_global.then(|| {
            format!(
                "the entry of {name} in {} now takes effect",
                context.paths.config_file.display()
            )
        }),
        Scope::Global { .. } => loaded.project_of(name).map(|project| {
            format!(
                "the entry of {name} in {} still takes effect in this directory",
                project.file.display()
            )
        }),
    };
    let tokens = match (&still, &entry) {
        (None, Some(entry)) => remove_tokens(context, name, entry).await,
        _ => TokenRemoval::default(),
    };
    // The file has no entry now, so the result shows also after Ctrl-C.
    report_removed(context, &target, name, still.as_deref(), &tokens)?;
    tokens.interrupted.map_or(Ok(()), Err)
}

fn report_removed(
    context: &Context<'_>,
    target: &Target,
    name: &str,
    still: Option<&str>,
    tokens: &TokenRemoval,
) -> Result<(), Failure> {
    let places: Vec<&str> = tokens
        .removed
        .iter()
        .map(|place| secrets::place_name(*place))
        .collect();
    context.ui.result(
        || {
            let removed: Vec<&str> = tokens
                .removed
                .iter()
                .map(|place| secrets::place_text(*place))
                .collect();
            [
                Some(format!("Removed {name} from {}.", target.file.display())),
                still.map(|still| format!("OAuth tokens of {name}, if any, stay: {still}.")),
                (!removed.is_empty()).then(|| format!(
                    "Deleted the OAuth tokens of {name} from {}. A project in another directory with the same entry needs a new login.",
                    removed.join(" and ")
                )),
                tokens.kept.then(|| format!(
                    "The OAuth tokens of {name} stay: they are for another URL or issuer, or ncp cannot compare them."
                )),
                tokens.interrupted.is_some().then(|| format!(
                    "OAuth tokens of {name}, if any, stay: ncp stopped before it checked each credential store."
                )),
            ]
            .into_iter()
            .flatten()
            .map(|line| line + "\n")
            .collect::<String>()
        },
        || {
            json!({
                "server": name,
                "scope": target.scope.word(),
                "file": target.file,
                "credentialStores": places,
                "tokensStay": still.is_some() || tokens.kept || tokens.interrupted.is_some(),
            })
        },
    )
}

// The failure of a name that no file has, with the nearest name.
fn not_found(context: &Context<'_>, loaded: &Loaded, name: &str) -> Failure {
    session::select(loaded, name, context.ui)
        .err()
        .unwrap_or_else(|| {
            Failure::usage(format!(
                "No server {} is configured",
                crate::text::code(name)
            ))
        })
}

// An entry that does not load has no URL to compare, so its tokens stay, and the result says so.
async fn remove_tokens(context: &Context<'_>, name: &str, entry: &Value) -> TokenRemoval {
    match config::parse_one(name, entry, context.env) {
        Ok(server) => match &server.transport {
            Transport::Http(remote) => {
                let issuer = remote
                    .oauth_client
                    .as_ref()
                    .and_then(|client| client.issuer());
                let resource = remote.url.without_query();
                auth::remove_bound_items(context, &server.name, Some(&resource), issuer).await
            }
            _ => TokenRemoval::default(),
        },
        Err(_) => match ServerName::parse(name) {
            Ok(server) => auth::remove_bound_items(context, &server, None, None).await,
            Err(_) => TokenRemoval::default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_picks_the_file_from_the_trust_state_and_the_files_with_the_name() {
        let untrusted = Some(Trust::Untrusted { changed: false });
        let cases = [
            (Some(Trust::Trusted), false, false, Place::Project),
            (Some(Trust::Trusted), true, false, Place::Project),
            (untrusted, false, false, Place::Project),
            (untrusted, true, false, Place::Project),
            (Some(Trust::Ignored), false, false, Place::Ignored),
            (Some(Trust::Ignored), true, false, Place::Global),
            (None, true, false, Place::Global),
            (None, false, false, Place::Missing),
            (Some(Trust::Trusted), true, true, Place::Global),
            (Some(Trust::Trusted), false, true, Place::OnlyInProject),
            (None, false, true, Place::Missing),
        ];
        for (project, in_global, global_only, place) in cases {
            assert_eq!(
                remove_place(project, in_global, global_only),
                place,
                "{project:?} {in_global} {global_only}"
            );
        }
    }
}
