//! `${VAR}` expansion in configuration values.
//!
//! `ncp` supports only `${VAR}` and `${env:VAR}`. Cursor, Cline and Windsurf
//! write the second form, so their files load with no change. Other forms can
//! run commands or ask for input. They are errors, so the configuration never
//! runs code.

use std::collections::HashMap;

/// The environment that the caller gives for expansion.
pub type Env = HashMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExpandError {
    #[error("variable {0} is not set")]
    Unset(String),
    #[error("\"${{\" has no closing \"}}\"")]
    Unterminated,
    #[error("{0:?} is not a variable name")]
    BadName(String),
}

/// Replaces each `${VAR}` and `${env:VAR}` in `text` with its value from
/// `env`.
///
/// # Errors
///
/// Returns an error for an unset variable, a `${` with no `}`, or a name that
/// is not a variable name.
pub fn expand(text: &str, env: &Env) -> Result<String, ExpandError> {
    parts(text)?
        .into_iter()
        .map(|part| match part {
            Part::Text(text) => Ok(text),
            Part::Variable(name) => env
                .get(name)
                .map(String::as_str)
                .ok_or_else(|| ExpandError::Unset(name.to_string())),
        })
        .collect()
}

/// The names of the variables in `text`, in order. A text with a syntax
/// error gives the names before the error.
#[must_use]
pub fn names(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((_, after)) = rest.split_once("${") {
        let Some((reference, next)) = after.split_once('}') else {
            break;
        };
        let name = reference.strip_prefix("env:").unwrap_or(reference);
        if is_variable_name(name) {
            found.push(name);
        }
        rest = next;
    }
    found
}

enum Part<'a> {
    Text(&'a str),
    Variable(&'a str),
}

fn parts(text: &str) -> Result<Vec<Part<'_>>, ExpandError> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("${") {
        let (reference, next) = after.split_once('}').ok_or(ExpandError::Unterminated)?;
        let name = reference.strip_prefix("env:").unwrap_or(reference);
        if !is_variable_name(name) {
            return Err(ExpandError::BadName(reference.to_string()));
        }
        found.push(Part::Text(before));
        found.push(Part::Variable(name));
        rest = next;
    }
    found.push(Part::Text(rest));
    Ok(found)
}

/// A configuration value that can hold `${NAME}`, kept as the file writes
/// it. The value of a name can come from a credential store, so the caller
/// expands it only when it starts the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template(String);

impl Template {
    /// # Errors
    ///
    /// Returns an error for a `${` with no `}`, or a name that is not a
    /// variable name.
    pub fn parse(text: &str) -> Result<Self, ExpandError> {
        parts(text).map(|_| Self(text.to_string()))
    }

    /// The names of the variables, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        names(&self.0)
    }

    /// True when the value holds a variable, so the expanded value can be a
    /// secret.
    #[must_use]
    pub fn has_variables(&self) -> bool {
        !self.names().is_empty()
    }

    /// # Errors
    ///
    /// Returns an error for a name that `values` does not have.
    pub fn expand(&self, values: &Env) -> Result<String, ExpandError> {
        expand(&self.0, values)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A POSIX environment variable name: a letter or `_`, then letters, digits
/// or `_`.
#[must_use]
pub fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env::from([
            ("HOME".to_string(), "/home/u".to_string()),
            ("TOKEN_2".to_string(), "t".to_string()),
            ("EMPTY".to_string(), String::new()),
        ])
    }

    #[test]
    fn expands_each_variable_in_text() {
        assert_eq!(
            expand("${HOME}/x-${TOKEN_2}-${HOME}", &env()).unwrap(),
            "/home/u/x-t-/home/u"
        );
    }

    #[test]
    fn expands_env_prefix_form_as_the_same_variable() {
        assert_eq!(
            expand("${env:HOME}/${HOME}", &env()).unwrap(),
            "/home/u//home/u"
        );
        assert_eq!(
            expand("${env:NOPE}", &env()),
            Err(ExpandError::Unset("NOPE".into()))
        );
    }

    #[test]
    fn template_keeps_the_text_and_names_its_variables() {
        let template = Template::parse("Bearer ${TOKEN}-${env:ORG}").unwrap();
        assert_eq!(template.names(), ["TOKEN", "ORG"]);
        assert!(template.has_variables());
        assert_eq!(template.as_str(), "Bearer ${TOKEN}-${env:ORG}");
        let values = Env::from([
            ("TOKEN".to_string(), "t".to_string()),
            ("ORG".to_string(), "o".to_string()),
        ]);
        assert_eq!(template.expand(&values).unwrap(), "Bearer t-o");
        assert!(!Template::parse("plain").unwrap().has_variables());
        assert_eq!(Template::parse("${A"), Err(ExpandError::Unterminated));
    }

    #[test]
    fn names_skip_bad_forms_and_stop_at_an_open_brace() {
        assert_eq!(names("${A} ${b-c} ${env:B} ${C"), ["A", "B"]);
    }

    #[test]
    fn keeps_text_without_variables() {
        assert_eq!(
            expand("plain $HOME $ {x}", &env()).unwrap(),
            "plain $HOME $ {x}"
        );
    }

    #[test]
    fn expands_set_but_empty_variable_to_empty_text() {
        assert_eq!(expand("a${EMPTY}b", &env()).unwrap(), "ab");
    }

    #[test]
    fn does_not_expand_text_that_comes_from_a_value() {
        let env = Env::from([("A".to_string(), "${B}".to_string())]);
        assert_eq!(expand("${A}", &env).unwrap(), "${B}");
    }

    #[test]
    fn rejects_unset_variable() {
        assert_eq!(
            expand("${NOPE}", &env()),
            Err(ExpandError::Unset("NOPE".into()))
        );
    }

    #[test]
    fn rejects_other_variable_forms_inside_braces() {
        assert_eq!(
            expand("${HOME:-x}", &env()),
            Err(ExpandError::BadName("HOME:-x".into()))
        );
        assert_eq!(
            expand("${env:}", &env()),
            Err(ExpandError::BadName("env:".into()))
        );
        assert_eq!(
            expand("${input:token}", &env()),
            Err(ExpandError::BadName("input:token".into()))
        );
        assert_eq!(
            expand("${}", &env()),
            Err(ExpandError::BadName(String::new()))
        );
    }

    #[test]
    fn rejects_unterminated_variable() {
        assert_eq!(expand("a${HOME", &env()), Err(ExpandError::Unterminated));
    }
}
