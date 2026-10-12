//! `server add` and `server remove`: the shell around `add_entry` and
//! `config_edit`. The rules are in docs/design/server-add.md, and the
//! target file and its trust in docs/design/project-config.md rule 9.

use std::{
    fs::{self, DirBuilder, OpenOptions},
    io::{ErrorKind, Write as _},
    os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    time::Duration,
};

use nix::unistd::AccessFlags;
use serde_json::{Value, json};

use super::{
    args::{AddArgs, CredentialPlace},
    ask, auth,
    outcome::Failure,
    project_file::{self, Change, ProjectFile},
    render, secrets,
    session::{self, Context, Loaded},
};
use crate::{
    add_entry::{self, AddError, Entry, Form, Request},
    config::{self, Server, Transport},
    config_edit::{self, EditError},
    private_file,
    project::{self, Project, Trust},
    server::ServerName,
};

/// The wait for the lock of the configuration files, as for the refresh
/// lock (B55).
const LOCK_LIMIT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Global,
    Project,
}

impl Scope {
    fn word(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

/// The file that a command changes, with the bytes that the user checked:
/// the base of the edit. A change of the file after this read stops the
/// write, because nobody checked the new bytes.
struct Target {
    /// The path where `ncp` found the file, or the path that it names. A
    /// symlink is not resolved: the trust record keys this path.
    file: PathBuf,
    scope: Scope,
    /// `None` for a new file.
    base: Option<String>,
    /// Only for the default global file, `ncp` makes the missing folders.
    makes_folders: bool,
}

impl Target {
    fn base_text(&self) -> &str {
        self.base.as_deref().unwrap_or("")
    }
}

/// `ncp server add`.
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
    let secret = args
        .secret
        .as_deref()
        .map(secrets::secret_name)
        .transpose()?;
    let loaded = session::load(context, None).await?;
    let target = add_target(context, &loaded, args.global).await?;
    match config_edit::entry(target.base_text(), name)
        .map_err(|error| edit_failure(&target, &error))?
    {
        Some(old) if old == entry.value() => {
            return added(context, &loaded, &target, name, &entry, false).await;
        }
        Some(_) => {
            return Err(Failure::usage(format!(
                "{} already has a server {name}",
                target.file.display()
            ))
            .hint(remove_command(name, target.scope)));
        }
        None => {}
    }
    if let Some(secret) = &secret {
        let server = config::unexpanded_server(name, &entry.value())
            .map_err(|error| add_failure(name, &AddError::Entry(error)))?;
        let place = args.credential_store.unwrap_or(CredentialPlace::Native);
        secrets::store_for_add(context, secret, &server, place).await?;
    }
    let text = write(context, &target, |text| {
        config_edit::add(text, name, &entry.fields)
    })?;
    trust_written(context, &target, &text);
    added(context, &loaded, &target, name, &entry, true).await
}

/// The form of the entry: one word after the name is a URL, and the words
/// after `--` are a command.
fn request(args: &AddArgs) -> Result<Request, Failure> {
    let form = match (&args.url, args.command.split_first()) {
        (Some(url), None) => Form::Remote { url: url.clone() },
        (None, Some((command, rest))) => Form::Stdio {
            command: command.clone(),
            args: rest.to_vec(),
        },
        _ => {
            return Err(Failure::usage(
                "give a URL or a command after --, not both and not neither",
            )
            .hint("ncp server add <SERVER> <URL>")
            .hint("ncp server add <SERVER> -- <COMMAND> [ARG...]"));
        }
    };
    Ok(Request {
        form,
        headers: args.headers.clone(),
        env: args.env.clone(),
        secret: args.secret.clone(),
    })
}

fn add_failure(name: &str, error: &AddError) -> Failure {
    let failure = Failure::usage(error.to_string()).server(name);
    match error {
        AddError::CredentialHeader(header) => failure.hint(format!(
            "--header '{header}: Bearer ${{NAME}}' --secret NAME"
        )),
        _ => failure,
    }
}

fn two_files() -> Failure {
    Failure::usage("--global and --config-file name two files; give one of them")
}

fn remove_command(name: &str, scope: Scope) -> String {
    let word = render::shell_word(name);
    match scope {
        Scope::Global => format!("ncp server remove {word} --global"),
        Scope::Project => format!("ncp server remove {word}"),
    }
}

/// The file of `server add`: the project file; the global file with
/// `--global`, with `--config-file`, or in the home folder; a new
/// `.mcp.json` in the current folder when there is no project file, as
/// `mise use` (project-config rule 9).
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
        ProjectFile::Unusable { file, reason } => Err(Failure::usage(format!(
            "ncp does not use {}: {reason}; ncp does not change it",
            file.display()
        ))
        .hint("--global")),
        ProjectFile::Absent(_) => {
            let folder = std::env::current_dir()
                .and_then(fs::canonicalize)
                .map_err(|error| {
                    Failure::protocol(format!("cannot find the current folder: {error}"))
                })?;
            let home = context
                .env
                .get("HOME")
                .and_then(|home| fs::canonicalize(home).ok());
            if home.as_deref() == Some(folder.as_path()) {
                return global_target(context);
            }
            Ok(Target {
                file: folder.join(project::FILE_NAME),
                scope: Scope::Project,
                base: None,
                makes_folders: false,
            })
        }
    }
}

