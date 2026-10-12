//! Edits of a configuration file that keep each byte that `ncp` does not
//! change: comments, the indent, the order and each unknown key. Pure
//! functions from the old text to the new text, through a concrete syntax
//! tree, as `deno add` edits `deno.json`.
//!
//! Lenient when it reads, strict when it writes: the new entry is strict
//! JSON, and the edit never writes over content that it cannot read.

use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde_json::{Map, Value};

use crate::config::{self, ConfigError};

const SERVERS_KEY: &str = "mcpServers";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The edit of the tree finds the first copy of a key, and the loader
    /// uses the last, so `ncp` does not choose.
    #[error("the file has the key {0:?} more than one time; remove the copies first")]
    Duplicate(String),
    #[error("the file has no server {0:?}")]
    NotFound(String),
    #[error("the file already has a server {0:?}")]
    Exists(String),
    /// The new text does not parse to the old content with the one change.
    #[error("the edit does not give the expected content")]
    SelfCheck,
}

/// The entry of the name as the file writes it, with no check.
///
/// # Errors
///
/// Returns an error when the text does not parse.
pub fn entry(text: &str, name: &str) -> Result<Option<Value>, EditError> {
    Ok(config::raw_servers(base(text))?.get(name).cloned())
}

/// Adds the entry at the end of `mcpServers`, with the indent of the
/// entries next to it. An empty text is a new file.
///
/// # Errors
///
/// Returns an error when the text does not parse, has a duplicate key on
/// the path of the edit, or the self-check fails.
pub fn add(text: &str, name: &str, fields: &[(String, Value)]) -> Result<String, EditError> {
    let (old, root, object) = tree(text)?;
    let servers = match single(&object, SERVERS_KEY)? {
        Some(_) => object
            .object_value(SERVERS_KEY)
            .ok_or(ConfigError::ServersNotObject)?,
        None => object.object_value_or_set(SERVERS_KEY),
    };
    if single(&servers, name)?.is_some() {
        return Err(EditError::Exists(name.to_string()));
    }
    servers.append(
        name,
        CstInputValue::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), input(value)))
                .collect(),
        ),
    );
    let mut expected = old;
    let mut entries = config::raw_servers(base(text))?;
    entries.insert(
        name.to_string(),
        Value::Object(fields.iter().cloned().collect()),
    );
    expected.insert(SERVERS_KEY.to_string(), Value::Object(entries));
    finish(text, &root, &expected)
}

/// Removes the entry by its raw key, so that an invalid entry or a name
/// outside the name rule can go too.
///
/// # Errors
///
/// Returns an error when the text does not parse, has no entry of the
/// name, has a duplicate key on the path of the edit, or the self-check
/// fails.
pub fn remove(text: &str, name: &str) -> Result<String, EditError> {
    let (old, root, object) = tree(text)?;
    let servers = single(&object, SERVERS_KEY)?
        .and_then(|_| object.object_value(SERVERS_KEY))
        .ok_or_else(|| EditError::NotFound(name.to_string()))?;
    single(&servers, name)?
        .ok_or_else(|| EditError::NotFound(name.to_string()))?
        .remove();
    let mut expected = old;
    let mut entries = config::raw_servers(text)?;
    entries.remove(name);
    expected.insert(SERVERS_KEY.to_string(), Value::Object(entries));
    finish(text, &root, &expected)
}

/// A new file starts as an empty object.
fn base(text: &str) -> &str {
    if text.trim().is_empty() { "{}\n" } else { text }
}

/// The old content, the tree, and its top-level object. The tree parses
/// with the options of the loader, not with the defaults of the crate,
/// which accept JSON5 forms.
fn tree(text: &str) -> Result<(Map<String, Value>, CstRootNode, CstObject), EditError> {
    let text = base(text);
    let old = config::parse_root(text)?;
    let root = CstRootNode::parse(text, &config::JSONC)
        .map_err(|error| ConfigError::Syntax(error.to_string()))?;
    let object = root.object_value().ok_or(ConfigError::NotObject)?;
    Ok((old, root, object))
}

/// The one property of the name, or none. Two copies are an error.
fn single(
    object: &CstObject,
    name: &str,
) -> Result<Option<jsonc_parser::cst::CstObjectProp>, EditError> {
    let mut found = object
        .properties()
        .into_iter()
        .filter(|prop| prop.decoded_name().as_deref() == Some(name));
    match (found.next(), found.next()) {
        (_, Some(_)) => Err(EditError::Duplicate(name.to_string())),
        (prop, None) => Ok(prop),
    }
}

