//! A check for the unit tests of the error types.

/// Fails when a Display text of `ncp` does not start with a capital letter,
/// a backtick or `ncp`, or when it has a semicolon.
pub fn assert_style(texts: &[String]) {
    for text in texts {
        let first = text.chars().next();
        assert!(
            first.is_some_and(|c| c.is_uppercase() || c == '`') || text.starts_with("ncp"),
            "{text:?} does not start with a capital letter, a backtick or ncp"
        );
        assert!(!text.contains(';'), "{text:?} has a semicolon");
    }
}
