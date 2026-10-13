# Study: how uv writes errors, warnings and hints

Date: 2026-10-13. Input for a change to the text form of the messages
of `ncp` on stderr. AGENTS.md makes uv the first reference for
command-line behavior. mise is in section 9 only for comparison.

**Method.** One research agent ran uv 0.13.0 (Homebrew,
`/opt/homebrew/bin/uv`) with `UV_OFFLINE=1` in empty scratch folders. It
caused 18 different failures and copied the output exactly. For color, it
set `FORCE_COLOR=1` and showed the escape codes with `cat -v`. It also read
the source of uv at tag `0.13.0`, commit
`4ecd0eff2a81e3ffdcd7cfee10d08936e4747c17`
(github.com/astral-sh/uv). It did not build uv. Paths are relative to the
root of that repository. Each fact has a mark: V (seen in a run, or read in
the source) or I (inferred).

In the samples, a long scratch path is replaced by `<tmp>`. All other text
is exact.

## 1. Prefixes and levels

uv has four labels: `error:`, `warning:`, `hint:` and `cause:`. uv has no
`note:` label. The labels are lowercase. A colon follows with no space,
then one space, then the message.

| Label | Position | Color (when color is on) | Evidence | Mark |
|---|---|---|---|---|
| `error:` | Column 0, first line | Word bold red, colon bold, message plain | `crates/uv-errors/src/lib.rs:349-375`, run | V |
| `warning:` | Column 0 | Word bold yellow, colon bold. The message is bold for a plain warning, plain for a warning with causes. | `crates/uv-warnings/src/lib.rs:75-90`, `56-69`, run | V |
| `cause:` | Two spaces, then the label | The color of its level (red under an error, yellow under a warning), bold | `crates/uv-errors/src/lib.rs:378-394` | V |
| `hint:` | Column 0, after a blank line | Word bold cyan, colon bold | `crates/uv-errors/src/lib.rs:247-251`, `398-400` | V |

Escape codes from a run (`FORCE_COLOR=1 uv add requests 2>&1 | cat -v`):

