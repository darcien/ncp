//! Values in the text of a message.

/// A value in backticks (rule 14 of `docs/design/output-messages.md`). A
/// value with a backtick or a control character shows in double quotes,
/// with the escapes of Rust `Debug`. Thus the backticks of a line stay in
/// pairs, and the value cannot start a new line.
#[must_use]
pub fn code(value: &str) -> String {
    // An empty pair of backticks shows nothing, so an empty value is "".
    if value.is_empty() || value.contains('`') || value.chars().any(char::is_control) {
        format!("{value:?}")
    } else {
        format!("`{value}`")
    }
}

/// The texts of an error and its sources, outer first.
pub fn source_texts(error: &dyn std::error::Error) -> impl Iterator<Item = String> {
    std::iter::once(error.to_string()).chain(
        std::iter::successors(error.source(), |error| error.source()).map(ToString::to_string),
    )
}

/// Transport and library errors put the inner text into the outer text. A
/// cause shows only when the last text that shows does not hold it (rule 7 of
/// `docs/design/output-messages.md`).
pub fn kept_causes(
    message: &str,
    causes: Vec<String>,
    new: impl IntoIterator<Item = String>,
) -> Box<[String]> {
    new.into_iter()
        .fold(causes, |mut causes, text| {
            let last = causes.last().map_or(message, String::as_str);
            if !last.contains(&text) {
                causes.push(text);
            }
            causes
        })
        .into_boxed_slice()
}

/// The text of an error, then the texts of its source chain that do not
/// repeat (rule 7 of `docs/design/output-messages.md`).
#[must_use]
pub fn chain(error: &dyn std::error::Error) -> Vec<String> {
    let mut texts = source_texts(error);
    let first = texts.next().unwrap_or_default();
    let rest = kept_causes(&first, Vec::new(), texts);
    std::iter::once(first).chain(rest.into_vec()).collect()
}

/// An error and its source chain on one line, joined with `: `. For a
/// place that has room for one line only, for example a table cell.
#[must_use]
pub fn chain_text(error: &dyn std::error::Error) -> String {
    chain(error).join(": ")
}

/// A path in backticks (rule 14 of `docs/design/output-messages.md`).
#[must_use]
pub fn path_code(path: &std::path::Path) -> String {
    code(&path.display().to_string())
}

/// The text with a capital first letter.
#[must_use]
pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_in_backticks_with_a_backtick_or_a_new_line_shows_in_quotes() {
        assert_eq!(code("/home/u/mcp.json"), "`/home/u/mcp.json`");
        assert_eq!(code("a`b"), "\"a`b\"");
        assert_eq!(code("a\nb"), "\"a\\nb\"");
        assert_eq!(code(""), "\"\"");
    }
}