/// The new text, after a parse that must give the expected content. A new
/// file ends with a new line.
fn finish(
    old: &str,
    root: &CstRootNode,
    expected: &Map<String, Value>,
) -> Result<String, EditError> {
    let text = root.to_string();
    let text = if old.trim().is_empty() && !text.ends_with('\n') {
        text + "\n"
    } else {
        text
    };
    match config::parse_root(&text) {
        Ok(new) if new == *expected => Ok(text),
        _ => Err(EditError::SelfCheck),
    }
}

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(flag) => CstInputValue::Bool(*flag),
        Value::Number(number) => CstInputValue::Number(number.to_string()),
        Value::String(text) => CstInputValue::String(text.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(input).collect()),
        Value::Object(map) => CstInputValue::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), input(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fields() -> Vec<(String, Value)> {
        vec![
            ("type".into(), json!("http")),
            ("url".into(), json!("https://x.test/mcp")),
        ]
    }

    #[test]
    fn a_new_file_gets_an_indent_of_two_and_a_last_new_line() {
        let text = add("", "web", &fields()).unwrap();
        assert_eq!(
            text,
            "{\n  \"mcpServers\": {\n    \"web\": {\n      \"type\": \"http\",\n      \"url\": \"https://x.test/mcp\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn an_add_keeps_comments_order_indent_and_unknown_keys() {
        let old = "{\n    // my servers\n    \"theme\": 1,\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": true}, // keep\n    },\n}\n";
        let new = add(old, "web", &fields()).unwrap();
        assert!(new.starts_with("{\n    // my servers\n    \"theme\": 1,\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": true}, // keep\n"), "{new}");
        assert!(new.contains("        \"web\": {"), "{new}");
        let servers = config::raw_servers(&new).unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["a", "web"]);
    }

    #[test]
    fn a_file_with_no_servers_map_gets_one() {
        let new = add("{\"theme\": 1}", "web", &fields()).unwrap();
        assert_eq!(config::parse_root(&new).unwrap()["theme"], json!(1));
        assert!(config::raw_servers(&new).unwrap().contains_key("web"));
    }

    #[test]
    fn a_remove_keeps_the_other_entries_and_bytes() {
        let old = "{\n  // top\n  \"mcpServers\": {\n    \"a\": {\"command\": \"x\"},\n    \"bad.name\": 1,\n    \"b\": {\"command\": \"y\"}\n  }\n}\n";
        let new = remove(old, "bad.name").unwrap();
        assert!(new.starts_with("{\n  // top\n"), "{new}");
        let servers = config::raw_servers(&new).unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(remove(old, "none"), Err(EditError::NotFound("none".into())));
    }

    #[test]
    fn refuses_duplicate_keys_on_the_path_and_text_it_cannot_read() {
        let twice = r#"{"mcpServers": {"a": {"command": "x"}, "a": {"command": "y"}}}"#;
        assert_eq!(remove(twice, "a"), Err(EditError::Duplicate("a".into())));
        assert_eq!(
            add(twice, "a", &fields()),
            Err(EditError::Duplicate("a".into()))
        );
        assert_eq!(
            add(r#"{"mcpServers": {"a": 1}}"#, "a", &fields()),
            Err(EditError::Exists("a".into()))
        );
        let servers_twice = r#"{"mcpServers": {}, "mcpServers": {}}"#;
        assert_eq!(
            add(servers_twice, "web", &fields()),
            Err(EditError::Duplicate("mcpServers".into()))
        );
        assert!(matches!(
            add("{'mcpServers': {}}", "web", &fields()),
            Err(EditError::Config(ConfigError::Syntax(_)))
        ));
        assert!(matches!(
            add("[]", "web", &fields()),
            Err(EditError::Config(ConfigError::NotObject))
        ));
    }

    #[test]
    fn reads_the_entry_of_a_name_as_the_file_writes_it() {
        let text = r#"{"mcpServers": {"a": {"command": "${X}"}}}"#;
        assert_eq!(entry(text, "a").unwrap(), Some(json!({"command": "${X}"})));
        assert_eq!(entry("", "a").unwrap(), None);
    }
}