```
^[[1m^[[31merror^[[39m^[[0m^[[1m:^[[0m Failed to add dependencies
  ^[[1m^[[31mcause^[[39m^[[0m^[[1m:^[[0m No solution found when resolving dependencies
  ^[[1m^[[31mcause^[[39m^[[0m^[[1m:^[[0m Because requests was not found in the cache and your project depends on requests, we can conclude that your project's requirements are unsatisfiable.

^[[36m^[[1mhint^[[0m^[[39m^[[1m:^[[0m Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.

^[[36m^[[1mhint^[[0m^[[39m^[[1m:^[[0m If you want to add the package regardless of the failed resolution, provide the `^[[32m--frozen^[[39m` flag to skip locking and syncing
```

Inside a message, uv colors a command or a flag green and keeps the
backticks. `STYLE.md:100-113` gives the colors: red for an error, yellow
for a warning, cyan for a hint, cyan for a file path, green for a command.
`STYLE.md:102-103` says that all output must be clear with no color, so
the backticks stay. Mark: V.

### Color control

`crates/uv-cli/src/settings.rs:171-197` uses this order. The first match
wins. Mark: V, and each row was seen in a run.

1. `--color auto|always|never`.
2. `--no-color`.
3. `NO_COLOR`, when it is not empty: no color.
4. `FORCE_COLOR` or `CLICOLOR_FORCE`, when it is not empty: color.
5. Else color only on a terminal.

`NO_COLOR=1 FORCE_COLOR=1` gives no color. `NO_COLOR=1 uv --color always`
gives color. With no variable and no flag, a pipe gives no color.

## 2. Casing and punctuation of a message

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| First letter | A capital letter. Of 773 `#[error("...")]` texts in the non-test source, 724 start with a capital letter or a backtick, and 34 with a small letter. The 34 are mostly low-level errors that show as a `cause:` line. | count over `crates/**/*.rs`, test folders excluded | V |
| The name `uv` | Never capitalized, also at the start of a sentence. | `STYLE.md:24-30` | V |
| Period, one sentence | No period. | `STYLE.md:96-97` | V |
| Period, more than one sentence | A period after each sentence. | `STYLE.md:96-97`; `hint: Packages were unavailable ... from the cache.` | V |
| Backticks | Around commands, code, package names, URLs and file paths. Exception: one value after a colon at the end of the message has no backticks. | `STYLE.md:12-16`; `error: Failed to parse: pyproject.toml` | V |
| Flags with a value | A space, not `=`: `--resolution lowest`. | `STYLE.md:21-22` | V |
| Point of view | The second person is permitted: "Did you mean...?". | `STYLE.md:98` | V |
| Semicolons | Yes. About 51 message texts put the fix after a semicolon, for example `; run \`{}\` to create an environment`. | count over the non-test source; `crates/uv-python-interpreter/src/environment.rs:131-142` | V |

Samples:

```
error: No `pyproject.toml` found in current directory or any parent directory
```

```
error: No virtual environment found; run `uv venv` to create an environment, or pass `--system` to install into a non-virtual environment
```

```
error: Unable to find lockfile at `uv.lock`, but `--locked` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
```

## 3. Cause chains

uv shows the full chain of `std::error::Error::source()` on each error.
There is no option to hide it. The outer error is on the `error:` line.
Each source follows on its own `cause:` line, outer first, inner last
(`crates/uv-errors/src/lib.rs:378-396`). Mark: V.

| Item | Fact | Mark |
|---|---|---|
| Indent of `cause:` | 2 spaces. | V |
| Continuation of a cause | 9 spaces, so that the text aligns under the text after `cause: `. A blank line in the cause stays blank. | V |
| Continuation of the error line | 7 spaces (the length of `error: `), only for a soft wrap on a terminal. A new line that the author puts in the message keeps only its own indent: `wrap_text` gets an empty indent for authored lines (`crates/uv-errors/src/lib.rs`, test `test_multiline_main_message_wraps_each_line`). Runs: `uv help foobar` gives 4 spaces, a TOML warning gives 2 spaces. | V |
| Label history | Before 0.12.14 (2026-09-15) the label was `Caused by:`. The change is "Render error and warning causes with compact `cause:` labels" (PR #21599, #21603). | V (`changelogs/0.12.x.md:978-980`) |
| Warnings | A warning can have a cause chain and hints, since 0.12.14 (PR #21565). | V |

A chain of two levels:

```
error: Failed to spawn: nonexistent-cmd
  cause: No such file or directory (os error 2)
```

A chain of three levels:

```
error: Failed to install cpython-3.9.1-macos-x86_64-none
  cause: Failed to download `https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-apple-darwin-pgo-20210103T1125.tar.zst`
  cause: Network connectivity is disabled, but the requested data wasn't found in the cache: https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-apple-darwin-pgo-20210103T1125.tar.zst
```

A cause with lines of its own (a TOML parse error):

```
error: Failed to parse: pyproject.toml
  cause: TOML parse error at line 3, column 11
           |
         3 | version = 1
           |           ^
         invalid type: integer `1`, expected a string
```

### Line wrap

uv wraps the `error:` line and each `cause:` line at the terminal width.
`COLUMNS` sets the width. `UV_NO_WRAP` stops the wrap. In a pipe with no
`COLUMNS`, uv does not wrap. uv does not break a word, and it breaks only
at an ASCII space, so a URL or a path stays on one line
(`crates/uv-errors/src/line_wrap.rs:1-86`). uv does not wrap a `hint:`
line. Mark: V.

`COLUMNS=60 uv add requests`:

```
error: Failed to add dependencies
  cause: No solution found when resolving dependencies
  cause: Because requests was not found in the cache and
         your project depends on requests, we can conclude
         that your project's requirements are unsatisfiable.

hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.

hint: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing
```

## 4. Hints

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| Place | After the error line and all `cause:` lines. | `crates/uv-errors/src/lib.rs:398-400` | V |
| Separation | A blank line before each hint, also between two hints. | same; `STYLE.md:134-136`; run | V |
| Number | Zero or more. uv collects the hints of each error type in the chain. | `crates/uv/src/commands/diagnostics.rs:22-78` | V |
| Duplicates | A hint with the same text shows one time. | `crates/uv-errors/src/lib.rs:128-140` | V |
| Order | Three groups: `First`, `Any`, `Last`. In a group, the order of insertion. A general hint uses `Last`. | `crates/uv-errors/src/lib.rs:26-35` | V |
| Form | A prose sentence. A command is in backticks inside the sentence: "To update the lockfile, run \`uv lock\`.". A hint is never a bare command. | snapshot tests; run | V |
| Source | An error type implements the trait `Hinted`. The text of the error has no hint in it. | `crates/uv-errors/src/lib.rs:17-22` | V |

The most frequent hint in the snapshot tests of uv (111 times):

```
error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

hint: To update the lockfile, run `uv lock`.
```

A hint about the wrong command:

```
error: No `pyproject.toml` found in current directory or any parent directory

hint: If you meant to view uv's version, use `uv self version` instead
```

### The fix: in the error line or in a hint

uv has no single rule. It uses four forms:

1. A hint line (about 59 hint constructors in the source, and the resolver
   hints).
2. A semicolon in the error line (about 51 texts): `; run \`uv venv\` to
   create an environment`.
3. A second sentence in the error line: `... was provided. To create a
   lockfile, run \`uv lock\` or \`uv sync\` without the flag.`
4. Text in parentheses: `(try: \`--find-links <uri>\`)` in a hint, and
   `No requirements found (hint: use \`--allow-empty-requirements\` to clear
   the environment)` on a status line.

In the sample runs, uv never gave the same fix two times (in the error
line and in a hint). Mark: V for the runs, I for all of uv.

The newer code moves the fix to hints: the trait `Hinted` and the central
function `hints_for_error` came with "Use a consistent format for error
rendering" (PR #17110, 0.12.14). The doc comment of `Hinted` gives
"try `--prerelease=allow`" as the example of a hint. Mark: I (the
direction comes from the change log, not from a written rule).

## 5. Warnings

Format: the same as an error, with `warning:` in yellow. Samples:

```
warning: `VIRTUAL_ENV=<tmp>/empty/venvx` does not match the project environment path `.venv` and will be ignored; use `--active` to target the active environment instead
```

```
warning: No `requires-python` value found in the workspace. Defaulting to `>=3.14`.
```

A warning with a cause chain:

```
warning: Failed to parse: pyproject.toml
  cause: TOML parse error at line 1, column 9
           |
         1 | [project
           |         ^
         unclosed table, expected `]`
```

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| Kinds | `warn_user!` (137 calls), `warn_user_once!` (110 calls), and `warn_user_with_chain!`, `warn_user_once_with_chain!` (15 calls together). | `crates/uv-warnings/src/lib.rs` | V |
| Deduplication | The `_once` forms keep a set of the full text. The second warning with the same text does not show. The key is the full text, with causes and hints, so a different cause shows again. | `crates/uv-warnings/src/lib.rs:39-55`, `116-158` | V |
| `-q` | No warnings. Errors and their hints stay. | `crates/uv/src/lib.rs:562-568`; `crates/uv-command-support/src/printer.rs` (`stderr_important`); run | V |
| `-qq` | No output at all, also no error. Only the exit code. Usage errors from clap still show. | `printer.rs` (`Silent`); run | V |
| Stream | stderr. | `STYLE.md:115-119`; run with `2>/dev/null` | V |
| Before a command | uv enables warnings before it loads the workspace, so that a warning from the configuration shows. | `crates/uv/src/lib.rs:140-143` | V |

## 6. Machine-readable output

uv writes no error as JSON. With `--output-format json`, an error is the
same text on stderr, and stdout is empty. Mark: V.

```
$ uv sync --output-format json   # in a folder with a bad pyproject.toml
error: Failed to parse: pyproject.toml
  cause: TOML parse error at line 3, column 11
...
```

stdout was empty, exit code 2.

Commands with a JSON result on stdout: `uv sync --output-format json`
(schema `{"schema": {"version": "preview"}, ...}`, marked as experimental
in the help), `uv python list --output-format json`, `uv version
--output-format json`, `uv audit --output-format json|sarif`,
`uv tree --format json`, `uv pip list --format json`. Field names use
snake_case (`version_parts`). Mark: V.

## 7. Exit codes

`crates/uv-command-support/src/lib.rs:20-85` and `crates/uv/src/lib.rs:3138-3175`. Mark: V.

| Code | Meaning | Sample |
|---|---|---|
| 1 | A failure from user input, for example no solution, a stale lockfile | `uv add requests` offline |
| 2 | An argument error, or an unexpected error | `No pyproject.toml found`, a parse error |
| 2 | A clap usage error | `uv frobnicate` |
| N | The code of an external command (`uv run`) | |

## 8. The Rust parts

| Part | Role | Path | Mark |
|---|---|---|---|
| `write_error_chain_with_options` | Writes `error:` or `warning:`, the `cause:` lines and the hints. One function for both levels. | `crates/uv-errors/src/lib.rs:345-403` | V |
| `ErrorOptions` | The level word, the color, the width and the output stream. | `crates/uv-errors/src/lib.rs:254-319` | V |
| `Hinted`, `Hint`, `Hints`, `HintOrdering`, `HintPrefix` | Hints as data on an error type. | `crates/uv-errors/src/lib.rs:17-251` | V |
| `ErrorWithHints` | An error and its hints for one `Display`. | `crates/uv-errors/src/lib.rs:184-210` | V |
| `wrap_text`, `get_wrap_width` | Wrap with `textwrap`. Based on the `wrap()` of miette 7.2.0. | `crates/uv-errors/src/line_wrap.rs` | V |
| `uv-warnings` | The `warn_user*` macros, a global switch, and the set for deduplication. | `crates/uv-warnings/src/lib.rs` | V |
| `hints_for_error` | Walks `anyhow::Error::chain()`, downcasts each cause to about 35 known types, and collects their hints. | `crates/uv/src/commands/diagnostics.rs:22-78` | V |
| `Printer` | `Silent` (`-qq`), `Quiet` (`-q`), `Default`, `Verbose`, `NoProgress`. Errors go to `stderr_important`, which only `Silent` turns off. | `crates/uv-command-support/src/printer.rs` | V |
| `UvError` | `User`, `Argument`, `Unexpected`. Selects the exit code. | `crates/uv-command-support/src/lib.rs:34-48` | V |
| clap | Parse errors. uv sets help colors (`STYLES`), and adds a subcommand suggestion. uv does not change the format of clap errors. | `crates/uv-cli/src/lib.rs:66-71`; `crates/uv/src/lib.rs:3016-3060`, `3096-3102` | V |
| Libraries | `anyhow` and `thiserror` for errors, `owo-colors` for colors, `anstream` for the color choice and for the removal of codes. uv does not use miette to show errors. | `crates/uv-errors/src/lib.rs:1-9` | V |

## 9. Comparison with mise 2026.10.7

mise differs from uv in these points. Mark: V (runs).

```
mise ERROR error parsing config file: <tmp>/mise3/mise.toml
mise ERROR Config files in <tmp>/mise3/mise.toml are not trusted.
Trust them with `mise trust`. See https://mise.jdx.dev/cli/trust.html for more information.
mise ERROR Version: 2026.10.7 macos-arm64 (2026-10-09)
mise ERROR Run with --verbose or MISE_VERBOSE=1 for more information
```

| Item | uv | mise |
|---|---|---|
| Prefix | `error:`, `warning:` | `mise ERROR`, `mise WARN ` (log style, uppercase level, program name) |
| Cause chain | `  cause:` lines | Each cause is a new `mise ERROR` line. With `-v`, a numbered `color-eyre` report (`0:`, `1:`, `Location:`, `Version:`). |
| Fix | Hint after a blank line | A second sentence, on a new line with no prefix |
| Extra lines | None | The version and "Run with --verbose" on each error |
| Case | Capital first letter | Mixed: "error parsing config file", "Config files in ..." |
| `-q` | Hides warnings | The error stays the same |
| Usage errors | clap format | clap format (the same) |

## 10. Points where uv is not consistent

1. **Fix placement.** Four forms (section 4): hint, semicolon, second
   sentence, parentheses.
2. **Periods.** `STYLE.md` says no period after one sentence. These lines
   have one sentence and a period: `error: The lockfile at \`uv.lock\` needs
   to be updated, but \`--locked\` was provided.` and `error: Invalid version
   request: Python <3.6 is not supported but 2.6 was requested.` Two hints
   in one run differ: one ends with a period, one does not.
3. **Command color.** `uv venv` and `--frozen` are green. `uv lock` in
   `hint: To update the lockfile, run \`uv lock\`.` has no color.
4. **Bold warning text.** `warn_user!` makes the message bold.
   `warn_user_with_chain!` does not.
5. **A cause in the message.** `warning: Failed to parse \`pyproject.toml\`
   during settings discovery:` puts the TOML error under the message with
   a 2-space indent and no `cause:` label. The next warning in the same
   run gives the same TOML error as a `cause:` line.
6. **clap errors.** They use single quotes (`'frobnicate'`), `  tip:`
   (indented, green), a `Usage:` line, and ignore `--color always`. The
   errors of uv use backticks and `hint:` at column 0.
7. **Wrap.** The error and cause lines wrap. Hint lines and `warn_user!`
   lines do not.
8. **Old label.** A debug log in `crates/uv-client/src/retry.rs:45-47`
   still writes `  Caused by:`.

## 11. Rules to copy

Each rule is a test on the text output of `ncp` (stderr, no color).
Section numbers point to the evidence.

1. **Level prefix.** The first line of an error starts with `error: `.
   Other prefixes: `warning: `, `hint: `. Lowercase word, colon, one space.
   No `note:` and no other level (section 1).
2. **Capital letter.** The message after `error: `, `warning: `, `hint: `
   and `cause: ` starts with a capital letter, unless it starts with a
   backtick. The name `ncp` stays lowercase (section 2).
3. **Periods.** A line with one sentence has no final period. A line with
   two or more sentences has a period after each sentence (section 2).
4. **No semicolons.** A message has no `;`. uv permits it, but the
   ASD-STE100 rule of the `ncp` docs does not, and the fix goes in a hint
   (rule 7).
5. **Context and cause.** The error line says what failed, with the
   object: `error: Server \`web\` has an entry that is not valid`. Each
   underlying reason goes on its own line `  cause: <reason>`, outer
   first. Do not join context and reason with `: ` on one line (section 3).
6. **One colon before a value.** A colon in a message is permitted only
   before one value at the end of the message: `error: Failed to parse:
   ~/.config/ncp/mcp.json`. That value has no backticks (section 2).
7. **Fix in a hint.** A fix is a `hint:` line, never a part of the error
   line, and never both. uv has both forms. `ncp` takes the hint form
   because its JSON error object has a separate `hint` field (design
   cli.md section 4), so the text and JSON forms carry the same parts.
8. **Hint place.** Hints come after the error line and all `cause:`
   lines. One blank line comes before each hint. A hint starts at column 0
   (section 4).
9. **Hint form.** A hint is a sentence, not a bare command. A command in
   a hint is in backticks: `hint: To call the tool, run \`ncp tool call a
   b\``. Do not give the same hint two times for one error (section 4).
10. **Backticks.** Commands, flags, file paths, URLs, configuration keys,
    server names and tool names are in backticks, except the one value
    after a final colon (rule 6) (section 2).
11. **Cause lines.** `cause:` is indented 2 spaces. A line that continues
    a cause is indented 9 spaces. A soft wrap of the error line on a
    terminal is indented 7 spaces. A new line that the author puts in a
    message or a hint keeps only its own indent in uv (section 3). The
    indent of a cause line keeps the rule of design B60: a line from a
    server cannot start at column 0.
12. **Warnings.** A warning has the same format as an error: `warning:`,
    optional `cause:` lines, optional hints. uv shows a `_once` warning
    only one time in a run. A plain `warn_user!` warning can repeat
    (section 5).
13. **Quiet.** `-q` removes warnings, hints that stand alone, and
    progress. The error, its `cause:` lines and its hints stay. uv does the
    same (section 5). `ncp` already keeps the hints of an error under `-q`:
    design B74 says so, and `error_text` in `ui.rs` does not read the
    verbosity. The `-q` row of cli.md section 2.3 ("No hints and no
    warnings") does not say this clearly. Correction of 2026-10-13: an
    earlier text of this rule said that `ncp` removes the hints of an
    error.
14. **Usage errors.** In text mode, clap keeps its own format (`error:`, `  tip:`,
    `Usage:`, `For more information, try '--help'.`) and exit code 2. uv
    does the same (section 8). In a pipe, design B21 gives the JSON error.
15. **No wrap in a pipe.** When stderr is not a terminal, `ncp` does not
    wrap lines. uv wraps on a terminal only. A wrap on a terminal is
    optional for `ncp` (section 3).
16. **Stream.** Errors, warnings and hints go to stderr. stdout is empty
    after an error, except for the exit codes 1 and 5 of `ncp` (section 6).
17. **Same parts in JSON.** uv has no JSON errors, so this rule is
    specific to `ncp`: the text form and the JSON object come from the same
    data (message, causes, hints). A test can compare the two forms
    (section 6).

Sample of the target form for the two messages of the request. The
wording is an example only:

```
error: Server `web` has an entry that is not valid
  cause: Field `url` uses plain http, which is permitted only for a loopback host

hint: Use an https URL
```

```
error: `a.b` joins a server name and a tool name

hint: Give the server and the tool as two words: `ncp tool call a b`
```
