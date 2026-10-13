//! Tool arguments from the command line: `key=value` pairs, one JSON
//! object, or `-` for a JSON object on stdin. A JSON value goes in the
//! object, and the text of a file comes from the shell, for example
//! `key="$(cat FILE)"` (design B57).

use std::collections::BTreeSet;

use serde_json::Value;

use rmcp::model::JsonObject;

/// One `key=value` pair. The value is text; the schema decides the type.
pub type Pair = (String, String);

/// The arguments after `ncp` read stdin: the input of the schema rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Pairs(Vec<Pair>),
    Object(JsonObject),
}

/// The arguments as the user wrote them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Pairs(Vec<Pair>),
    Object(JsonObject),
    /// `-`: read one JSON object from stdin.
    Stdin,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgumentError {
    #[error("argument {0:?} is not key=value, a JSON object, or -")]
    NotPair(String),
    #[error(
        "argument {0:?} is key:=json, which ncp does not read; give the arguments as one JSON object, for example '{{\"n\": 12}}'"
    )]
    JsonPair(String),
    #[error("argument {0:?} has an empty key")]
    EmptyKey(String),
    #[error("key {0:?} is given more than one time")]
    Repeated(String),
    #[error("{0} must be the only argument")]
    NotAlone(&'static str),
    #[error("the arguments are not a JSON object: {0}")]
    NotObject(String),
}

/// # Errors
///
/// Returns an error for a word that is not an argument form, a repeated
/// key, or a JSON object or `-` together with other words.
pub fn parse(words: &[String]) -> Result<Input, ArgumentError> {
    match words {
        [only] if only == "-" => Ok(Input::Stdin),
        [only] if only.starts_with('{') => object(only).map(Input::Object),
        _ => {
            if words.iter().any(|word| word == "-") {
                return Err(ArgumentError::NotAlone("-"));
            }
            if words.iter().any(|word| word.starts_with('{')) {
                return Err(ArgumentError::NotAlone("a JSON object"));
            }
            pairs(words).map(Input::Pairs)
        }
    }
}

/// # Errors
///
/// Returns an error when the text is not one JSON object.
pub fn object(text: &str) -> Result<JsonObject, ArgumentError> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(ArgumentError::NotObject("it is another JSON value".into())),
        Err(error) => Err(ArgumentError::NotObject(error.to_string())),
    }
}

fn pairs(words: &[String]) -> Result<Vec<Pair>, ArgumentError> {
    let pairs = words
        .iter()
        .map(|word| pair(word))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = BTreeSet::new();
    match pairs.iter().find(|(key, _)| !seen.insert(key.as_str())) {
        Some((key, _)) => Err(ArgumentError::Repeated(key.clone())),
        None => Ok(pairs),
    }
}

/// One pair. The first `=` splits the word. A value that starts with `@`
/// is text, for example an npm scope.
fn pair(word: &str) -> Result<Pair, ArgumentError> {
    let (key, value) = word
        .split_once('=')
        .ok_or_else(|| ArgumentError::NotPair(word.to_string()))?;
    if key.is_empty() {
        return Err(ArgumentError::EmptyKey(word.to_string()));
    }
    // An older form: a key that ends in `:` would go out with a wrong name.
    if key.ends_with(':') {
        return Err(ArgumentError::JsonPair(word.to_string()));
    }
    Ok((key.to_string(), value.to_string()))
}

/// True when the arguments need the input schema for their types.
#[must_use]
pub fn needs_schema(resolved: &Resolved) -> bool {
    match resolved {
        Resolved::Pairs(pairs) => !pairs.is_empty(),
        Resolved::Object(_) => false,
    }
}

/// A value is text, except when the input schema gives the property a type
/// that is not `string`. Then `ncp` reads the value as JSON. A value that
/// is not valid JSON stays text, so that the schema check names the
/// problem.
#[must_use]
pub fn coerce(resolved: &Resolved, schema: &JsonObject) -> JsonObject {
    match resolved {
        Resolved::Object(object) => object.clone(),
        Resolved::Pairs(pairs) => pairs
            .iter()
            .map(|(key, text)| {
                let value = if reads_as_json(schema, key) {
                    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
                } else {
                    Value::String(text.clone())
                };
                (key.clone(), value)
            })
            .collect(),
    }
}

