//! Edits of a configuration file or a project file that keep each byte that
//! `ncp` does not change: comments, the indent, the order and each unknown key.
//! Pure functions from the old text to the new text, through a concrete
//! syntax tree. A user keeps comments in these files, and the diff of a
//! change in a repository must show only the change.
//!
//! Lenient when it reads, strict when it writes: the new entry is strict
//! JSON, and the edit never writes over content that it cannot read.

use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde_json::{Map, Value};

use crate::config::{self, ConfigError, SERVERS_KEY};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The edit of the tree finds the first copy of a key, and the loader
    /// uses the last, so `ncp` does not choose.
    #[error("The file has the key {} more than one time", crate::text::code(.0))]
    Duplicate(String),
    #[error("The file has no entry of the server {}", crate::text::code(.0))]
    NotFound(String),
    #[error("The file already has an entry of the server {}", crate::text::code(.0))]
    Exists(String),
    /// The new text does not parse to the old content with the one change.
    #[error("The edit does not give the expected content")]
    SelfCheck,
}

/// The entry of the name as the file writes it. A duplicate key on the
/// path is an error, as for an edit.
///
/// # Errors
///
/// Returns an error when the text does not parse or has a duplicate key.
pub fn entry(text: &str, name: &str) -> Result<Option<Value>, EditError> {
    let (old, _, object) = tree(text)?;
    let found = match single(&object, SERVERS_KEY)? {
        Some(_) => object
            .object_value(SERVERS_KEY)
            .map(|servers| single(&servers, name))
            .transpose()?
            .flatten(),
        None => None,
    };
    Ok(found.and_then(|_| old.get(SERVERS_KEY)?.get(name).cloned()))
}

/// Adds the entry at the end of `mcpServers`, with the indent of the
/// entries next to it. An empty text is a new file. `None` means that the
/// file has the same entry already. Thus a retry after a failure passes.
///
/// # Errors
///
/// Returns an error when:
/// - the text does not parse
/// - the path of the edit has a duplicate key
/// - the file has another entry of the name
/// - the self-check fails
pub fn add(
    text: &str,
    name: &str,
    fields: &[(String, Value)],
) -> Result<Option<String>, EditError> {
    let value = Value::Object(fields.iter().cloned().collect());
    match entry(text, name)? {
        Some(old) if old == value => return Ok(None),
        Some(_) => return Err(EditError::Exists(name.to_string())),
        None => {}
    }
    let (old, root, object) = tree(text)?;
    let servers = match single(&object, SERVERS_KEY)? {
        Some(_) => object
            .object_value(SERVERS_KEY)
            .ok_or(ConfigError::ServersNotObject)?,
        None => object.object_value_or_set(SERVERS_KEY),
    };
    servers.append(
        name,
        CstInputValue::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), input(value)))
                .collect(),
        ),
    );
    finish(text, &root, &expected(old, name, Some(value))).map(Some)
}

/// Removes the entry by its raw key, so that an entry that is not valid or a
/// name outside the name rule can go too.
///
/// # Errors
///
/// Returns an error when:
/// - the text does not parse
/// - the file has no entry of the name
/// - the path of the edit has a duplicate key
/// - the self-check fails
pub fn remove(text: &str, name: &str) -> Result<String, EditError> {
    let (old, root, object) = tree(text)?;
    let servers = single(&object, SERVERS_KEY)?
        .and_then(|_| object.object_value(SERVERS_KEY))
        .ok_or_else(|| EditError::NotFound(name.to_string()))?;
    single(&servers, name)?
        .ok_or_else(|| EditError::NotFound(name.to_string()))?
        .remove();
    finish(text, &root, &expected(old, name, None))
}

// The old content with the one change: the entry of the name added, or
// removed with `None`.
fn expected(mut old: Map<String, Value>, name: &str, entry: Option<Value>) -> Map<String, Value> {
    let mut servers = match old.remove(SERVERS_KEY) {
        Some(Value::Object(servers)) => servers,
        _ => Map::new(),
    };
    match entry {
        Some(entry) => servers.insert(name.to_string(), entry),
        None => servers.remove(name),
    };
    old.insert(SERVERS_KEY.to_string(), Value::Object(servers));
    old
}

// A new file starts as an empty object.
fn base(text: &str) -> &str {
    if text.trim().is_empty() { "{}\n" } else { text }
}

// The old content, the tree, and its top-level object. The tree parses
// with the options of the loader, not with the defaults of the crate,
// which accept JSON5 forms.
fn tree(text: &str) -> Result<(Map<String, Value>, CstRootNode, CstObject), EditError> {
    let text = base(text);
    let old = config::parse_root(text)?;
    let root = CstRootNode::parse(text, &config::JSONC)
        .map_err(|error| ConfigError::Syntax(config::ParserText(error.to_string())))?;
    let object = root.object_value().ok_or(ConfigError::NotObject)?;
    Ok((old, root, object))
}

// The one property of the name, or none. Two copies are an error.
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

// The new text, after a parse that must give the expected content. A new
// file ends with a new line.
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

    #[test]
    fn error_texts_follow_the_message_rules() {
        let texts = [
            EditError::Duplicate("web".into()),
            EditError::NotFound("web".into()),
            EditError::Exists("web".into()),
            EditError::SelfCheck,
        ]
        .map(|error| error.to_string());
        crate::style::assert_style(&texts);
    }

    fn fields() -> Vec<(String, Value)> {
        vec![
            ("type".into(), json!("http")),
            ("url".into(), json!("https://x.test/mcp")),
        ]
    }

    #[test]
    fn a_new_file_gets_an_indent_of_two_and_a_last_new_line() {
        let text = add("", "web", &fields()).unwrap().unwrap();
        assert_eq!(
            text,
            "{\n  \"mcpServers\": {\n    \"web\": {\n      \"type\": \"http\",\n      \"url\": \"https://x.test/mcp\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn an_add_keeps_comments_order_indent_and_unknown_keys() {
        let old = "{\n    // my servers\n    \"theme\": 1,\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": true}, // keep\n    },\n}\n";
        let new = add(old, "web", &fields()).unwrap().unwrap();
        assert!(new.starts_with("{\n    // my servers\n    \"theme\": 1,\n    \"mcpServers\": {\n        \"a\": {\"command\": \"x\", \"odd\": true}, // keep\n"), "{new}");
        assert!(new.contains("        \"web\": {"), "{new}");
        let servers = config::raw_servers(&new).unwrap();
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["a", "web"]);
    }

    #[test]
    fn a_file_with_no_servers_map_gets_one() {
        let new = add("{\"theme\": 1}", "web", &fields()).unwrap().unwrap();
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
        assert_eq!(entry(twice, "a"), Err(EditError::Duplicate("a".into())));
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
    fn the_same_entry_again_is_no_change() {
        let text = add("", "web", &fields()).unwrap().unwrap();
        assert_eq!(add(&text, "web", &fields()), Ok(None));
    }

    #[test]
    fn reads_the_entry_of_a_name_as_the_file_writes_it() {
        let text = r#"{"mcpServers": {"a": {"command": "${X}"}}}"#;
        assert_eq!(entry(text, "a").unwrap(), Some(json!({"command": "${X}"})));
        assert_eq!(entry("", "a").unwrap(), None);
    }
}
