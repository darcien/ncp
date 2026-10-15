//! Text output for people on a TTY. Pure functions: data in, text out.
//!
//! The JSON output copies MCP objects with no change, so it needs no code
//! here. Text output is for reading, and can change between versions.

use super::outcome::chain_text;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use rmcp::model::{JsonObject, Tool};
use serde_json::Value;

use super::tool_args;
use crate::{
    config::UnexpandedEntry,
    config::{Server, Target, Transport},
    project::{Project, Trust},
};

#[must_use]
pub fn transport_name(server: &Server) -> &'static str {
    match server.transport {
        Transport::Stdio(_) => "stdio",
        Transport::Http(_) => "http",
        Transport::Sse(_) => "sse",
    }
}

/// The command line of a stdio server, or the URL of a remote server, as the
/// configuration file writes it: `ncp` does not expand `${NAME}`, so no value
/// of a variable shows.
#[must_use]
pub fn target(server: &Server) -> String {
    target_text(&server.target)
}

#[must_use]
pub fn target_text(target: &Target) -> String {
    match target {
        Target::Stdio { command, args, .. } => std::iter::once(command)
            .chain(args)
            .map(|word| shell_word(word))
            .collect::<Vec<_>>()
            .join(" "),
        Target::Remote { url } => url.clone(),
    }
}

/// What a person checks before a trust, beside the target: the parts of
/// rule 5 of docs/design/project-config.md.
pub struct Shown {
    pub cwd: Option<String>,
    pub env: Vec<String>,
    pub sends: Vec<(String, String)>,
}

/// One row of `server list` and `ncp trust`: an entry with its file.
pub struct Row {
    pub name: String,
    /// The transport and the target, when the loader can read the entry.
    pub target: Option<(&'static str, String)>,
    pub error: Option<Reason>,
    pub file: PathBuf,
    /// `None` for an entry of the global file.
    pub trust: Option<Trust>,
    /// Present for an entry of a project file that the user checks.
    pub shown: Option<Shown>,
}

/// Why an entry is not valid: the text of the error and its causes.
pub struct Reason {
    error: String,
    causes: Vec<String>,
}

impl Reason {
    #[must_use]
    pub fn of(error: &dyn std::error::Error) -> Self {
        let mut texts = super::outcome::chain(error).into_iter();
        Self {
            error: texts.next().unwrap_or_default(),
            causes: texts.collect(),
        }
    }

