---
state: accepted
supersedes: []
---

# 0006: Behavior tests of the command line

## Background

This RFC proposes inline snapshots of command behavior with [`insta`][insta].
A snapshot holds expected text in the Rust test source. A failed assertion
produces new text for review.

This RFC stays editable until its completed implementation lands on `main`.

The black-box tests run `ncp-cli` with a temporary `HOME` and a clean
environment. They use local servers, `ncp-fixture`, a fake browser,
a fake Keychain and a fake TTY. JSON diagnostic helpers also check style.

The hook runs affected tests on macOS and in a read-only Linux musl
container. Linux source builds can use glibc. Snapshots must work across
these environments, not only on the development host.

### Prior art

The source comparisons use uv 0.13.7 and mise v2026.11.3.
These pinned sources do not describe every installed release.
The order of choice remains the MCP specification, security, ease of use,
uv and mise practice, then the simplest design.

The installed tools reported these versions. The source comparisons above
retain their own pinned versions:

```text
$ uv --version
uv 0.13.8 (Homebrew 2026-11-12 aarch64-apple-darwin)
$ mise --version
2026.11.8 macos-arm64 (2026-11-13)
```

- uv uses [`uv_snapshot!`][uv-snapshot] with filters, a `Command` and
  an inline snapshot. Its [transcript helper][uv-transcript] captures
  piped output and returns the process output.
- uv provides [standard filters][uv-filters] for changing values and
  an opt-in [missing-file helper][uv-os-error].
- mise uses [shell assertions][mise-assert]. Rust tests suit `ncp`
  because they can start fixture servers in the test process.
- The SDK uses fixed identities in its [discovery-client test][sdk-client]
  and [HTTP discovery test][sdk-http]. The client test checks the returned identity.

## Problems

1. Part checks miss unexpected output. Tests of JSON fields alone do
   not protect the default text output or the rest of stderr.
2. A check of one file key misses wrong values and extra keys.
3. Paths, ports, measured times and release versions change between runs.
   OS messages and error numbers also differ across systems.
4. Broad filters can hide behavior. Examples include fixed timeout limits,
   intentional whitespace, server metadata and the real client version.
5. Snapshot updates need a review workflow that also works with read-only
   source and stale pending files.

## Determinations

### Transcript and helper

Tests remain Rust code. `ncp-testkit` uses `insta` 1.49 with its `filters`
feature and `regex`. The update tool is `cargo-insta` 1.49.0.
mise installs it from source under the existing `cargo.binstall = false`
rule. Only the update tool rewrites inline snapshots.
The hook and CI do not need the update tool.

```rust
ncp_snapshot!(home.filters(), home.ncp(["server", "list"]), @"...");
ncp_snapshot!(
    home.filters(),
    home.ncp(["secret", "set", "TOKEN"]),
    input = "abc",
    @"..."
);
```

The macro takes filters, a `Command`, optional text input for stdin,
and one inline snapshot. It applies transcript filters before the
`insta::assert_snapshot!` comparison, as uv does.

The transcript holds the exit code, stdout and stderr:

```text
exit_code: 0 (success)
----- stdout -----
myproject 1.10.31
```

An empty stream has no section. An empty line separates stdout and
stderr. A signal termination has an `exit_status:` line.
The macro prints the unfiltered transcript to stderr for failure diagnosis.

The macro returns the same run for further assertions. A test must not
run the command again to obtain raw output or other data.
The macro does not infer the output format or run diagnostic-style checks.
Tests retain `run.error()` and `run.warnings()` on the returned run.

`home.ncp(args)` sets the home environment and current directory.
It sets `argv[0]` to `ncp` with `CommandExt::arg0`, so clap writes
`Usage: ncp`, not `Usage: ncp-cli`. Tests can adjust the command before
the macro runs it.

Arguments select the full command. Text remains the default.
JSON tests pass `--output-format json`. Stdout uses a pipe, so JSON
results retain the one-line layout that scripts read.
The helper does not parse, indent or otherwise rewrite JSON output.

### Coverage and incremental migration

New tests of command output snapshot the full transcript of each relevant
run, except for the cases below. Each command gets a text snapshot of
its main success and main error. A command without an error path uses
its warning instead.

The initial delivery adds this text coverage without a whole-suite
migration. A later change moves a current test to snapshots when it edits
that test. An explicit change can instead move one command group.
Other current tests retain their part checks.

A moved test of a main success or main error snapshots both output
formats. Shared helpers and loops can cross command groups.
Their migration must retain the affected cases.

These rules apply to tests that `mise run test` runs.
Ignored tests retain their part checks.
Runs with open stdin, signals, concurrent stream reads, or output streamed
to a file also retain their part checks.

