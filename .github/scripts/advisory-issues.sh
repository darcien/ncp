#!/usr/bin/env bash
# Opens one issue for each new advisory ID of the locked crates. A person
# closes the issue. While it is open, the script skips its ID.
#
# The run fails for each other error, for example a yanked crate, an unused
# entry in `ignore`, or a check that did not run.
#
# Needs GH_TOKEN and GITHUB_REPOSITORY.
set -euo pipefail

report="$(mktemp)"
findings="$(mktemp)"

# Prints one JSON line for each advisory ID: the ID, the title and the body.
list_findings() {
  jq -cs -f "$(dirname "$0")/advisory-issues.jq" "$report"
}

# The author filter matches the issues of this script only with the login
# `github-actions[bot]`.
list_open_titles() {
  gh issue list --repo "$GITHUB_REPOSITORY" --author 'github-actions[bot]' \
    --state open --limit 1000 --json title --jq '.[].title'
}

# True when there is at least one error and each error is an advisory. A
# check that did not run gives zero errors.
each_error_is_an_advisory() {
  jq -se '
    map(select(.type == "diagnostic" and .fields.severity == "error"))
    | length > 0 and all(.fields.advisory.id)' "$report" > /dev/null
}

status=0
cargo deny --locked --format json check advisories 2> "$report" || status=$?
cat "$report"
[ "$status" = 0 ] && exit 0

# Parse the report before the first call to GitHub, so that a bad report
# stops the run before it changes an issue.
list_findings > "$findings"
open="$(list_open_titles)"
while IFS= read -r finding; do
  id="$(jq -r .id <<< "$finding")"
  grep -q "^$id:" <<< "$open" && continue
  gh issue create --repo "$GITHUB_REPOSITORY" \
    --title "$(jq -r .title <<< "$finding")" --body "$(jq -r .body <<< "$finding")"
done < "$findings"

each_error_is_an_advisory