fn global_target(context: &Context<'_>) -> Result<Target, Failure> {
    let file = context.paths.config_file.clone();
    Ok(Target {
        base: read(&file)?,
        scope: Scope::Global,
        makes_folders: !context.named_config,
        file,
    })
}

fn project_target(project: &Project) -> Target {
    Target {
        file: project.file.clone(),
        scope: Scope::Project,
        base: Some(project.text.clone()),
        makes_folders: false,
    }
}

fn read(file: &Path) -> Result<Option<String>, Failure> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Failure::protocol(format!(
            "cannot read {}: {error}",
            file.display()
        ))),
    }
}

/// `ncp` writes only a trusted project file, as `mise use` checks the
/// trust before it writes (project-config rule 9). With a TTY, an
/// untrusted file gets the question: a Yes trusts it, and a No writes the
/// ignored record and stops.
async fn trusted_for_write(context: &Context<'_>, project: &Project) -> Result<(), Failure> {
    let file = project.file.display();
    match project.trust {
        Trust::Trusted => Ok(()),
        Trust::Ignored => Err(Failure::usage(format!(
            "you ignored {file}, so ncp does not change it"
        ))
        .hint("--global")),
        Trust::Untrusted { .. } if context.interactive => {
            context
                .ui
                .diagnostic(&format!("{}\n", render::project_question(project)));
            match ask::confirm(context, "Trust? [y/N] ").await? {
                Some(true) => {
                    project_file::change(&context.paths, project, Change::Trust).map(|_| ())
                }
                Some(false) => {
                    project_file::change(&context.paths, project, Change::Ignore)?;
                    Err(Failure::usage(format!(
                        "you did not trust {file}, so ncp did not change it"
                    ))
                    .hint("--global"))
                }
                None => Err(untrusted(project)),
            }
        }
        Trust::Untrusted { .. } => Err(untrusted(project)),
    }
}

fn untrusted(project: &Project) -> Failure {
    let state = match project.trust {
        Trust::Untrusted { changed: true } => "changed since you trusted it",
        _ => "is not trusted",
    };
    Failure::usage(format!(
        "{} {state}, and ncp changes only a trusted project file:\n{}",
        project.file.display(),
        render::project_entries(&project.entries).trim_end()
    ))
    .hint("ncp trust --show")
}

