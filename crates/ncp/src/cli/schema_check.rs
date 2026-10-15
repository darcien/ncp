//! The check of tool arguments against the input schema, before the call.
//!
//! The check runs with no network and no file access. Thus a `$ref` to
//! another document does not resolve, and the schema is not usable.

use super::outcome::chain_text;
use rmcp::model::JsonObject;
use serde_json::Value;

/// One place where the arguments do not match the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// A JSON pointer to the argument, empty for the whole object.
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Valid,
    Invalid(Vec<Problem>),
    /// The server gave a schema that `ncp` cannot use. The reason is in the
    /// text.
    SchemaNotUsable(String),
}

#[must_use]
pub fn check(schema: &JsonObject, arguments: &JsonObject) -> Outcome {
    let validator = match jsonschema::validator_for(&Value::Object(schema.clone())) {
        Ok(validator) => validator,
        Err(error) => return Outcome::SchemaNotUsable(chain_text(&error)),
    };
    let instance = Value::Object(arguments.clone());
    let problems: Vec<Problem> = validator
        .iter_errors(&instance)
        .map(|error| Problem {
            path: error.instance_path().to_string(),
            message: chain_text(&error),
        })
        .collect();
    if problems.is_empty() {
        Outcome::Valid
    } else {
        Outcome::Invalid(problems)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object(value: Value) -> JsonObject {
        match value {
            Value::Object(map) => map,
            _ => JsonObject::new(),
        }
    }

    fn schema() -> JsonObject {
        object(json!({
            "type": "object",
            "properties": {"n": {"type": "integer"}, "s": {"type": "string"}},
            "required": ["s"]
        }))
    }

    #[test]
    fn accepts_arguments_that_match_the_schema() {
        assert_eq!(
            check(&schema(), &object(json!({"n": 1, "s": "x"}))),
            Outcome::Valid
        );
    }

    #[test]
    fn names_the_path_of_each_problem() {
        let Outcome::Invalid(problems) = check(&schema(), &object(json!({"n": "x"}))) else {
            panic!("not invalid")
        };
        let paths: Vec<&str> = problems.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths.len(), 2, "{problems:?}");
        assert!(paths.contains(&"/n"));
        assert!(paths.contains(&""));
    }

    #[test]
    fn reports_schema_with_a_reference_to_another_document_as_not_usable() {
        let schema = object(json!({"$ref": "https://example.com/schema.json"}));
        assert!(matches!(
            check(&schema, &JsonObject::new()),
            Outcome::SchemaNotUsable(_)
        ));
    }
}
