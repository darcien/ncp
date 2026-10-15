//! Values in the text of a message.

/// A value in backticks (rule 14 of `docs/design/output-messages.md`). A
/// value with a backtick or a control character shows in double quotes,
/// with the escapes of Rust `Debug`. Thus the backticks of a line stay in
/// pairs, and the value cannot start a new line.
#[must_use]
pub fn code(value: &str) -> String {
    if value.contains('`') || value.chars().any(char::is_control) {
        format!("{value:?}")
    } else {
        format!("`{value}`")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_in_backticks_with_a_backtick_or_a_new_line_shows_in_quotes() {
        assert_eq!(code("/home/u/mcp.json"), "`/home/u/mcp.json`");
        assert_eq!(code("a`b"), "\"a`b\"");
        assert_eq!(code("a\nb"), "\"a\\nb\"");
    }
}
