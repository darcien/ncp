//! `ncp trust`: the trust of the commands in the project file of the
//! current folder.

use crate::text::path_code;
use serde_json::json;

use super::{
    args::TrustArgs,
    outcome::Failure,
    project_file::{self, Change, ProjectFile},
    render,
    session::Context,
};
use crate::project::{self, Project};

/// `ncp trust` asks no question and needs no TTY, because the user typed
/// the command, so the user means it.
///
/// # Errors
///
/// Returns a usage failure with `--config-file`, with no project file, or
/// with a file that `ncp` does not use. Returns a failure when `ncp`
/// cannot write a record.
pub fn command(context: &Context<'_>, args: &TrustArgs) -> Result<(), Failure> {
    let project = match project_file::find(&context.paths, context.env, !context.named_config) {
        ProjectFile::Read(project) => project,
        ProjectFile::NotSearched => {
            return Err(Failure::usage(
                "`--config-file` names the file, so ncp has no project file to trust",
            ));
        }
        ProjectFile::Absent(folder) => {
            return Err(Failure::usage(format!(
                "ncp found no `{}` from the current folder up to {}",
                project::FILE_NAME,
                path_code(&folder)
            )));
        }
        ProjectFile::Unusable { file, reasons } => {
            return Err(project_file::unusable(&file, &reasons, None));
        }
    };
    let change = match (args.show, args.untrust, args.ignore) {
        (true, _, _) => None,
        (_, true, _) => Some(Change::Untrust),
        (_, _, true) => Some(Change::Ignore),
        _ => Some(Change::Trust),
    };
    let trust = match change {
        Some(change) => project_file::change(&context.paths, &project, change)?,
        None => project.trust,
    };
    let project = Project { trust, ..project };
    context.ui.result(
        || render::project_file(&project),
        || {
            let (servers, invalid) = render::rows_json(&render::project_rows(&project));
            json!({
                "file": project.file,
                "trust": trust.word(),
                "servers": servers,
                "invalid": invalid,
            })
        },
    )
}