Each inline assertion has its own call site. It must not run repeatedly
through a loop, closure or helper. `insta::allow_duplicates!` is appropriate
only when all runs must produce the same text.

### Files and raw assertions

A test that changes a selected file snapshots that whole file.
It does not inventory every file that the command changes.
File snapshots use `insta::assert_snapshot!` inside
`insta::with_settings!({filters => home.filters()}, { ... })`.
File-mode assertions remain.

The following assertions remain beside snapshots:

- Secrets do not appear.
- JSON diagnostics obey their rules through `run.error()` and `run.warnings()`.
- Exact bytes, final newlines and line-end spaces remain correct.
- Requests, file modes and other data outside the transcript remain correct.
- A filtered value remains correct when that value is the subject of the test.
- Output with variable order or line count retains its required properties.
  Examples include server logs, rate-limited progress and origin-sorted values.

Snapshots cannot replace raw-byte assertions. Transcript conversion can
replace non-UTF-8 bytes, and `insta` normalizes CRLF and trims snapshot
boundaries. An accepted snapshot can also contain a secret.

### Filters

Filters replace changing values, not behavior that the test must protect.
`home.filters()` supplies home and workspace paths, local ports,
measured times and the narrowly scoped client-version contexts below.
Snapshots must remain stable across build locations and distinguish different
executables, servers and other values whose identity matters.

Like uv's [own-executable filter](https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L1040-L1046),
filters replace each known build executable's exact path with a distinct placeholder.
Examples are `[NCP]`, `[FIXTURE]`, `[FAKE_SECURITY]` and `[BLOCKED_STORE]`.
Each replacement applies when that executable's path appears in compared output
or a selected file. Arguments and unrelated paths retain their existing treatment.
There is no build-directory replacement.

uv's [temporary `[BIN]` directory](https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L1083-L1087)
is separate from its own-executable filter. This design does not need that directory,
executable copies or symlinks.

Path filters cover real paths and paths as supplied by tests.
They escape regex metacharacters and preserve path separators.
More-specific paths precede enclosing paths. Real paths precede aliases,
including `/private/var/...` before `/var/...` on macOS.
Paths precede local ports and the remaining standard filters.

Tests can add filters before or after the standard filters.
Later filters see earlier replacements. Tests with multiple local servers
name all but one before the standard `[LOCALHOST]` filter.
This preserves the distinction between servers.

Other changing values need per-test filters, even when repeated runs
produce the same value. Examples include OAuth state, encoded callback
ports, expiry times, resolver messages and trust-record names.
Path-dependent table padding can use a per-test filter that retains indentation.
Fixture payloads must avoid accidental filter matches unless the test
explicitly checks filter scope.

#### OS errors

There is no blanket OS-code filter. Tests explicitly select error-specific
normalization. The uv missing-file helper removes the localized message
and produces `[OS ERROR 2]`. It also normalizes Windows error `3` to `2`.
Its scope is missing files, not all OS errors.

The `ncp` missing-file helper follows that scope and opt-in model.
Its matching must preserve adjacent diagnostic causes, including JSON strings.

Connection-refused tests also select an explicit, error-specific filter.
It replaces only known native connection-refused variants with
`[CONNECTION REFUSED]`. Other error categories remain unmatched.
The surrounding diagnostic and adjacent causes remain intact in text and JSON.
Production diagnostics retain the native cause.