/// Changes the file under the lock: a new read that must give the base,
/// the edit, a temporary file, a last read, and an atomic rename. Each
/// question was before the lock, so no person waits inside it.
fn write(
    context: &Context<'_>,
    target: &Target,
    edit: impl FnOnce(&str) -> Result<String, EditError>,
) -> Result<String, Failure> {
    let lock = &context.paths.config_lock;
    let _lock = private_file::lock(lock, LOCK_LIMIT)
        .map_err(|error| Failure::protocol(format!("cannot lock {}: {error}", lock.display())))?;
    // A symlink stays: the new bytes go to its target.
    let real = fs::canonicalize(&target.file).unwrap_or_else(|_| target.file.clone());
    // The base of a project file is the bytes that the user trusted. The
    // global file needs no trust, so its base is the bytes under the lock,
    // and two adds of agents at the same time both pass.
    let base = match target.scope {
        Scope::Project => {
            unchanged(target, &real, target.base.as_ref())?;
            target.base.clone()
        }
        Scope::Global => read(&real)?,
    };
    let text = edit(base.as_deref().unwrap_or("")).map_err(|error| edit_failure(target, &error))?;
    let cannot = |error: &dyn std::fmt::Display| {
        Failure::protocol(format!("cannot write {}: {error}", target.file.display()))
    };
    // A rename needs only the access to the folder, so a read-only file
    // gets its own check.
    let mode = match fs::metadata(&real) {
        Ok(metadata) => {
            nix::unistd::access(&real, AccessFlags::W_OK).map_err(|error| cannot(&error))?;
            metadata.permissions().mode() & 0o7777
        }
        Err(_) => 0o600,
    };
    let folder = real.parent().unwrap_or(Path::new("."));
    if target.makes_folders {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .map_err(|error| cannot(&error))?;
    }
    let file_name = real
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = folder.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            // The mode of the old file, also with a umask.
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.sync_all()
        })
        .map_err(|error| cannot(&error));
    let renamed = written.and_then(|()| {
        (context.before_rename)(&real);
        unchanged(target, &real, base.as_ref())?;
        fs::rename(&temporary, &real).map_err(|error| cannot(&error))
    });
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed.map(|()| text)
}

/// The file has the bytes that the user checked. The lock cannot stop an
/// editor or another tool.
fn unchanged(target: &Target, real: &Path, base: Option<&String>) -> Result<(), Failure> {
    if read(real)?.as_ref() == base {
        Ok(())
    } else {
        Err(Failure::protocol(format!(
            "{} changed after ncp read it, so ncp did not change it",
            target.file.display()
        ))
        .hint("run the command again"))
    }
}

fn edit_failure(target: &Target, error: &EditError) -> Failure {
    let message = format!("{}: {error}", target.file.display());
    match error {
        EditError::SelfCheck => Failure::protocol(format!("{message}; ncp did not change it")),
        EditError::Exists(name) => Failure::usage(message).hint(remove_command(name, target.scope)),
        _ => Failure::usage(message),
    }
}

/// After a write, `ncp` trusts the new bytes of a project file: the user
/// saw the old bytes and gave the change (project-config rule 9). A
/// record that fails gives a warning, because the change is done.
fn trust_written(context: &Context<'_>, target: &Target, text: &str) {
    if target.scope == Scope::Project
        && let Err(failure) = project_file::trust_written(&context.paths, &target.file, text)
    {
        context.ui.warning(None, &failure.message);
        context.ui.hint(None, "ncp trust");
    }
}

