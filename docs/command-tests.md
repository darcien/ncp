# Command tests

This document gives the rules for the tests of command output.
[CONTRIBUTING.md](../CONTRIBUTING.md), "Tests", gives the rules for all
tests.

## Snapshots

- A new test of command output snapshots the full transcript: the exit
  code, stdout and stderr. Use `common::ncp_snapshot!` with
  `home.filters()` and `home.ncp(args)`. Text is the default.
  JSON tests pass `--output-format json`.
- For each command, keep a text snapshot of its main success and main
  error. A command with no error path snapshots its warning instead.
- When a change edits a current test of command output, move that test
  to snapshots. For its main success and main error, snapshot both output
  formats. An explicit change can instead move one command group.
  Other current tests keep their part checks.
- Shared helpers and loops can cross command groups. When you move one to
  snapshots, keep each case that it covers.
- These rules apply to tests that `mise run test` runs. Ignored tests
  keep their part checks.
- Runs with open stdin, signals, concurrent stream reads or output streamed
  to a file keep their part checks.
- A test that changes a selected file snapshots the whole file with
  `insta::assert_snapshot!` inside
  `insta::with_settings!({filters => home.filters()}, { ... })`.
  It does not inventory every changed file. Keep file-mode assertions.
- The macro returns the same run for raw assertions. Do not run the
  command again to obtain its output. Keep `run.error()` and
  `run.warnings()`, which check the rules of JSON diagnostics.
- Keep raw assertions for secrets, exact bytes, final newlines and
  line-end spaces. Snapshots cannot protect these properties alone.
- Keep assertions for requests, file modes and other data outside the
  transcript. If a filtered value is the subject of the test, check its
  unfiltered value too.
- Output with variable order or line count keeps its required property
  checks. Examples include server logs, progress and origin-sorted values.
- Each inline assertion has its own call site. Do not run it repeatedly
  through a loop, closure or helper. Use `insta::allow_duplicates!` only
  when all runs must produce the same text.

## Filters

- Per-test filters can precede or follow `home.filters()`. Later filters
  see earlier replacements. With multiple local servers, name all but
  one before the standard loopback filter.
- Standard filters cover paths, exact build executables, local ports,
  measured times in traces and the cache, and outgoing client-version
  metadata. `ncp_snapshot!` itself replaces each time of a `--timings`
  line, so the number of responses stays visible. It also filters CLI
  version results when the `ncp` binary runs with `--version` or `-V`
  before `--`. Other changing values need per-test filters, also when
  repeated runs produce the same value.
- Table padding can depend on a path. Use a per-test filter that keeps
  the indentation.
- Missing-file and connection-refused normalization is opt-in. Do not
  filter arbitrary OS codes, fixed time limits or server identities.
- A controlled server failure is good when it tests the intended
  behavior. It does not replace the connection-refused case.
- Fixture payloads avoid accidental filter matches unless the test checks
  filter scope.
- Inline snapshots retain intentional whitespace. The trailing-whitespace
  step in `hk.pkl` lists narrow source-file exclusions.
  Keep raw assertions for whitespace and final newlines.

## Update snapshots

- To update snapshots, run `cargo insta test --workspace --accept`.
  Review every changed transcript. `cargo insta review` is interactive.
  An agent uses `cargo insta accept` for pending snapshots.
  The hook and CI never accept snapshots automatically.
- Host runs write ignored `*.pending-snap` files beside test sources.
  Linux runs set `INSTA_PENDING_DIR=/tmp/insta-pending`, so pending files
  stay inside the container, also with stale host files.
  Container pending files disappear after the run.
- If a container snapshot fails, correct its filter or update it on the
  host. Workspace discovery uses offline `cargo metadata` in the container.