This follows uv's opt-in, error-specific model, not an established uv
connection-refused mapping. uv also provides an opt-in
[permission-error filter](https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L590-L601).
Its [network tests](https://github.com/astral-sh/uv/blob/0.13.7/crates/uv/tests/it/network.rs#L322-L342)
snapshot complete diagnostics from controlled server failures.
Controlled failures remain appropriate when they exercise the intended behavior.
They do not replace the existing connection-refused case with a different failure.

#### Measured times

Time-regex adaptation is an implementation detail, not a policy difference
from uv. Filters cover text and JSON timings, HTTP trace durations and
cache age. They handle whole and decimal milliseconds.

For example, measured `in 12 ms` becomes `in [TIME]`.
The fixed limit `gave no answer in 500 ms` stays exact.
Protocol fields such as `ttlMs` also stay exact.
Filters must identify measured contexts rather than every time-like value.

#### Intentional whitespace

Inline snapshots retain intentional output whitespace.
The standard filters must not remove that whitespace to satisfy source cleanup.
Like uv's inline help exclusion, exempt each source file that intentionally
snapshots trailing whitespace. Cleanup remains active elsewhere.
A file-level exclusion also exempts ordinary Rust code in that file.

uv's [editor configuration][uv-editor] exempts `.snap` files from trimming.
A [historical editor configuration][uv-inline-editor] also exempts a
[Rust file with inline help snapshots][uv-help].
These are editor exclusions, not evidence of hook exclusions.
Insta's [end-of-file fixer exclusions][insta-cleanup] support preservation,
but do not establish a trailing-whitespace rule.

The cleanup configuration lists the source-file exclusions.
Selecting those paths is implementation work, not an open design question.
No content-aware cleanup or per-line exceptions are necessary.
Raw-byte assertions retain exact whitespace and newline obligations.

### Fixture identity and client versions

The shared fixture gets an explicit, fixed identity and test version,
such as `Implementation::new("fixture-server", "1.0.0")`.
That version is test data, not a package release.
The shared handler serves discovery and initialization over stdio and HTTP.

Without an explicit identity, `ServerConfig::new` uses the SDK package
identity. `ncp` displays the identity that the fixture reports.
It does not independently insert its SDK dependency version.
Stable fixture data removes the need for fixture SDK-version filters.

Fixture identity remains exact and visible in command and trace snapshots.
Discovery carries it in `_meta["io.modelcontextprotocol/serverInfo"]`.
Initialization returns `serverInfo`. Tests protect both paths and their
conversion into command output.

The actual `ncp` client retains its real package version in production.
This applies to outgoing discovery and initialize handshakes, and requests after discovery.
The SDK also adds client identity to those later requests.

Version filters apply only to client-owned outgoing metadata or CLI version
results. If a test concerns version reporting, it checks the unfiltered value.
The filter must identify these contexts, not only a matching name or version.
Broad `ncp-*` patterns and filters of arbitrary `serverInfo` are not acceptable.
Protocol versions and identities from real servers remain unchanged.

Implementation first reviews all metadata paths, filter scope and affected
assertions. It removes obsolete fixture filters and updates affected
expectations and helper-test names. It reuses current cases and retains
protocol, cache and diagnostic checks. It does not migrate unrelated tests.

### Delivery and pending files

A separate prerequisite adds `.github/workflows/ci.yaml` for macOS and Linux glibc.
It runs `mise run check` and `mise run test` on the unchanged pre-snapshot suite.
Both runners must pass before snapshot implementation.
Record the successful run URL and commit SHA as the baseline.
The workflow file alone does not establish that pass.
This baseline separates existing failures from snapshot problems.

The helper, filter tests and pending-file support come before new command
coverage. Coverage proceeds by command group, with trust and auth early
because prompts and login flows need distinct capture paths.

Host runs write ignored `*.pending-snap` files beside test sources.
Each Linux container run sets `INSTA_PENDING_DIR=/tmp/insta-pending`.
Pending-file reads and writes then stay inside the container.
This also prevents writes to stale pending files in read-only source.
`INSTA_UPDATE=no` alone does not prevent those writes.
By default, CI runs write no pending files.
Workspace discovery through `cargo metadata` needs no network in the container.

Snapshot updates use `cargo insta test --workspace --accept`.
Every changed transcript needs review. `cargo insta review` is interactive.
An agent uses `cargo insta accept` for pending snapshots.
Container pending files disappear after the run. A container failure needs
a corrected filter or a snapshot update on the host.

## Alternatives

The choices below retain the established order of choice.

| Alternative | Reason not chosen |
|---|---|
| `snapbox` or `expect-test` | Both support snapshots. `insta` follows uv and keeps the expected output beside the Rust test. |
| External `.snap` files | They support direct updates without `cargo-insta`, but separate the expected output from the test. |
| `trycmd`, cram or shell scripts | A second test language cannot start the existing Rust fixtures in the test process. |
| `testscript` | This library uses Go, while the fixtures and tests use Rust. |
| `insta-cmd` | The existing helper still needs control of the environment and stdin. The small custom macro follows uv. |
| `assert_cmd`, `assert_fs` or `predicates` alone | Shorter part checks still miss unexpected output. |
| Snapshot every changed file | Cache files, trust-record names and fake credential data need unrelated filters. Selected whole-file snapshots protect the intended file. |
| Fake TTY or JSON reformatting for readable snapshots | These change the layout or escapes under test. Piped stdout and JSON Lines must remain as consumers receive them. |
| Separate JSON snapshots for every stdout | They split the transcript. An extra snapshot remains possible for a specific large value with a clear need. |
| Blanket OS message-and-code normalization | It hides error identity and can consume adjacent causes. Error-specific opt-in filters retain scope. |
| Strip trailing whitespace from output | It hides intentional whitespace to satisfy a source tool. Narrow source exclusions preserve the consumer-visible behavior. |
| Filter fixture SDK versions | A fixed fixture identity removes release churn without hiding server metadata. |
| Whole-suite migration | It creates a large review burden. Migration on an edited test or explicit command group keeps review bounded. |
| Never migrate current tests | Existing gaps then remain indefinitely. |
| Macro takes the home instead of filters and a command | Explicit arguments preserve uv's form, command adjustment and filter ordering. |
| Transcript filters only through `insta::with_settings!` | uv applies transcript filters itself. File snapshots still use settings. |
| Symlink for the `ncp` program name | `arg0` needs no additional file. |
| `INSTA_UPDATE=no` for read-only runs | Stale pending files can still cause writes. A container-local pending directory handles them. |
| Add a new runner or cache now | Neither resolves the coverage or snapshot correctness problems. |

## Tests

The implementation must retain or add these checks:

- Transcript layout, empty streams, signal status, optional stdin and
  same-run return behavior.
- Standard and per-test filters on text and JSON, including two distinct
  local servers and real-path aliases.
- Exact build-executable path filters in transcripts and selected files.
  Different executables retain distinct placeholders. Only exact executable paths
  change. Other arguments, sibling filenames and unrelated build paths remain intact
  under these filters.
- Opt-in missing-file normalization, its Windows `3` to `2` rule,
  and preservation of adjacent causes.
- Opt-in connection-refused normalization in text and JSON, with unchanged
  surrounding causes. Timeout, DNS and permission failures must remain unmatched.
- Text and JSON timings, stdio timing fields, cache age, whole-millisecond
  HTTP traces, fixed timeout limits and unchanged `ttlMs`.
- A JSON `-vv` transcript against the `legacy` fixture with a JSON-RPC error.
- Intentional spaces, whitespace-only help lines, final newlines and raw
  bytes, with narrow source exclusions.
- Negative version-filter cases for server metadata and unrelated payloads
  named `ncp`, `rmcp`, `ncp-fixture` and `fixture-server`.
  Their versions must remain exact.
- Unfiltered comparisons with the built `ncp` package version for
  `--version`, `server --version`, outgoing discovery and initialize handshakes, and
  one post-discovery request, such as `tools/list`.
  These use the raw run or captured requests, not duplicate command runs.
- Exact fixture identity through discovery and initialization over both
  stdio and HTTP, including conversion into command output.
- Retained protocol, cache, diagnostic, secret and file-mode checks.
- Main text success and error coverage for every command, or its warning
  when no error path exists. Migrated main cases cover both output formats.
- Trust questions, login capture and fake-TTY prompts through the appropriate
  macro or streaming exception.
- A valid stale host `.pending-snap` beside a test source with inline
  assertions that run during `mise run linux:test`. The source stays
  read-only, and passing assertions must still pass.
- The prerequisite CI baseline and later snapshots on macOS, musl and glibc.

## Compatibility

This design changes test infrastructure and controlled fixture data.
It does not change production output, protocol behavior, configuration,
credentials or compatibility rules. Real client versions and real server
identities remain intact.

Implementation affects `crates/testkit/`, dependency and tool lockfiles,
snapshot-ignore rules, container pending-file support and contributor guidance.
Narrow whitespace exclusions can also affect source-cleanup configuration.
The CI baseline remains a separate prerequisite.

[insta]: https://docs.rs/insta/1.49.0/insta/
[uv-snapshot]: https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L2691
[uv-transcript]: https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L2374
[uv-filters]: https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L101
[uv-os-error]: https://github.com/astral-sh/uv/blob/0.13.7/crates/uv-test/src/lib.rs#L354
[mise-assert]: https://github.com/jdx/mise/blob/v2026.11.3/e2e/assert.sh
[uv-editor]: https://github.com/astral-sh/uv/blob/0.13.7/.editorconfig
[uv-inline-editor]: https://github.com/astral-sh/uv/blob/fce7a838e9c8e89cb71f55356cb4ecbc836df5cb/.editorconfig
[uv-help]: https://github.com/astral-sh/uv/blob/fce7a838e9c8e89cb71f55356cb4ecbc836df5cb/crates/uv/tests/help.rs
[insta-cleanup]: https://github.com/mitsuhiko/insta/blob/1.49.0/.pre-commit-config.yaml
[sdk-client]: https://github.com/modelcontextprotocol/rust-sdk/blob/53e64a93c8e32d8f7699af587b9e28566929940c/crates/rmcp/tests/test_server_discover_client.rs#L12-L19
[sdk-http]: https://github.com/modelcontextprotocol/rust-sdk/blob/53e64a93c8e32d8f7699af587b9e28566929940c/crates/rmcp/tests/test_server_discover_http.rs#L19-L26