/// The result of `server add`: the file, the stored values of the entry,
/// and the steps that are left.
async fn added(
    context: &Context<'_>,
    loaded: &Loaded,
    target: &Target,
    name: &str,
    entry: &Entry,
    changed: bool,
) -> Result<(), Failure> {
    warn_about_other_file(context, loaded, target, name)?;
    let server = config::unexpanded_server(name, &entry.value())
        .map_err(|error| add_failure(name, &AddError::Entry(error)))?;
    let servers: Vec<&Server> = loaded
        .config
        .servers()
        .filter(|other| other.name != server.name)
        .chain(std::iter::once(&server))
        .collect();
    let secrets = secrets::rows_of_entry(context, &server, &servers).await;
    let steps = next_steps(context, name, entry, &secrets);
    let file = target.file.display();
    let created = changed && target.base.is_none();
    context.ui.result(
        || {
            let head = match (changed, created) {
                (false, _) => format!("{name} is already in {file}.\n"),
                (true, true) => format!("Created {file}.\nAdded {name} to {file}.\n"),
                (true, false) => format!("Added {name} to {file}.\n"),
            };
            let values: String = secrets.iter().map(|row| secret_line(name, row)).collect();
            let steps: String = steps.iter().map(step_line).collect();
            let steps = if steps.is_empty() {
                steps
            } else {
                format!("Next:\n{steps}")
            };
            head + &values + &steps
        },
        || {
            json!({
                "server": name,
                "transport": entry.shape.transport,
                "target": render::target_text(&entry.shape.target),
                "scope": target.scope.word(),
                "file": target.file,
                "created": created,
                "changed": changed,
                "secrets": secrets,
                "nextSteps": steps,
            })
        },
    )
}