    /// The error and its causes on one line, for a table cell.
    #[must_use]
    pub fn text(&self) -> String {
        std::iter::once(&self.error)
            .chain(&self.causes)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(": ")
    }
}

impl Row {
    fn json(&self) -> Value {
        let mut row = serde_json::json!({"name": self.name});
        if let Some(reason) = &self.error {
            row["error"] = serde_json::json!(reason.error);
            row["causes"] = serde_json::json!(reason.causes);
        }
        if let Some((transport, target)) = &self.target {
            row["transport"] = serde_json::json!(transport);
            row["target"] = serde_json::json!(target);
        }
        row["scope"] = serde_json::json!(if self.trust.is_some() {
            "project"
        } else {
            "global"
        });
        row["file"] = serde_json::json!(self.file);
        if let Some(trust) = self.trust {
            row["trust"] = serde_json::json!(trust.word());
        }
        if let Some(shown) = &self.shown {
            row["cwd"] = serde_json::json!(shown.cwd);
            row["env"] = serde_json::json!(shown.env);
            row["sends"] = shown
                .sends
                .iter()
                .map(|(name, host)| serde_json::json!({"name": name, "host": host}))
                .collect();
        }
        row
    }
}

/// The JSON of the rows: the servers, and the invalid entries with their
/// errors (design section 4).
#[must_use]
pub fn rows_json(rows: &[Row]) -> (Vec<Value>, Vec<Value>) {
    let (servers, invalid): (Vec<&Row>, Vec<&Row>) =
        rows.iter().partition(|row| row.error.is_none());
    (
        servers.into_iter().map(Row::json).collect(),
        invalid.into_iter().map(Row::json).collect(),
    )
}

/// The rows of the entries of a project file, with no expansion.
#[must_use]
pub fn project_rows(project: &Project) -> Vec<Row> {
    project
        .entries
        .iter()
        .map(|entry| {
            let shape = entry.shape.as_ref().ok();
            Row {
                name: entry.name.clone(),
                target: shape.map(|shape| (shape.transport, target_text(&shape.target))),
                error: match &entry.shape {
                    Ok(shape) => shape.problem.as_ref().map(|problem| Reason::of(problem)),
                    Err(error) => Some(Reason::of(error)),
                },
                file: project.file.clone(),
                trust: Some(project.trust),
                shown: shape.map(|shape| Shown {
                    cwd: match &shape.target {
                        Target::Stdio { cwd, .. } => cwd.clone(),
                        Target::Remote { .. } => None,
                    },
                    env: shape.env.clone(),
                    sends: shape.sends.clone(),
                }),
            }
        })
        .collect()
}

/// One line for each server, for the user to check before a trust. A
/// stdio line has the full command, `cwd` and the names of the `env`
/// variables. An HTTP line has the URL, and each `${NAME}` with the host
/// that gets it.
///
/// No value of `env` or of a header shows, because it can be a literal
/// secret. The output can go into the transcript of an agent. Each word
/// from the file is a shell word, so that a word cannot look like two
/// parts. `table` escapes each cell.
#[must_use]
pub fn project_entries(entries: &[UnexpandedEntry]) -> Vec<String> {
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            let details: Vec<String> = match &entry.shape {
                Ok(shape) => std::iter::once(match &shape.target {
                    Target::Remote { url } => shown_url(url),
                    stdio @ Target::Stdio { .. } => target_text(stdio),
                })
                .chain(match &shape.target {
                    Target::Stdio { cwd, .. } => cwd
                        .as_deref()
                        .map(|cwd| format!("cwd: {}", shell_word(cwd))),
                    Target::Remote { .. } => None,
                })
                .chain((!shape.env.is_empty()).then(|| {
                    let names: Vec<String> =
                        shape.env.iter().map(|name| shell_word(name)).collect();
                    format!("env: {}", names.join(", "))
                }))
                .chain(
                    shape
                        .sends
                        .iter()
                        .map(|(name, host)| format!("{name} -> {}", shell_word(host))),
                )
                .chain(
                    shape
                        .problem
                        .iter()
                        .map(|problem| format!("not valid: {}", chain_text(problem))),
                )
                .collect(),
                Err(error) => vec![format!("not valid: {error}", error = chain_text(&error))],
            };
            vec![entry.name.clone(), details.join("   ")]
        })
        .collect();
    table(&rows).lines().map(str::to_string).collect()
}

/// The rows of a project file with an indent, one on each line.
#[must_use]
pub fn indented_entries(entries: &[UnexpandedEntry], indent: &str) -> String {
    project_entries(entries)
        .iter()
        .fold(String::new(), |mut text, line| {
            let _ = writeln!(text, "{indent}{line}");
            text
        })
}

// The URL as the client sends it. The parse removes tabs and encodes spaces,
// which can make one URL look like two parts. A URL with `${NAME}` shows as a
// shell word.
fn shown_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) if !url.contains("${") => parsed.to_string(),
        _ => shell_word(url),
    }
}

// A path on one line: a folder name can hold a new line.
fn path_text(path: &Path) -> String {
    super::ui::printable(&super::ui::one_line(&path.display().to_string())).into_owned()
}

#[must_use]
pub fn project_question(project: &Project) -> String {
    let state = match project.trust {
        Trust::Untrusted { changed: true } => "changed since you trusted it",
        _ => "is not trusted",
    };
    project_text(project, &format!("{state}. It has these servers:"))
}

/// The text of `ncp trust`: the file with its state, then its entries.
#[must_use]
pub fn project_file(project: &Project) -> String {
    project_text(project, &format!("({})", project.trust.word()))
}