/// True when the property has a JSON type and no `string` type.
fn reads_as_json(schema: &JsonObject, key: &str) -> bool {
    const JSON_TYPES: [&str; 5] = ["number", "integer", "boolean", "array", "object"];
    let types = schema
        .get("properties")
        .and_then(|properties| properties.get(key))
        .map(|property| types(schema, property))
        .unwrap_or_default();
    !types.contains("string") && types.iter().any(|name| JSON_TYPES.contains(name))
}

/// The limit of nested schemas to follow. A `$ref` can point to itself.
const MAX_DEPTH: usize = 8;

/// The JSON types that a property schema permits. Tool SDKs write optional
/// and nested values in these forms:
/// - `type`
/// - each branch of `anyOf`, `oneOf` and `allOf`
/// - a `$ref` into the same document
/// - the values of `enum` and `const`
#[must_use]
pub fn types<'a>(root: &'a JsonObject, property: &'a Value) -> BTreeSet<&'a str> {
    let mut found = BTreeSet::new();
    collect(root, property, 0, &mut BTreeSet::new(), &mut found);
    found
}

/// Each `$ref` resolves one time only. A server can send a schema whose
/// references form a loop or a wide tree, and that must not stop `ncp`.
fn collect<'a>(
    root: &'a JsonObject,
    schema: &'a Value,
    depth: usize,
    seen: &mut BTreeSet<&'a str>,
    found: &mut BTreeSet<&'a str>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    match schema.get("type") {
        Some(Value::String(name)) => {
            found.insert(name.as_str());
        }
        Some(Value::Array(names)) => found.extend(names.iter().filter_map(Value::as_str)),
        _ => {
            let values = schema
                .get("enum")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            found.extend(
                values
                    .iter()
                    .chain(schema.get("const"))
                    .map(|value| json_type(value)),
            );
        }
    }
    for branch in ["anyOf", "oneOf", "allOf"]
        .iter()
        .filter_map(|key| schema.get(*key).and_then(Value::as_array))
        .flatten()
    {
        collect(root, branch, depth + 1, seen, found);
    }
    let reference = schema.get("$ref").and_then(Value::as_str);
    if let Some(target) = reference
        .filter(|reference| depth < MAX_DEPTH && seen.insert(reference))
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| local_target(root, pointer))
    {
        collect(root, target, depth + 1, seen, found);
    }
}