/// One line for each stored value: the stores, and the other servers that
/// use it, so that a reused value shows.
fn secret_line(name: &str, row: &Value) -> String {
    let text = |key: &str| -> Vec<String> {
        row[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let places: Vec<&str> = text("credentialStores")
        .iter()
        .map(|place| {
            if place == "native" {
                "the native store"
            } else {
                "the credential file"
            }
        })
        .collect();
    let others: Vec<String> = text("servers")
        .into_iter()
        .filter(|other| other != name)
        .collect();
    let stored = if places.is_empty() {
        "not stored".to_string()
    } else {
        format!("in {}", places.join(" and "))
    };
    let used = if others.is_empty() {
        "used by no other server".to_string()
    } else {
        format!("also used by {}", others.join(", "))
    };
    format!(
        "{}: {stored}, {used}.\n",
        row["name"].as_str().unwrap_or_default()
    )
}

fn step_line(step: &Value) -> String {
    match (
        step["command"].as_str(),
        step["setEnv"].as_str(),
        step["when"].as_str(),
    ) {
        (Some(command), _, Some(when)) => format!("  {command}   (if {when})\n"),
        (Some(command), _, None) => format!("  {command}\n"),
        (None, Some(name), _) => format!("  export {name}=<value>\n"),
        _ => String::new(),
    }
}

/// The steps that are left: a value that no store and no variable has, a
/// variable of the command or the URL, the login of an http server with
/// no credential header, and a check of the connection.
fn next_steps(context: &Context<'_>, name: &str, entry: &Entry, secrets: &[Value]) -> Vec<Value> {
    let word = render::shell_word(name);
    let unset = |variable: &str| !context.env.contains_key(variable);
    let values = secrets.iter().filter_map(|row| {
        let variable = row["name"].as_str()?;
        let stored = row["credentialStores"]
            .as_array()
            .is_some_and(|places| !places.is_empty());
        (!stored && unset(variable))
            .then(|| json!({"command": format!("ncp secret set {variable} --server {word}")}))
    });
    let environment = add_entry::environment_names(&entry.shape)
        .into_iter()
        .filter(|variable| unset(variable))
        .map(|variable| json!({"setEnv": variable}));
    let login = (entry.shape.transport == "http" && !add_entry::has_credential_header(entry)).then(
        || json!({"command": format!("ncp auth login {word}"), "when": "the server needs a login"}),
    );
    values
        .chain(environment)
        .chain(login)
        .chain(std::iter::once(
            json!({"command": format!("ncp server show {word}")}),
        ))
        .collect()
}

/// A warning when the other file has the name: it says which entry takes
/// effect in this folder (project-config rule 9).
fn warn_about_other_file(
    context: &Context<'_>,
    loaded: &Loaded,
    target: &Target,
    name: &str,
) -> Result<(), Failure> {
    let message = match target.scope {
        Scope::Project => {
            let global = global_target(context)?;
            config_edit::entry(global.base_text(), name)
                .ok()
                .flatten()
                .map(|_| {
                    format!(
                        "{} also has {name}; the entry in {} takes effect in this folder",
                        global.file.display(),
                        target.file.display()
                    )
                })
        }
        Scope::Global => loaded.project_of(name).map(|project| {
            format!(
                "{} also has {name}; its entry takes effect in this folder",
                project.file.display()
            )
        }),
    };
    if let Some(message) = message {
        context.ui.warning(Some(name), &message);
    }
    Ok(())
}

/// `ncp server remove`: the entry that runs for the name, or the global
/// entry with `--global` (docs/design/server-add.md section 6).
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
    let in_global = config_edit::entry(global_file.base_text(), name)
        .map_err(|error| edit_failure(&global_file, &error))?
        .is_some();
    let in_project = loaded
        .project
        .project()
        .filter(|project| !global && project.has(name));
    let target = match in_project {
        Some(project) if project.trust == Trust::Ignored && in_global => global_file,
        Some(project) if project.trust == Trust::Ignored => {
            return Err(Failure::usage(format!(
                "{name} is in {}, which you ignored",
                project.file.display()
            ))
            .hint("ncp trust"));
        }
        Some(project) => {
            trusted_for_write(context, project).await?;
            project_target(project)
        }
        None if in_global => global_file,
        None => return Err(not_found(context, &loaded, name)),
    };
    let entry = config_edit::entry(target.base_text(), name)
        .map_err(|error| edit_failure(&target, &error))?;
    let text = write(context, &target, |text| config_edit::remove(text, name))?;
    trust_written(context, &target, &text);
    let still = match target.scope {
        Scope::Project => in_global
            .then(|| global_target(context))
            .transpose()?
            .map(|file| {
                format!(
                    "the entry of {name} in {} now takes effect",
                    file.file.display()
                )
            }),
        Scope::Global => loaded.project_of(name).map(|project| {
            format!(
                "the entry of {name} in {} still takes effect in this folder",
                project.file.display()
            )
        }),
    };
    let places = match (&still, entry) {
        (None, Some(entry)) => remove_tokens(context, name, &entry).await,
        _ => Vec::new(),
    };
    let file = target.file.display();
    let place_names: Vec<&str> = places
        .iter()
        .map(|place| secrets::place_name(*place))
        .collect();
    context.ui.result(
        || {
            let tokens = match places.as_slice() {
                [] => String::new(),
                places => {
                    let names: Vec<&str> =
                        places.iter().map(|place| secrets::place_text(*place)).collect();
                    format!(
                        "Removed the OAuth tokens of {name} from {}. A project in another folder with the same entry needs a new login.\n",
                        names.join(" and ")
                    )
                }
            };
            let still = still
                .as_ref()
                .map(|text| format!("The tokens stay: {text}.\n"))
                .unwrap_or_default();
            format!("Removed {name} from {file}.\n{still}{tokens}")
        },
        || {
            json!({
                "server": name,
                "scope": target.scope.word(),
                "file": target.file,
                "credentialStores": place_names,
            })
        },
    )
}

/// The failure of a name that no file has, with the nearest name.
fn not_found(context: &Context<'_>, loaded: &Loaded, name: &str) -> Failure {
    session::select(loaded, name, context.ui)
        .err()
        .unwrap_or_else(|| Failure::usage(format!("no server {name:?} is configured")))
}

/// The OAuth items of the removed entry, when it is an http entry. An
/// invalid entry has no URL to compare, so its tokens stay.
async fn remove_tokens(context: &Context<'_>, name: &str, entry: &Value) -> Vec<CredentialPlace> {
    let Ok(server) = config::parse_one(name, entry, context.env) else {
        return Vec::new();
    };
    let Transport::Http(remote) = &server.transport else {
        return Vec::new();
    };
    let Ok(server_name) = ServerName::parse(name) else {
        return Vec::new();
    };
    let issuer = remote
        .oauth_client
        .as_ref()
        .and_then(|client| client.issuer());
    auth::remove_bound_items(context, &server_name, &remote.url.without_query(), issuer).await
}