fn project_text(project: &Project, state: &str) -> String {
    format!(
        "{} {state}\n{}",
        path_text(&project.file),
        indented_entries(&project.entries, "  ")
    )
}

/// The text of `server list`: the project file with its state, then one
/// row for each entry. A project entry has a mark with its state.
#[must_use]
pub fn server_list(project: Option<&Project>, rows: &[Row]) -> String {
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let (transport, target) = match (&row.error, &row.target) {
                (Some(reason), _) => ("invalid".to_string(), reason.text()),
                (None, Some((transport, target))) => ((*transport).to_string(), target.clone()),
                (None, None) => (String::new(), String::new()),
            };
            let mark = match row.trust {
                None => String::new(),
                Some(Trust::Trusted) => "project".to_string(),
                Some(trust) => format!("project, {}", trust.word()),
            };
            vec![row.name.clone(), transport, target, mark]
        })
        .collect();
    let head = project
        .map(|project| {
            format!(
                "Project file: {} ({})\n",
                path_text(&project.file),
                project.trust.word()
            )
        })
        .unwrap_or_default();
    head + &table(&rows)
}

/// The word as the user types it in a POSIX shell.
#[must_use]
pub fn shell_word(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-.,/:=@%+".contains(c);
    if !word.is_empty() && word.chars().all(plain) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// Rows with the columns aligned. The last column gets no padding. Each
/// cell gets its escapes before the measure of the columns, so that the
/// escapes do not move the columns.
#[must_use]
pub fn table(rows: &[Vec<String>]) -> String {
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| super::ui::printable(&super::ui::one_line(cell)).into_owned())
                .collect()
        })
        .collect();
    let widths: Vec<usize> = rows.iter().fold(Vec::new(), |widths, row| {
        row.iter()
            .enumerate()
            .map(|(index, cell)| {
                widths
                    .get(index)
                    .copied()
                    .unwrap_or(0)
                    .max(cell.chars().count())
            })
            .chain(widths.iter().skip(row.len()).copied())
            .collect()
    });
    rows.iter().fold(String::new(), |mut text, row| {
        let last = row.len().saturating_sub(1);
        let line: String = row
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                if index == last {
                    cell.clone()
                } else {
                    format!("{cell:<width$}  ", width = widths[index])
                }
            })
            .collect();
        let _ = writeln!(text, "{}", line.trim_end());
        text
    })
}

