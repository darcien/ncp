//! The near name for an unknown name: "did you mean ...?".

/// The edit distance (Levenshtein) of two names, in characters.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let first: Vec<usize> = (0..=b.len()).collect();
    a.chars()
        .enumerate()
        .fold(first, |previous, (i, a_char)| {
            let mut row = vec![i + 1];
            for (j, b_char) in b.iter().enumerate() {
                let replace = previous[j] + usize::from(a_char != *b_char);
                row.push(replace.min(previous[j + 1] + 1).min(row[j] + 1));
            }
            row
        })
        .last()
        .copied()
        .unwrap_or_default()
}

/// The known name that is nearest to the unknown name, when it is near
/// enough: at most one edit for each three characters, and at least one.
/// A case difference only is always near.
#[must_use]
pub fn nearest<'a>(unknown: &str, known: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (unknown.chars().count() / 3).max(1);
    known
        .into_iter()
        .map(|name| {
            let edits = if name.eq_ignore_ascii_case(unknown) {
                0
            } else {
                distance(unknown, name)
            };
            (edits, name)
        })
        .filter(|(edits, _)| *edits <= limit)
        .min_by_key(|(edits, name)| (*edits, *name))
        .map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_inserts_removals_and_changes() {
        assert_eq!(distance("search_issue", "search_issues"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("same", "same"), 0);
    }

    #[test]
    fn nearest_name_is_near_enough_or_none() {
        let tools = ["search_issues", "create_issue", "list_repos"];
        assert_eq!(nearest("search_issue", tools), Some("search_issues"));
        assert_eq!(nearest("Create_Issue", tools), Some("create_issue"));
        assert_eq!(nearest("deploy", tools), None);
        assert_eq!(nearest("gh", ["gl", "github"]), Some("gl"));
    }
}
