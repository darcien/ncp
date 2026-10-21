//! Style checks for the unit tests: the text of the error types, and the
//! words that `STYLE.md` forbids in the output text of the source files.
//!
//! The word test scans the source text of `src/`. It checks:
//!
//! - each string literal with a space (a plain string or a raw string);
//! - the doc comments of `cli/args.rs`, because clap shows them as help.
//!
//! It does not check:
//!
//! - a literal without a space, because it is a key or a name, not a
//!   sentence. A one-word output text is not checked.
//! - a text that the code computes, for example a value from `format!`
//!   arguments, `concat!`, `include_str!` or a constant of a dependency.
//!   A phrase that is split between two literals is not found.
//! - a byte string or a C string, because it is not output text.
//! - an item that is only for tests. The scan skips an item that has
//!   `#[cfg(test)]` or `#[cfg(all(test, ...))]`, and a whole file that has
//!   `#![cfg(test)]` before its first item. The skip ends at the first `;`,
//!   at the first `,` outside brackets and angle brackets, at the end of the
//!   first brace pair, or before a closing bracket that has no opening
//!   bracket in the item. A `where` clause with a comma, or a comma in a
//!   closure, can end the skip too early. A `<` operator can end it too
//!   late, so the test can miss text after it. The scan does not
//!   know `cfg_attr`, `any` or `not`, and it ignores an inner `#![cfg(test)]`
//!   that is not at the top of a file.
//! - text that a macro makes, because the scan does not expand macros.

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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// A piece of output text and where it comes from.
    struct Text {
        path: String,
        line: usize,
        body: String,
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Takes the forbidden phrases from the table under `## Words`.
    fn forbidden_phrases(style_guide: &str) -> Result<Vec<String>, String> {
        let section: Vec<&str> = style_guide
            .lines()
            .skip_while(|line| line.trim_end() != "## Words")
            .skip(1)
            .take_while(|line| !line.starts_with("## "))
            .collect();
        if section.is_empty() {
            return Err("STYLE.md has no `## Words` section, or the section is empty".into());
        }
        // The first two rows are the header and the separator.
        let phrases = section
            .iter()
            .filter(|line| line.trim_start().starts_with('|'))
            .skip(2)
            .map(|row| first_cell_phrase(row))
            .collect::<Result<Vec<_>, _>>()?;
        if phrases.is_empty() {
            return Err("the `## Words` section of STYLE.md has no table row".into());
        }
        Ok(phrases)
    }

    /// Takes the text between the backticks of the first cell of a row.
    fn first_cell_phrase(row: &str) -> Result<String, String> {
        let cell = row.trim_start().trim_start_matches('|');
        let cell = cell.split('|').next().unwrap_or_default().trim();
        let inner = cell
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
            .filter(|inner| !inner.is_empty());
        inner
            .map(str::to_owned)
            .ok_or_else(|| format!("the first cell of this row has no backtick text: {row:?}"))
    }

    /// Skips from `start` (a quote) to the end of a normal string. Returns the
    /// value and the index after the closing quote.
    fn normal_string(chars: &[char], start: usize) -> (String, usize) {
        let mut value = String::new();
        let mut i = start + 1;
        while i < chars.len() && chars[i] != '"' {
            if chars[i] == '\\' && i + 1 < chars.len() {
                i += 1;
                match chars[i] {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    // A backslash before a line break removes the break and
                    // the white space after it.
                    '\n' => {
                        while i + 1 < chars.len() && chars[i + 1].is_whitespace() {
                            i += 1;
                        }
                    }
                    other => value.push(other),
                }
            } else {
                value.push(chars[i]);
            }
            i += 1;
        }
        (value, i + 1)
    }

    /// If a raw string starts at `start` (the `r`), returns the value and the
    /// index after the end.
    fn raw_string(chars: &[char], start: usize) -> Option<(String, usize)> {
        let hashes = chars[start + 1..].iter().take_while(|&&c| c == '#').count();
        if chars.get(start + 1 + hashes) != Some(&'"') {
            return None;
        }
        let body = start + 2 + hashes;
        let closing: Vec<char> = std::iter::once('"')
            .chain(std::iter::repeat_n('#', hashes))
            .collect();
        let end = (body..chars.len()).find(|&i| chars[i..].starts_with(&closing))?;
        Some((chars[body..end].iter().collect(), end + closing.len()))
    }

    /// The index after a char literal or a lifetime quote that starts at
    /// `start`.
    fn after_quote(chars: &[char], start: usize) -> usize {
        match chars.get(start + 1) {
            // An escaped char: skip the escaped char, then find the closing quote.
            Some('\\') => (start + 3..chars.len())
                .find(|&i| chars[i] == '\'')
                .map_or(chars.len(), |i| i + 1),
            // A plain char literal, for example `'"'`.
            _ if chars.get(start + 2) == Some(&'\'') => start + 3,
            // A lifetime has no closing quote.
            _ => start + 1,
        }
    }

    /// A piece of text of a source file: its line, its value, and whether it
    /// is a doc comment (`///`) and not a string literal.
    type Piece = (usize, String, bool);

    /// Whether the literal that starts at `start` has a `b` or `c` prefix, as in
    /// `b"..."`, `c"..."`, `br"..."` or `cr#"..."#`. Such a literal is not
    /// output text.
    fn has_byte_or_c_prefix(chars: &[char], start: usize) -> bool {
        start > 0
            && matches!(chars[start - 1], 'b' | 'c')
            && (start < 2 || !is_word_char(chars[start - 2]))
    }

    /// Reads one token that can hide text: a comment, a string or a char. Returns
    /// the index after it, and the value when it is a string literal that is
    /// output text (not a byte string or a C string). Any other char is one
    /// step.
    fn step(chars: &[char], i: usize) -> (usize, Option<String>) {
        let next = chars.get(i + 1).copied();
        match chars[i] {
            '/' if next == Some('/') => (
                (i..chars.len())
                    .find(|&j| chars[j] == '\n')
                    .unwrap_or(chars.len()),
                None,
            ),
            '/' if next == Some('*') => (
                (i + 2..chars.len())
                    .find(|&j| chars[j..].starts_with(&['*', '/']))
                    .map_or(chars.len(), |j| j + 2),
                None,
            ),
            '"' => {
                let (value, end) = normal_string(chars, i);
                (end, (!has_byte_or_c_prefix(chars, i)).then_some(value))
            }
            'r' if i == 0 || !is_word_char(chars[i - 1]) || has_byte_or_c_prefix(chars, i) => {
                match raw_string(chars, i) {
                    Some((value, end)) => (end, (!has_byte_or_c_prefix(chars, i)).then_some(value)),
                    None => (i + 1, None),
                }
            }
            '\'' => (after_quote(chars, i), None),
            _ => (i + 1, None),
        }
    }

    /// The index after the item that starts at `from`. The item ends after a
    /// `;` or a `,` outside brackets, after the closing brace of the first
    /// brace pair, or before a closing bracket that the item did not open (the
    /// end of the enclosing block, which the caller must keep). A `,` inside
    /// angle brackets, for example `fn f<A, B>()`, does not end the item.
    fn item_end(chars: &[char], from: usize) -> usize {
        let (mut i, mut depth, mut angle) = (from, 0_usize, 0_usize);
        while i < chars.len() {
            match chars[i] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => return i,
                ')' | ']' => depth -= 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                '<' => angle += 1,
                // `->` and `=>` are arrows, not closing angle brackets.
                '>' if i > 0 && !matches!(chars[i - 1], '-' | '=') => {
                    angle = angle.saturating_sub(1);
                }
                ';' if depth == 0 => return i + 1,
                ',' if depth == 0 && angle == 0 => return i + 1,
                _ => {}
            }
            i = step(chars, i).0.max(i + 1);
        }
        chars.len()
    }

    /// If an attribute starts at `hash` (the `#`), returns whether it is inner
    /// (`#!`), its text without white space, and the index after it.
    fn attribute(chars: &[char], hash: usize) -> Option<(bool, String, usize)> {
        let inner = chars.get(hash + 1) == Some(&'!');
        let open = hash + 1 + usize::from(inner);
        if chars.get(open) != Some(&'[') {
            return None;
        }
        let (mut i, mut depth) = (open, 0_usize);
        while i < chars.len() {
            match chars[i] {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        let text = chars[open + 1..i]
                            .iter()
                            .filter(|c| !c.is_whitespace())
                            .collect();
                        return Some((inner, text, i + 1));
                    }
                }
                _ => {}
            }
            i = step(chars, i).0.max(i + 1);
        }
        None
    }

    /// Whether a cfg predicate (without white space) is `test`, or an `all(...)`
    /// that has `test` as one of its arguments.
    fn is_test_predicate(predicate: &str) -> bool {
        predicate == "test"
            || predicate
                .strip_prefix("all(")
                .and_then(|rest| rest.strip_suffix(')'))
                .is_some_and(|args| split_arguments(args).into_iter().any(is_test_predicate))
    }

    /// Splits the text of an argument list at the commas outside parentheses.
    fn split_arguments(args: &str) -> Vec<&str> {
        let (mut parts, mut depth, mut start) = (Vec::new(), 0_usize, 0);
        for (at, c) in args.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(&args[start..at]);
                    start = at + 1;
                }
                _ => {}
            }
        }
        parts.push(&args[start..]);
        parts
    }

    /// Whether the text of an attribute (without white space) is `cfg(test)`
    /// or `cfg(all(test, ...))`.
    fn is_test_cfg(attribute: &str) -> bool {
        attribute
            .strip_prefix("cfg(")
            .and_then(|rest| rest.strip_suffix(')'))
            .is_some_and(is_test_predicate)
    }

    /// Whether an inner attribute `#![cfg(test)]` comes before the first item
    /// of the file. Only white space, comments and other inner attributes can
    /// come before it.
    fn is_test_file(chars: &[char]) -> bool {
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_whitespace() {
                i += 1;
            } else if chars[i] == '/' && matches!(chars.get(i + 1), Some('/' | '*')) {
                i = step(chars, i).0.max(i + 1);
            } else {
                return match attribute(chars, i) {
                    Some((true, text, end)) => is_test_cfg(&text) || is_test_file(&chars[end..]),
                    _ => false,
                };
            }
        }
        false
    }

    /// Takes the string literals and the doc comment lines of Rust source with
    /// the line where each starts. It skips each item with a test-only `cfg`
    /// attribute, and a whole file with an inner one, because test code is not
    /// output. A small scanner is enough, because a regex crate is a new
    /// dependency. The limits are in the module documentation.
    fn scan(source: &str) -> Vec<Piece> {
        let chars: Vec<char> = source.chars().collect();
        if is_test_file(&chars) {
            return Vec::new();
        }
        let mut found = Vec::new();
        let (mut i, mut line) = (0, 1);
        while i < chars.len() {
            let test_attribute = (chars[i] == '#')
                .then(|| attribute(&chars, i))
                .flatten()
                .filter(|(inner, text, _)| !inner && is_test_cfg(text));
            let after = if let Some((_, _, end)) = test_attribute {
                item_end(&chars, end)
            } else if chars[i..].starts_with(&['/', '/', '/']) && chars.get(i + 3) != Some(&'/') {
                let end = step(&chars, i).0;
                found.push((line, chars[i + 3..end].iter().collect(), true));
                end
            } else {
                let (end, literal) = step(&chars, i);
                if let Some(value) = literal {
                    found.push((line, value, false));
                }
                end
            };
            // Count the line breaks that a literal, a comment or a skipped item
            // contains.
            line += chars[i..after.min(chars.len())]
                .iter()
                .filter(|&&ch| ch == '\n')
                .count();
            i = after;
        }
        found
    }

    /// Joins the doc comment lines that follow each other into one text, with
    /// the line of the first. A phrase can span two lines.
    fn join_doc_lines(pieces: Vec<Piece>) -> Vec<(usize, String)> {
        let mut joined: Vec<(usize, String)> = Vec::new();
        let mut last_doc_line = None;
        for (line, text, is_doc) in pieces {
            match joined.last_mut() {
                Some((_, body)) if is_doc && last_doc_line.is_some_and(|l| l + 1 == line) => {
                    body.push(' ');
                    body.push_str(&text);
                }
                _ => joined.push((line, text)),
            }
            last_doc_line = is_doc.then_some(line);
        }
        joined
    }

    /// The phrases that the text contains, as whole words and with any case.
    fn hits<'a>(text: &str, phrases: &'a [String]) -> Vec<&'a str> {
        // A line break or a double space must not hide a phrase.
        let lower = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        phrases
            .iter()
            .filter(|phrase| {
                let needle = phrase.to_lowercase();
                lower.match_indices(&needle).any(|(at, _)| {
                    let before = lower[..at].chars().next_back();
                    let after = lower[at + needle.len()..].chars().next();
                    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
                })
            })
            .map(String::as_str)
            .collect()
    }

    /// The text that a person can read in the output of a source file: the
    /// literals with a space, and for the argument file also the help lines.
    /// A literal without a space is a key or a name, for example "invalid".
    fn output_texts(path: &str, source: &str) -> Vec<Text> {
        let is_args = path.ends_with("cli/args.rs");
        let pieces: Vec<Piece> = scan(source)
            .into_iter()
            // clap shows the doc comments of the arguments as help.
            .filter(|(_, text, is_doc)| if *is_doc { is_args } else { text.contains(' ') })
            .collect();
        join_doc_lines(pieces)
            .into_iter()
            .map(|(line, body)| Text {
                path: path.to_owned(),
                line,
                body,
            })
            .collect()
    }

    fn rust_files(dir: &Path) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        entries
            .into_iter()
            .flat_map(|path| {
                if path.is_dir() {
                    rust_files(&path)
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    vec![path]
                } else {
                    Vec::new()
                }
            })
            .collect()
    }

    #[test]
    fn output_text_uses_no_word_that_the_style_guide_forbids() {
        // STYLE.md is at the workspace root, so this test runs only in the
        // workspace, not in a copy of the crate alone.
        let style_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../STYLE.md");
        let style_guide = std::fs::read_to_string(style_path)
            .unwrap_or_else(|error| panic!("cannot read {style_path}: {error}"));
        let phrases = forbidden_phrases(&style_guide).unwrap_or_else(|message| panic!("{message}"));

        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let violations: Vec<String> = rust_files(&src)
            .iter()
            .flat_map(|path| {
                let name = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let source = std::fs::read_to_string(path).unwrap();
                output_texts(&name, &source)
            })
            .flat_map(|text| {
                hits(&text.body, &phrases)
                    .into_iter()
                    .map(|phrase| format!("src/{}:{}: {phrase}", text.path, text.line))
                    .collect::<Vec<_>>()
            })
            .collect();

        assert!(
            violations.is_empty(),
            "output text uses words that STYLE.md forbids:\n{}",
            violations.join("\n")
        );
    }

    fn literals(source: &str) -> Vec<(usize, String)> {
        scan(source)
            .into_iter()
            .filter(|(_, _, is_doc)| !is_doc)
            .map(|(line, text, _)| (line, text))
            .collect()
    }

    #[test]
    fn scanner_takes_string_literals_and_skips_the_rest() {
        let source = r##"
let a = "say \"hi\" now"; // "not a literal"
let b = r"raw text";
let c = r#"has "quotes" inside"#;
let d = '"';
let e = '\'';
fn f<'a>(x: &'a str) -> &'a str { "after lifetime" }
/// "doc comment"
//! "inner doc"
"##;
        assert_eq!(
            literals(source),
            vec![
                (2, "say \"hi\" now".to_owned()),
                (3, "raw text".to_owned()),
                (4, "has \"quotes\" inside".to_owned()),
                (7, "after lifetime".to_owned()),
            ]
        );
    }

    #[test]
    fn scanner_scans_code_after_a_test_module() {
        let source = r#"
fn a() -> &'static str { "before one" }
#[cfg(test)]
mod tests {
    fn t() { let _ = "in } test"; let _ = '}'; }
}
fn b() -> &'static str { "after two" }
"#;
        assert_eq!(
            literals(source),
            vec![(2, "before one".to_owned()), (7, "after two".to_owned())]
        );
    }

    #[test]
    fn scanner_skips_a_test_function_and_a_test_use() {
        let source = r#"
#[cfg(test)]
fn t(x: [u8; 2]) -> &'static str { "in test" }
#[cfg(test)]
use std::fmt;
fn b() -> &'static str { "kept text" }
"#;
        assert_eq!(literals(source), vec![(6, "kept text".to_owned())]);
    }

    #[test]
    fn scanner_skips_a_test_item_that_ends_at_a_comma() {
        let source = r#"
enum E {
    #[cfg(test)] Foo(u8, u8),
    Bar,
}
struct S {
    #[cfg(test)]
    a: Vec<(u8, u8)>,
    b: &'static str,
}
fn g(#[cfg(test)] x: u8, y: &str) -> &'static str { "kept arg" }
fn h(v: u8) -> &'static str {
    match v {
        #[cfg(test)]
        0 => "in test",
        _ => "kept arm",
    }
}
#[cfg(test)]
fn t<A, B>(a: A, b: B) { let _ = "in generic test"; }
fn k() -> &'static str { "kept last" }
"#;
        assert_eq!(
            literals(source),
            vec![
                (11, "kept arg".to_owned()),
                (16, "kept arm".to_owned()),
                (21, "kept last".to_owned()),
            ]
        );
    }

    #[test]
    fn scanner_does_not_consume_the_closing_bracket_of_the_enclosing_block() {
        let source = "fn a() { #[cfg(test)] let _ = 1 }\nfn b() -> &'static str { \"kept one\" }\n";
        assert_eq!(literals(source), vec![(2, "kept one".to_owned())]);
        let source = "fn a() -> [u8; 1] { [#[cfg(test)] 1] }\nconst B: &str = \"kept two\";\n";
        assert_eq!(literals(source), vec![(2, "kept two".to_owned())]);
    }

    #[test]
    fn scanner_skips_every_form_of_a_test_cfg() {
        let source = r#"
#[cfg(all(test, unix))]
fn a() { let _ = "in all"; }
#[cfg( test )]
fn b() { let _ = "in spaced"; }
#[cfg ( all ( unix , test ) )]
fn c() { let _ = "in all last"; }
#[cfg(all(unix, all(test)))]
fn d() { let _ = "in nested all"; }
#[cfg(not(test))]
fn e() -> &'static str { "kept not" }
#[cfg(any(test, unix))]
fn f() -> &'static str { "kept any" }
#[cfg(unix)]
fn g() -> &'static str { "kept unix" }
"#;
        assert_eq!(
            literals(source),
            vec![
                (11, "kept not".to_owned()),
                (13, "kept any".to_owned()),
                (15, "kept unix".to_owned()),
            ]
        );
    }

    #[test]
    fn scanner_skips_a_whole_file_with_an_inner_test_cfg() {
        let source =
            "//! Doc.\n\n#![allow(unused)]\n#![cfg(test)]\nfn a() { let _ = \"in test file\"; }\n";
        assert_eq!(literals(source), Vec::new());
        // An inner attribute that is not at the top does not skip the file.
        let source = "fn a() -> &'static str { \"kept\" }\nmod m {\n    #![cfg(test)]\n}\n";
        assert_eq!(literals(source), vec![(1, "kept".to_owned())]);
    }

    #[test]
    fn scanner_skips_byte_strings_and_c_strings_without_taking_them() {
        let source = r##"
let a = b"byte text";
let b = br"raw byte";
let c = br#"raw "byte" hash"#;
let d = c"c text";
let e = cr"raw c";
let f = cr#"raw "c" hash"#;
let g = b'"';
let h = "kept text";
let i = r#"kept raw"#;
let abr = "kept after word";
"##;
        assert_eq!(
            literals(source),
            vec![
                (9, "kept text".to_owned()),
                (10, "kept raw".to_owned()),
                (11, "kept after word".to_owned()),
            ]
        );
    }

    #[test]
    fn doc_comment_lines_join_into_one_text() {
        let source = "/// If you\n/// mean it\nfn a() {}\n\n/// Other\nfn b() {}\n";
        assert_eq!(
            join_doc_lines(scan(source)),
            vec![(1, " If you  mean it".to_owned()), (5, " Other".to_owned())]
        );
        let phrases = vec!["If you mean".to_owned()];
        assert_eq!(
            hits(&join_doc_lines(scan(source))[0].1, &phrases),
            vec!["If you mean"]
        );
    }

    #[test]
    fn matcher_hits_whole_words_with_any_case() {
        let phrases: Vec<String> = ["uv", "e.g.", "please", "If you mean"]
            .map(String::from)
            .to_vec();
        assert_eq!(hits("uvloop and tuv", &phrases), Vec::<&str>::new());
        assert_eq!(hits("Use uv here", &phrases), vec!["uv"]);
        assert_eq!(hits("a tool, e.g. this", &phrases), vec!["e.g."]);
        assert_eq!(hits("PLEASE wait", &phrases), vec!["please"]);
        assert_eq!(hits("if you mean it", &phrases), vec!["If you mean"]);
        assert_eq!(hits("if  you\n\tmean it", &phrases), vec!["If you mean"]);
    }

    #[test]
    fn table_reader_takes_the_first_cell_and_checks_the_section() {
        let guide = "# Style\n\n## Words\n\n| Do not use | Use |\n|---|---|\n| `invalid` | not valid |\n| `e.g.` | for example |\n\n## Next\n\n| `other` | x |\n";
        assert_eq!(
            forbidden_phrases(guide).unwrap(),
            vec!["invalid".to_owned(), "e.g.".to_owned()]
        );
        assert!(forbidden_phrases("# Style\n").is_err());
        assert!(forbidden_phrases("## Words\n\n| a | b |\n|---|---|\n").is_err());
    }
}