/// The schema at a JSON pointer in the root schema (RFC 6901).
fn local_target<'a>(root: &'a JsonObject, pointer: &str) -> Option<&'a Value> {
    let path = pointer.strip_prefix('/')?;
    let (first, rest) = path.split_once('/').unwrap_or((path, ""));
    let start = root.get(&first.replace("~1", "/").replace("~0", "~"))?;
    if rest.is_empty() {
        Some(start)
    } else {
        start.pointer(&format!("/{rest}"))
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    fn schema(value: Value) -> JsonObject {
        match value {
            Value::Object(map) => map,
            _ => JsonObject::new(),
        }
    }

    fn text(key: &str, value: &str) -> Pair {
        (key.into(), value.into())
    }

    #[test]
    fn reads_key_value_pairs_and_splits_at_the_first_equals_sign() {
        assert_eq!(
            parse(&words(&["a=1", "b=x=y", "c="])),
            Ok(Input::Pairs(vec![
                text("a", "1"),
                text("b", "x=y"),
                text("c", "")
            ]))
        );
        assert_eq!(parse(&[]), Ok(Input::Pairs(Vec::new())));
    }

    #[test]
    fn reads_one_json_object_or_stdin_marker() {
        assert_eq!(
            parse(&words(&[r#"{"a": 1}"#])),
            Ok(Input::Object(schema(json!({"a": 1}))))
        );
        assert_eq!(parse(&words(&["-"])), Ok(Input::Stdin));
    }

    #[test]
    fn reads_an_at_sign_as_text_and_refuses_the_json_pair_form() {
        assert_eq!(
            parse(&words(&["scope=@acme/pkg", "a:b=1"])),
            Ok(Input::Pairs(vec![
                text("scope", "@acme/pkg"),
                text("a:b", "1")
            ]))
        );
        assert_eq!(
            parse(&words(&["n:=12"])),
            Err(ArgumentError::JsonPair("n:=12".into()))
        );
    }

    #[test]
    fn rejects_repeated_key() {
        assert_eq!(
            parse(&words(&["a=1", "a=2"])),
            Err(ArgumentError::Repeated("a".into()))
        );
    }

    #[test]
    fn rejects_word_that_is_no_argument_form() {
        assert_eq!(
            parse(&words(&["a"])),
            Err(ArgumentError::NotPair("a".into()))
        );
        assert_eq!(
            parse(&words(&["=1"])),
            Err(ArgumentError::EmptyKey("=1".into()))
        );
    }

    #[test]
    fn json_object_and_stdin_marker_must_be_alone() {
        assert_eq!(
            parse(&words(&["-", "a=1"])),
            Err(ArgumentError::NotAlone("-"))
        );
        assert_eq!(
            parse(&words(&["{}", "a=1"])),
            Err(ArgumentError::NotAlone("a JSON object"))
        );
    }

    #[test]
    fn rejects_json_that_is_not_an_object() {
        assert!(matches!(object("[1]"), Err(ArgumentError::NotObject(_))));
        assert!(matches!(object("{"), Err(ArgumentError::NotObject(_))));
    }

    #[test]
    fn coerces_values_by_the_schema_type_and_keeps_strings_as_text() {
        let schema = schema(json!({"properties": {
            "n": {"type": "integer"},
            "f": {"type": "boolean"},
            "list": {"type": "array"},
            "s": {"type": "string"},
            "either": {"type": ["string", "number"]},
            "maybe": {"type": ["integer", "null"]},
        }}));
        let pairs: Vec<Pair> = [
            ("n", "12"),
            ("f", "true"),
            ("list", "[1,2]"),
            ("s", "123"),
            ("either", "5"),
            ("maybe", "7"),
            ("unknown", "1"),
            ("bad", "x"),
        ]
        .iter()
        .map(|(k, v)| text(k, v))
        .collect();
        assert_eq!(
            Value::Object(coerce(&Resolved::Pairs(pairs), &schema)),
            json!({"n": 12, "f": true, "list": [1, 2], "s": "123", "either": "5",
                   "maybe": 7, "unknown": "1", "bad": "x"})
        );
    }

    #[test]
    fn finds_types_in_any_of_one_of_local_references_and_enums() {
        let schema = schema(json!({
            "properties": {
                "optional": {"anyOf": [{"type": "integer"}, {"type": "null"}]},
                "either": {"oneOf": [{"type": "boolean"}, {"type": "string"}]},
                "nested": {"$ref": "#/$defs/Filter"},
                "loop": {"$ref": "#/properties/loop"},
                "level": {"enum": [1, 2]},
                "fixed": {"const": 2.5}
            },
            "$defs": {"Filter": {"type": "object"}}
        }));
        let types_of = |key: &str| -> Vec<&str> {
            types(&schema, &schema["properties"][key])
                .into_iter()
                .collect()
        };
        assert_eq!(types_of("optional"), ["integer", "null"]);
        assert_eq!(types_of("either"), ["boolean", "string"]);
        assert_eq!(types_of("nested"), ["object"]);
        assert_eq!(types_of("loop"), Vec::<&str>::new());
        assert_eq!(types_of("level"), ["integer"]);
        assert_eq!(types_of("fixed"), ["number"]);
        let pairs: Vec<Pair> = [
            ("optional", "5"),
            ("either", "true"),
            ("nested", "{}"),
            ("level", "2"),
        ]
        .iter()
        .map(|(k, v)| text(k, v))
        .collect();
        assert_eq!(
            Value::Object(coerce(&Resolved::Pairs(pairs), &schema)),
            json!({"optional": 5, "either": "true", "nested": {}, "level": 2})
        );
    }

    #[test]
    fn wide_tree_of_references_resolves_each_reference_one_time() {
        let branches: Vec<Value> = (0..1000).map(|_| json!({"$ref": "#/$defs/A"})).collect();
        let schema = schema(json!({
            "properties": {"a": {"$ref": "#/$defs/A"}},
            "$defs": {"A": {"anyOf": branches}}
        }));
        assert!(types(&schema, &schema["properties"]["a"]).is_empty());
    }

    #[test]
    fn keeps_value_that_is_not_json_as_text_for_the_schema_check() {
        let schema = schema(json!({"properties": {"n": {"type": "integer"}}}));
        let pairs = Resolved::Pairs(vec![text("n", "twelve")]);
        assert_eq!(
            Value::Object(coerce(&pairs, &schema)),
            json!({"n": "twelve"})
        );
    }
}