/// One line for each tool: the name, the title or the first line of the
/// description, and the marks of the annotations.
#[must_use]
pub fn tool_list(tools: &[Tool]) -> String {
    let rows: Vec<Vec<String>> = tools
        .iter()
        .map(|tool| {
            let summary = tool
                .title
                .clone()
                .or_else(|| {
                    tool.description
                        .as_deref()
                        .and_then(|text| text.lines().next())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let marks = hint_marks(tool);
            let summary = if marks.is_empty() {
                summary
            } else {
                format!("{summary}  [{}]", marks.join(", "))
            };
            vec![tool.name.to_string(), summary]
        })
        .collect();
    table(&rows)
}

// The annotations that the server set to true. The spec tells clients to
// treat annotations as untrusted, so each mark says "hint".
fn hint_marks(tool: &Tool) -> Vec<&'static str> {
    let Some(annotations) = &tool.annotations else {
        return Vec::new();
    };
    [
        (annotations.read_only_hint, "read-only hint"),
        (annotations.destructive_hint, "destructive hint"),
        (annotations.idempotent_hint, "idempotent hint"),
        (annotations.open_world_hint, "open-world hint"),
    ]
    .into_iter()
    .filter(|(set, _)| *set == Some(true))
    .map(|(_, mark)| mark)
    .collect()
}

// Each annotation hint with its value, and the default of the spec when the
// server sets none: an absent hint must not look safe. The destructive and
// idempotent hints have a meaning only when `readOnlyHint` is false (MCP
// 2025-11-25, server/tools, `ToolAnnotations`).
fn annotation_lines(tool: &Tool) -> String {
    let annotations = tool.annotations.clone().unwrap_or_default();
    let read_only = annotations.read_only_hint;
    let line = |name: &str, set: Option<bool>, default: bool| {
        let (value, from) = match set {
            Some(value) => (value, ""),
            None => (default, " (default)"),
        };
        format!("  {name}: {}{from}\n", if value { "yes" } else { "no" })
    };
    let mut text = line("read-only", read_only, false);
    if read_only != Some(true) {
        text += &line("destructive", annotations.destructive_hint, true);
        text += &line("idempotent", annotations.idempotent_hint, false);
    }
    text + &line("open-world", annotations.open_world_hint, true)
}

#[must_use]
pub fn tool_show(server: &str, tool: &Tool) -> String {
    let mut text = format!("Tool: {}\n", tool.name);
    if let Some(title) = &tool.title {
        let _ = writeln!(text, "Title: {title}");
    }
    if let Some(description) = &tool.description {
        let _ = writeln!(text, "\n{}", description.trim_end());
    }
    let _ = write!(text, "\nArguments:\n{}", properties(&tool.input_schema));
    let output = tool
        .output_schema
        .as_deref()
        .map_or_else(|| "  not described\n".to_string(), properties);
    let _ = write!(text, "\nOutput:\n{output}");
    let _ = write!(
        text,
        "\nAnnotations (hints from the server, not checked):\n{}",
        annotation_lines(tool)
    );
    let _ = write!(text, "\nExample:\n  {}\n", example_call(server, tool));
    text
}

#[must_use]
pub fn properties(schema: &JsonObject) -> String {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "  none\n".to_string();
    };
    if properties.is_empty() {
        return "  none\n".to_string();
    }
    properties
        .iter()
        .map(|(name, property)| {
            let need = if required.contains(&name.as_str()) {
                "required"
            } else {
                "optional"
            };
            let mut block = format!("  {name} ({}, {need})\n", type_text(schema, property));
            let description = property.get("description").and_then(Value::as_str);
            for line in description.into_iter().flat_map(str::lines) {
                let _ = writeln!(block, "      {line}");
            }
            if let Some(values) = property.get("enum").and_then(Value::as_array) {
                let values: Vec<String> = values.iter().map(Value::to_string).collect();
                let _ = writeln!(block, "      Allowed values: {}", values.join(", "));
            }
            if let Some(default) = property.get("default") {
                let _ = writeln!(block, "      Default: {default}");
            }
            block
        })
        .collect()
}

fn type_text(schema: &JsonObject, property: &Value) -> String {
    let types = tool_args::types(schema, property);
    if types.is_empty() {
        "any".to_string()
    } else {
        types.into_iter().collect::<Vec<_>>().join(" or ")
    }
}

#[must_use]
pub fn example_call(server: &str, tool: &Tool) -> String {
    example_call_with(server, tool, &[])
}

/// An example call with the required arguments, and then the other
/// properties in `also`, for example the ones that failed the check.
#[must_use]
pub fn example_call_with(server: &str, tool: &Tool, also: &[&str]) -> String {
    let schema = &tool.input_schema;
    let properties = schema.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let others = also.iter().copied().filter(|name| {
        !required.contains(name) && properties.is_some_and(|known| known.contains_key(*name))
    });
    let arguments = required.iter().copied().chain(others).map(|name| {
        let property = properties
            .and_then(|known| known.get(name))
            .unwrap_or(&Value::Null);
        shell_word(&format!("{name}=<{}>", type_text(schema, property)))
    });
    std::iter::once(format!(
        "ncp tool call {} {}",
        shell_word(server),
        shell_word(&tool.name)
    ))
    .chain(arguments)
    .collect::<Vec<_>>()
    .join(" ")
}

