//! Text output for people on a TTY. Pure functions: data in, text out.
//!
//! The JSON output copies MCP objects with no change, so it needs no code
//! here. Text output is for reading, and can change between versions.

use std::fmt::Write as _;

use rmcp::model::{JsonObject, Tool};
use serde_json::Value;

use super::tool_args;
use crate::config::{Server, Target, Transport};

/// The transport name of an entry.
#[must_use]
pub fn transport_name(server: &Server) -> &'static str {
    match server.transport {
        Transport::Stdio(_) => "stdio",
        Transport::Http(_) => "http",
        Transport::Sse(_) => "sse",
    }
}

/// The command line of a stdio server, or the URL of a remote server, as
/// the configuration file writes it: `${NAME}` is not expanded, so no value
/// of a variable shows.
#[must_use]
pub fn target(server: &Server) -> String {
    match &server.target {
        Target::Stdio { command, args, .. } => std::iter::once(command)
            .chain(args)
            .map(|word| shell_word(word))
            .collect::<Vec<_>>()
            .join(" "),
        Target::Remote { url } => url.clone(),
    }
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

/// Rows with the columns aligned. The last column is not padded.
#[must_use]
pub fn table(rows: &[Vec<String>]) -> String {
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

/// The annotations that the server set to true. The spec tells clients to
/// treat annotations as untrusted, so each mark says "hint".
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

/// One tool for people to read.
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
    let marks = hint_marks(tool);
    if !marks.is_empty() {
        let _ = writeln!(
            text,
            "\nAnnotations (from the server, not checked): {}",
            marks.join(", ")
        );
    }
    let _ = write!(text, "\nExample:\n  {}\n", example_call(server, tool));
    text
}

/// The properties of an object schema, one block for each property.
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

/// A call with each required argument.
#[must_use]
pub fn example_call(server: &str, tool: &Tool) -> String {
    let schema = &tool.input_schema;
    let required: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| {
                    let property = schema
                        .get("properties")
                        .and_then(|properties| properties.get(name))
                        .unwrap_or(&Value::Null);
                    shell_word(&format!("{name}=<{}>", type_text(schema, property)))
                })
                .collect()
        })
        .unwrap_or_default();
    std::iter::once(format!(
        "ncp tool call {} {}",
        shell_word(server),
        shell_word(&tool.name)
    ))
    .chain(required)
    .collect::<Vec<_>>()
    .join(" ")
}

/// The handshake data of a server, from the JSON object of `server show`.
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

/// One content block as a line: text as it is, other kinds as a summary.
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

/// The first line of a text field, or empty.
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
                oauth_client_id: None,
            }),
        );
        assert_eq!(target(&http), "https://example.com/mcp");
        assert_eq!(transport_name(&http), "http");
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
