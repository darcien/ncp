//! `${VAR}` expansion in configuration values.
//!
//! Only `${VAR}` is supported. Other agent tools use other forms, and some
//! run commands. A value with an unknown form keeps it as text, so the
//! configuration never runs code.

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

/// Replaces each `${VAR}` in `text` with its value from `env`.
///
/// # Errors
///
/// Returns an error for an unset variable, a `${` with no `}`, or a name that
/// is not a variable name.
pub fn expand(text: &str, env: &Env) -> Result<String, ExpandError> {
    match text.split_once("${") {
        None => Ok(text.to_string()),
        Some((before, rest)) => {
            let (name, after) = rest.split_once('}').ok_or(ExpandError::Unterminated)?;
            let value = lookup(name, env)?;
            Ok(format!("{before}{value}{}", expand(after, env)?))
        }
    }
}

fn lookup<'a>(name: &str, env: &'a Env) -> Result<&'a str, ExpandError> {
    if !is_variable_name(name) {
        return Err(ExpandError::BadName(name.to_string()));
    }
    env.get(name)
        .map(String::as_str)
        .ok_or_else(|| ExpandError::Unset(name.to_string()))
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
            expand("${env:HOME}", &env()),
            Err(ExpandError::BadName("env:HOME".into()))
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