#[must_use]
pub fn server_show(show: &JsonObject) -> String {
    let text_of = |key: &str| show.get(key).and_then(Value::as_str).unwrap_or("");
    let mut text = format!(
        "Server: {}\nTarget: {}\nProtocol version: {}\n",
        text_of("server"),
        text_of("target"),
        text_of("protocolVersion")
    );
    if let Some(info) = show.get("serverInfo") {
        let field = |key: &str| info.get(key).and_then(Value::as_str);
        let name = field("title").or(field("name")).unwrap_or("");
        let version = field("version").unwrap_or("");
        let _ = writeln!(text, "Server info: {name} {version}");
    }
    let capabilities: Vec<&str> = show
        .get("capabilities")
        .and_then(Value::as_object)
        .map(|map| map.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let capabilities = if capabilities.is_empty() {
        "none".to_string()
    } else {
        capabilities.join(", ")
    };
    let _ = writeln!(text, "Capabilities: {capabilities}");
    if let Some(instructions) = show.get("instructions").and_then(Value::as_str) {
        let _ = writeln!(text, "\nInstructions:");
        for line in instructions.lines() {
            let _ = writeln!(text, "  {line}");
        }
    }
    text
}

/// A tool result for people. Text blocks show as they are. Other blocks
/// show as one summary line each. With no content, the structured content
/// shows as JSON.
#[must_use]
pub fn call_result(result: &Value) -> String {
    let content = result
        .get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if content.is_empty() {
        return result
            .get("structuredContent")
            .map(|value| format!("{}\n", pretty(value)))
            .unwrap_or_default();
    }
    content.iter().map(content_line).collect()
}

// One content block as a line: text as it is, other kinds as a summary.
fn content_line(block: &Value) -> String {
    let field = |key: &str| block.get(key).and_then(Value::as_str).unwrap_or("?");
    let line = match field("type") {
        "text" => field("text").to_string(),
        "image" | "audio" => format!(
            "[{} {}, {} bytes as base64]",
            field("type"),
            field("mimeType"),
            field("data").len()
        ),
        "resource" => format!(
            "[resource {}]",
            block
                .get("resource")
                .and_then(|resource| resource.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("?")
        ),
        "resource_link" => format!("[resource link {}]", field("uri")),
        other => format!("[{other} content]"),
    };
    if line.ends_with('\n') {
        line
    } else {
        format!("{line}\n")
    }
}

fn first_line(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|text| text.lines().next())
        .unwrap_or_default()
        .to_string()
}

/// `resource list`: the resources, then the templates, each with its
/// title or the first line of its description.
#[must_use]
pub fn resource_list(resources: &[Value], templates: &[Value]) -> String {
    let rows = |items: &[Value], uri_key: &str| -> Vec<Vec<String>> {
        items
            .iter()
            .map(|item| {
                let summary = item
                    .get("title")
                    .and_then(Value::as_str)
                    .map_or_else(|| first_line(item, "description"), str::to_string);
                vec![
                    item.get(uri_key)
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    first_line(item, "mimeType"),
                    summary,
                ]
            })
            .collect()
    };
    let section = |title: &str, items: &[Value], uri_key: &str| {
        if items.is_empty() {
            format!("{title}: none\n")
        } else {
            format!("{title}:\n{}", table(&rows(items, uri_key)))
        }
    };
    format!(
        "{}\n{}",
        section("Resources", resources, "uri"),
        section("Templates", templates, "uriTemplate")
    )
}

/// `resource read` on a TTY shows text as it is, and the caller escapes
/// control characters. Each blob gets a summary line, because binary data
/// can change a terminal.
#[must_use]
pub fn resource_contents(result: &Value) -> String {
    let contents = result
        .get("contents")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    contents.iter().fold(String::new(), |mut text, content| {
        match (content.get("text").and_then(Value::as_str), content.get("blob").and_then(Value::as_str)) {
            (Some(body), _) => text.push_str(body),
            (None, Some(blob)) => {
                let mime = content.get("mimeType").and_then(Value::as_str).unwrap_or("binary data");
                let _ = writeln!(
                    text,
                    "[{mime}, {} bytes as base64; use --output-format text and send stdout to a file]",
                    blob.len()
                );
            }
            (None, None) => {}
        }
        text
    })
}

/// `prompt list`: the name, the first line of the description, and the
/// arguments, with the required ones marked.
#[must_use]
pub fn prompt_list(prompts: &[Value]) -> String {
    let rows: Vec<Vec<String>> = prompts
        .iter()
        .map(|prompt| {
            let arguments: Vec<String> = prompt
                .get("arguments")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|argument| {
                    let name = argument.get("name").and_then(Value::as_str).unwrap_or("?");
                    if argument.get("required") == Some(&Value::Bool(true)) {
                        format!("{name}=")
                    } else {
                        format!("[{name}=]")
                    }
                })
                .collect();
            vec![
                prompt
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                first_line(prompt, "description"),
                arguments.join(" "),
            ]
        })
        .collect();
    table(&rows)
}

/// `prompt get`: each message as `role: content`.
#[must_use]
pub fn prompt_messages(result: &Value) -> String {
    result
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .fold(String::new(), |mut text, message| {
            let role = message.get("role").and_then(Value::as_str).unwrap_or("?");
            let content = message.get("content").map(content_line).unwrap_or_default();
            let _ = write!(text, "{role}: {content}");
            text
        })
}

#[must_use]
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use rmcp::model::ToolAnnotations;
    use serde_json::json;

    use super::*;
    use crate::{
        config::{RemoteServer, StdioServer},
        server::{ServerName, ServerUrl},
    };

    fn object(value: Value) -> JsonObject {
        match value {
            Value::Object(map) => map,
            _ => JsonObject::new(),
        }
    }

    fn tool(schema: Value) -> Tool {
        Tool::new("search", "Find issues\nMore text", Arc::new(object(schema)))
    }

    #[test]
    fn quotes_shell_words_only_when_needed() {
        assert_eq!(shell_word("a-b/c.d=1"), "a-b/c.d=1");
        assert_eq!(shell_word("a b"), "'a b'");
        assert_eq!(shell_word("it's"), r"'it'\''s'");
        assert_eq!(shell_word(""), "''");
    }

    #[test]
    fn target_is_the_command_line_or_the_url() {
        let stdio = Server::new(
            ServerName::parse("s").unwrap(),
            Transport::Stdio(StdioServer {
                command: "/bin/srv".into(),
                args: vec!["--name".into(), "a b".into()],
                env: BTreeMap::new(),
                cwd: None,
            }),
        );
        assert_eq!(target(&stdio), "/bin/srv --name 'a b'");
        let http = Server::new(
            ServerName::parse("r").unwrap(),
            Transport::Http(RemoteServer {
                url: ServerUrl::parse("https://example.com/mcp").unwrap(),
                headers: BTreeMap::new(),
                oauth_client: None,
                url_values: Vec::new(),
            }),
        );
        assert_eq!(target(&http), "https://example.com/mcp");
        assert_eq!(transport_name(&http), "http");
    }

    #[test]
    fn example_call_adds_the_failed_properties_after_the_required_ones() {
        let tool = Tool::new(
            "search",
            "Search",
            Arc::new(
                serde_json::from_value(json!({"type": "object", "properties": {
                    "query": {"type": "string"}, "limit": {"type": "integer"}},
                    "required": ["query"]}))
                .unwrap(),
            ),
        );
        assert_eq!(
            example_call_with("web", &tool, &["limit", "query", "nothing"]),
            "ncp tool call web search 'query=<string>' 'limit=<integer>'"
        );
    }

    #[test]
    fn aligns_table_columns() {
        let rows = vec![
            vec!["a".to_string(), "stdio".to_string(), "x".to_string()],
            vec!["long".to_string(), "http".to_string(), "y".to_string()],
        ];
        assert_eq!(table(&rows), "a     stdio  x\nlong  http   y\n");
    }

    #[test]
    fn table_measures_cells_after_the_escapes() {
        let rows = vec![
            vec!["a\u{1b}".to_string(), "x".to_string()],
            vec!["b\nc".to_string(), "y".to_string()],
            vec!["long-name".to_string(), "z".to_string()],
        ];
        assert_eq!(table(&rows), "a\\u{1b}    x\nb\\nc       y\nlong-name  z\n");
    }

    #[test]
    fn tool_list_shows_first_description_line_and_hint_marks() {
        let mut marked = tool(json!({}));
        marked.annotations = Some(ToolAnnotations::new().read_only(true));
        let mut titled = Tool::new("b", "desc", Arc::new(JsonObject::new()));
        titled.title = Some("Title B".into());
        assert_eq!(
            tool_list(&[marked, titled]),
            "search  Find issues  [read-only hint]\nb       Title B\n"
        );
    }

    #[test]
    fn tool_show_gives_each_annotation_hint_and_the_spec_defaults() {
        let plain = Tool::new("a", "A", Arc::new(serde_json::Map::new()));
        assert_eq!(
            annotation_lines(&plain),
            "  read-only: no (default)\n  destructive: yes (default)\n  \
             idempotent: no (default)\n  open-world: yes (default)\n"
        );
        let read_only = plain
            .clone()
            .annotate(ToolAnnotations::new().read_only(true).open_world(false));
        assert_eq!(
            annotation_lines(&read_only),
            "  read-only: yes\n  open-world: no\n"
        );
    }

    #[test]
    fn tool_show_lists_arguments_output_and_an_example() {
        let tool = tool(json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Words to find"},
                "state": {"type": "string", "enum": ["open", "closed"], "default": "open"},
                "limit": {"type": ["integer", "null"]}
            },
            "required": ["query"]
        }));
        let text = tool_show("gh", &tool);
        assert!(
            text.starts_with("Tool: search\n\nFind issues\nMore text\n"),
            "{text}"
        );
        assert!(text.contains("  query (string, required)\n      Words to find\n"));
        assert!(text.contains(
            "  state (string, optional)\n      Allowed values: \"open\", \"closed\"\n      Default: \"open\"\n"
        ));
        assert!(text.contains("  limit (integer or null, optional)\n"));
        assert!(text.contains("Output:\n  not described\n"));
        assert!(text.ends_with("Example:\n  ncp tool call gh search 'query=<string>'\n"));
    }

    #[test]
    fn tool_with_no_arguments_says_none() {
        let tool = tool(json!({"type": "object"}));
        let text = tool_show("gh", &tool);
        assert!(text.contains("Arguments:\n  none\n"));
        assert!(text.ends_with("ncp tool call gh search\n"));
    }

    #[test]
    fn server_show_text_has_the_handshake_fields() {
        let show = object(json!({
            "server": "gh", "target": "https://x/mcp", "protocolVersion": "2026-07-28",
            "serverInfo": {"name": "srv", "version": "1.2"},
            "capabilities": {"tools": {}, "resources": {}},
            "instructions": "Use search first.\nThen read."
        }));
        assert_eq!(
            server_show(&show),
            "Server: gh\nTarget: https://x/mcp\nProtocol version: 2026-07-28\n\
             Server info: srv 1.2\nCapabilities: resources, tools\n\n\
             Instructions:\n  Use search first.\n  Then read.\n"
        );
    }

    #[test]
    fn call_result_shows_text_and_summarizes_other_blocks() {
        let result = json!({"content": [
            {"type": "text", "text": "hello"},
            {"type": "image", "mimeType": "image/png", "data": "AAAA"},
            {"type": "resource_link", "uri": "file:///a", "name": "a"},
            {"type": "resource", "resource": {"uri": "file:///b", "text": "b"}}
        ]});
        assert_eq!(
            call_result(&result),
            "hello\n[image image/png, 4 bytes as base64]\n[resource link file:///a]\n[resource file:///b]\n"
        );
    }

    #[test]
    fn call_result_with_no_content_shows_structured_content() {
        let result = json!({"content": [], "structuredContent": {"a": 1}});
        assert_eq!(call_result(&result), "{\n  \"a\": 1\n}\n");
    }
}
