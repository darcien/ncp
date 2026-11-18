---
state: accepted
supersedes: []
---

# 0005: One form and one name for each placeholder

## Background

A placeholder is a word that the user replaces with a value, for example
`SERVER` in `ncp tool list <SERVER>`. The CLI and the README on `main`
have these placeholders:

| Place | Examples | Source |
|---|---|---|
| Usage line and list of arguments | `<SERVER>`, `[URL]`, `[PROMPT]`, `[ARGUMENTS]...`, `--header <'NAME: VALUE'>`, `--env <NAME[=VALUE]>` | clap: field names and `value_name` |
| Command line in the help | `ncp tool call <SERVER> <TOOL> [key=value...]` | `AFTER_HELP` in `cli/args.rs` |
| Help sentence | "NAME=VALUE writes VALUE", "'<scheme> ${NAME}'", `key="$(cat FILE)"` | doc comments in `cli/args.rs` |
| Diagnostic | "is not `key=value`", "is `key:=json`", "`<scheme> ${NAME}`", "The word for `<NAME>`" | `tool_args.rs`, `commands.rs`, `add_entry.rs`, `secrets.rs` |
| Command in a hint or a "Next:" line | `ncp server add <SERVER> -- <COMMAND> [ARG...]`, `export TOKEN=<value>`, `ncp secret set TOKEN < FILE`, `ncp server add web ... --secret TOKEN < FILE`, `--header 'Authorization: Bearer ${NAME}' --secret NAME` | `server_edit.rs`, `session.rs`, `secrets.rs` |
| Example call of `tool show` | `'query=<string>'` | `render.rs` |
| README | `<server-name>`, `<tool-name>`, `[<key>=<value>...]`, `SERVER_VAR` | `README.md` |

uv and mise are the first references for names
(`docs/command-development.md`). Their output (uv 0.13.9, mise
2026.11.9):

| Question | uv | mise |
|---|---|---|
| Case in a usage line | Uppercase: `<PACKAGE>`, `[TARGETS]...` | Uppercase: `[TOOL@VERSION]…`, `[TASK]` |
| Case in a message | Lowercase: "`uv run <command>`" | Both: "`mise dot save <path>`", "`mise run <TASK>`" |
| Case in a help sentence | Uppercase: "specified as `KEY=VALUE` pairs" | Both: "TOOL@VERSION arguments add tools", "`-E <env>`" |
| Name of an object | The noun: `uv tool install <PACKAGE>` | The noun: `mise run [TASK]`, `mise settings get <SETTING>` |
| Two words | `uv self update [TARGET_VERSION]`, `--index-url <INDEX_URL>` | To tell two values apart: `[NEW_PLUGIN] [GIT_URL]`. A variable: `mise set [ENV_VAR]…` (a `NAME=VALUE` pair), `mise unset [ENV_KEY]…` (a name) |
| A program and its arguments | `uv run [COMMAND]` | `mise exec [TOOL@VERSION]… [-- COMMAND]…` |
| Two usages of one command | One line: `uv add [OPTIONS] <PACKAGES\|--requirements <REQUIREMENTS>>`. The list of arguments shows `[PACKAGES]...` | No example |

ncp builds its grammar with clap 4.6.7 (`Cargo.lock`). The
documentation and the output of clap:

- `Arg::value_name`: "it's somewhat convention to use all capital
  letters for the value name". With no `value_name`, the derive uses
  the field name in uppercase.
- `Command::override_usage`: "Multiple usage lines may be present in
  the usage argument". Each line after the first starts with 7 spaces.
- The errors of clap write a placeholder in uppercase. For example,
  `ncp-cli secret set` with no name writes "error: the following
  required arguments were not provided:" and then `<NAME>`.
- A probe with clap 4.6.7 puts `URL` and `COMMAND` of `server add` in a
  required group. The usage is then
  `ncp server add [OPTIONS] <URL|COMMAND> <SERVER>`. The group comes
  before `SERVER`.

uv 0.13.9 uses clap 4.6.7 (`clap_builder-4.6.7` in its binary). In
`crates/uv-cli/src/lib.rs` at tag 0.13.9, the required group `sources`
of `uv add` holds `packages` and `--requirements`. `packages` is the
first argument. The usage of `uv add` is one line:
`uv add [OPTIONS] <PACKAGES|--requirements <REQUIREMENTS>>`. uv also uses
`override_usage`, for `uvx [OPTIONS] [COMMAND]`.

POSIX.1-2024 (XBD 12.1, Utility Argument Syntax) gives multiple synopsis
lines as an alternative to the `|` notation for mutually exclusive
arguments:
"Arguments separated by the '|' ...
bar notation are mutually-exclusive. ... Alternatively,
mutually-exclusive options and operands may be listed with multiple
synopsis lines."

## Problems

The test build `ncp-cli` and the README show these problems.

1. **Three forms of case.** The usage lines and most hints are
   uppercase. Some hints (`<value>`) and the diagnostics (`key=value`)
   are lowercase. The README is lowercase with `-`.
2. **Two forms of a repeated placeholder.** clap writes `[ARGUMENTS]...`
   and `-- <COMMAND>...`. The hint of `server add` writes `[ARG...]`, and
   the after-help writes `[key=value...]`.
3. **One value has two names.** The README writes `<server-name>`, and
   the CLI writes `<SERVER>`. Two hints and the help of `tool call` write
   `FILE`, and the help of `--config-file` writes `<PATH>`.
4. **One name has three values on one screen.** In `server add --help`,
   `NAME` is the name in `${NAME}` (`--secret <NAME>`), a variable of the
   server (`--env <NAME[=VALUE]>`) and a header name
   (`--header <'NAME: VALUE'>`).
5. **One word has two values.** `server add --help` writes `<scheme>`
   for an auth scheme, and "the scheme, host and port" for the origin of
   a URL.
6. **The usage line of `server add` is not correct.** It is
   `<SERVER> [URL] [-- <COMMAND>...]`, so `ncp server add web` looks
   valid. The code needs a URL or a command, but not both.
7. **The zsh completion of `--header` fails.** The script holds
   `'*--header=[...]:'NAME: VALUE':_default'`. The `'` of the value name
   ends the quoted string. zsh thus gives `_arguments` two strings:
   `*--header=[...]:NAME:` and `VALUE:_default`.
8. **A hint has `...` for the rest of a command.** A user cannot copy
   `ncp server add web ... --secret TOKEN < FILE`.
9. **A hint has a placeholder with no brackets.** When `server add`
   gets a literal credential in a header, the hint is
   `--header 'Authorization: Bearer ${NAME}' --secret NAME`. `NAME` is
   a placeholder in the help of the same command. A reader thus replaces
   it, and can replace one `NAME` and not the other.

## Determinations

### 1. Scope

The rules apply to all output of the CLI and to the README, so that both
show the same placeholders. The output of the CLI is the usage lines,
the help, the diagnostics, the hints, the "Next:" lines and the value
names in the completion scripts. In the zsh script, the label of a
positional argument comes from the clap argument ID, not from the value
name. This label does not change. The rules do not apply to the other
text of `docs/` or to code comments.

### 2. Form

- A placeholder is uppercase, with `_` between words. Uppercase is the
  default of clap, and the errors of clap use it. The text that ncp
  writes matches it.
- In a command line, a required placeholder is in `< >`, and an
  optional placeholder is in `[ ]`. `...` after the brackets means that
  the user can repeat it. clap writes this form.
- A command line is a usage line, a line of an example or a code block,
  a "Next:" line, or a command or a part of one in backticks or quotes.
  Examples: `< <PATH>`, `--timeout <SECONDS>`. In the help, a text with
  `$(` is also a command line: `<KEY>="$(cat <PATH>)"`.
- Placeholder words joined by `=` take one pair of brackets:
  `[KEY=VALUE]...`, `<ENV_VAR=VALUE>`, and the clap form
  `<ENV_VAR[=VALUE]>`. Brackets close before literal text:
  `export TOKEN=<VALUE>`, `--env '<ENV_VAR>=${API_KEY}'`.
- In a command line, an uppercase word with no brackets is not a
  placeholder. An example is the sample name `API_KEY`.
- Prose, and a form that prose names, write the word alone, also in
  backticks: "ENV_VAR=VALUE writes VALUE", "is not `KEY=VALUE`", "Give
  each argument as `KEY=VALUE`".
- In `${NAME}` and `${env:NAME}`, `NAME` has no brackets, because these
  forms are the syntax of the configuration file.
- A value name of the grammar has only uppercase letters, `_`, `[`, `]`
  and `=`. Thus the completion scripts can hold it.
- Two texts keep their form:
  - A text that names an argument writes it as clap does: "The word for
    `<NAME>`". clap writes `<NAME>` in its own errors.
  - The example call of `tool show` writes the JSON Schema type of each
    argument: `<string>`, `<string or null>`.

### 3. Names

A placeholder is the noun of the value. A second word comes only when
the noun alone has another value on the same screen or in the same
command. A new value takes the name that clap makes, if that name obeys
these rules. The table holds all the words:

| Word | Value |
|---|---|
| `SERVER` | Server name |
| `URL` | URL of an HTTP server |
| `COMMAND` | A stdio server: its command and its arguments, after `--`. Also an ncp command in `ncp [OPTIONS] <COMMAND>`. |
| `TOOL` | Tool name |
| `PROMPT` | Prompt name |
| `URI` | Resource URI |
| `ARGUMENTS` | Arguments of a tool or a prompt: `KEY=VALUE` pairs, and for a tool also one JSON object, or `-` |
| `KEY` | Key of one argument of a tool or a prompt: `KEY=VALUE` |
| `NAME` | The name in `${NAME}`: a secret or a variable of the environment of ncp |
| `ENV_VAR` | Environment variable of a stdio server: `--env <ENV_VAR[=VALUE]>` |
| `HEADER` | A header of `--header`: its name, `:` and its value |
| `VALUE` | The value of the word before it |
| `PATH` | Path of a file |
| `SECONDS` | Value of `--timeout` |
| `FORMAT` | Value of `--output-format` |
| `CREDENTIAL_STORE` | Value of `--credential-store` |
| `SHELL` | Argument of `generate-shell-completion` |
| `OPTIONS` | Options of a command: `[OPTIONS]` |

### 4. Changes to the CLI

- `--header <HEADER>`, with the help "A header of an HTTP server, for
  example 'X-Api-Key: ${API_KEY}'. The value of an Authorization or
  Proxy-Authorization header must be '${NAME}', or an auth scheme and
  ${NAME}, for example 'Bearer ${NAME}'."
  - The error of `add_entry.rs` writes the same: "The value of the
    header X must be `${NAME}`, or an auth scheme and `${NAME}`, for
    example `Bearer ${NAME}`."
  - Its clause "so that the value comes from the environment or a
    credential store" becomes a second sentence. Each sentence then
    obeys the word limit of STYLE.md.
  - "Auth scheme" is the term of RFC 9110 §11.1. It is not the scheme
    of a URL.
  - The text gives a concrete example, because a literal copy of a
    placeholder form, such as `AUTH_SCHEME ${NAME}`, is a valid header
    value. ncp stores it, and the server gets the word.
- `--env <ENV_VAR[=VALUE]>`, with the help "ENV_VAR writes ${ENV_VAR},
  and ENV_VAR=VALUE writes VALUE with no change". The after-help of
  `server add` uses the same words: "--env ENV_VAR writes
  ${ENV_VAR}. --env ENV_VAR=VALUE writes VALUE into the file".
- `key=value` becomes `KEY=VALUE` in the help, the diagnostics and the
  hints, for example "Give each argument as `KEY=VALUE`" in
  `commands.rs`.
  - "A key=value value is text" becomes "The VALUE of KEY=VALUE is
    text".
  - `key="$(cat FILE)"` becomes `<KEY>="$(cat <PATH>)"`. This text is a
    command line, and in it `PATH` is not the shell variable.
  - The after-help of `ncp` writes `[KEY=VALUE]...`.
- "The value of X has a `${...}` that is not a valid reference" in
  `add_entry.rs` becomes "The value of X has a reference that is not
  valid". The error then has no `...`.
- "The argument X is `key:=json`, which ncp does not read" becomes "The
  argument X has `:=`, which ncp does not read". The error then needs no
  placeholder.
- `<value>` becomes `<VALUE>` in `session.rs`, `secrets.rs` and
  `server_edit.rs`.
- The hint of `server add` in `server_edit.rs` writes `-- <COMMAND>...`.
- The header hint of `server add` in `server_edit.rs` writes the sample
  name of the README:
  `--header 'Authorization: Bearer ${API_KEY}' --secret API_KEY`.
  `<NAME>` is not possible there, because `${NAME}` takes no brackets.
- The two file hints in `secrets.rs` say "To give the value from a
  file, add `< <PATH>` to the command".
  - The hint of `secret set` thus keeps `--server` and
    `--credential-store`. A full command in the hint drops them.
  - Both callers get the same hint. The hint thus becomes a constant in
    `read_value`, and the parameter of `read_value` for the hint goes
    away.
- `server add` sets its usage with `override_usage`:

  ```
  ncp server add [OPTIONS] <SERVER> <URL>
  ncp server add [OPTIONS] <SERVER> -- <COMMAND>...
  ```

  The second line starts with 7 spaces, so that it is under the first
  line after `Usage: `. The list of arguments keeps `[URL]` and
  `[COMMAND]...`, because the grammar makes each one optional alone.

  The reasons for two lines:
  - The plain usage of clap makes `ncp server add web` look valid
    (problem 6).
  - A required clap group gives `<URL|COMMAND> <SERVER>`, the wrong
    order (Background, probe).
  - In uv, the required group `sources` holds the first argument, and
    the usage of `uv add` is one line.
  - POSIX.1-2024 XBD 12.1 gives multiple synopsis lines as an
    alternative to `|` for mutually exclusive arguments.
  - clap documents more than one line in `override_usage`, and uv also
    uses `override_usage`.

  The tradeoff: `override_usage` replaces the usage that clap makes from
  the words of the run with the two valid forms. Each clap error of
  `server add` thus shows both lines, also when the words of the run
  fit only one form. Today that usage can be wrong:
  `ncp server add web https://x.example/mcp --bogus` shows
  `<SERVER> <URL> [-- <COMMAND>...]`, which permits a URL with a
  command.

  An error for an option value, such as `--header` with no value, shows
  no usage line before or after the change. When the URL and the
  command are both given or both missing, the error of ncp keeps its
  two hints.

### 5. Changes to the README and the documents

- The README obeys determinations 2 and 3, for example
  `ncp server add <SERVER> -- <COMMAND>...` and
  `ncp tool call <SERVER> <TOOL> [KEY=VALUE]...`.
  - A list item writes the placeholder as the code block does:
    "`<SERVER>`: a name that you choose".
  - The stdio example has `--env '<ENV_VAR>=${API_KEY}'`. The README
    says that the user can keep the sample name `API_KEY`.
  - The rule sentence is: "Replace each word in `< >` or `[ ]` with a
    value, and remove the brackets. Keep the other characters, such as
    `=`." The sentence about `[ ]` and `...` stays.
- `docs/command-development.md` gets a section "Placeholders" with
  determinations 1 to 3.
  - A new argument or option takes its value name from the table. A new
    word gets a row in the same commit.
  - CONTRIBUTING.md sends each change to a command, an option or an
    output to this document. The next change thus finds the rule.
  - The two commands in "Streams and formats" of the same document obey
    determination 2: `ncp resource read <SERVER> <URI> > <PATH>`, and
    `tool call <SERVER> <TOOL> --help` with
    `tool call <SERVER>.<TOOL> --help`.
- STYLE.md links to the new section from "Hints" and "Help text", and
  does not copy it.
  - Its templates, such as "To GOAL, run `COMMAND`" and `server NAME: `,
    give the shape of a message, not a command. They do not change.
  - In "Plain text", three uses of "placeholder" mean content that ncp
    does not show. They become "description", so that "placeholder" has
    one meaning.

### 6. Style test

The style test of `crates/ncp/src/style.rs` reads the table of
determination 3 from the section "Placeholders" of
`docs/command-development.md`. The test fails in these cases:

- An argument that takes a value has a value name with a character that
  determination 2 does not permit, or a word that is not in the table.
  - The value name is `get_value_names()` of the argument. If the
    argument has none, it is the ID of the argument in uppercase.
  - A flag has no value name.
  - A word is a run of uppercase letters and `_`.
- A checked text of the "Words" test has `<`, then a lowercase letter,
  then letters, digits, ` `, `_` or `-` up to `>`.
- A checked text of the "Words" test has `...` that does not come
  directly after `]` or `>`.

The only exception is the HTML of the login page in `auth.rs`.

The test matches these patterns with code, because a regex crate is a
new dependency. The `test` step of `hk.pkl` adds
`docs/command-development.md` to its glob. The two comments of `hk.pkl`
that name the Markdown files that a test reads change in the same way.
Review checks the README.

## Alternatives

This RFC rejects each design below, with the reason. The order of choice
is: the MCP spec, secure by default, ease of use, the practice of uv and
mise, the simplest design.

### Names

- **`<SERVER_NAME>` and `<TOOL_NAME>`**, as the README has now. uv and
  mise use the noun. The help already says "The server name". RFC 0004
  tells a user who gives a URL that no server of that name is
  configured.
- **`<PROMPT_NAME>`**, so that an agent does not give the text of a
  prompt as the word. `PROMPT` has one value in ncp: the `name` of
  `prompts/get` (MCP 2025-11-25, server/prompts). uv and mise use the
  noun, as for `SERVER`, `TOOL` and `URI`.
- **`<SCHEME>` or `<AUTH_SCHEME>` for the auth scheme.** `SCHEME` has
  the scheme of a URL on the same screen. A literal copy of
  `AUTH_SCHEME ${NAME}` is a valid header value. ncp stores it, and the
  server gets the word `AUTH_SCHEME`. `Bearer ${NAME}` is a concrete
  valid value.
- **`<COMMAND> [ARG]...`**, as the hint has now. It matches the
  `command` and `args` keys of the entry. uv, mise and clap write one
  placeholder.
- **`ENV_KEY`**, as `mise unset [ENV_KEY]…`. In ncp, `KEY` is the key of
  a tool argument. `ENV_VAR[=VALUE]` shows that the name comes first.
- **`NAME` for the variable of `--env`.** The README example
  `--env 'SERVER_VAR=${API_KEY}' --secret API_KEY` then has two `NAME`
  placeholders in one command.
- **`--header <'HEADER: VALUE'>`.** The completion script cannot hold
  `'` or `:`. uv writes `--config-setting <CONFIG_SETTING>` and gives
  the form in the help.
- **`<SUBCOMMAND>` for an ncp command**, as mise. uv writes `<COMMAND>`
  for both uses, as ncp does now. The two uses are not on one screen.

### Form

- **Lowercase with `-`** (`<server-name>`), as the README has now. clap,
  uv and mise write uppercase in a usage line.
- **Lowercase in messages only**, as uv. ncp then has two forms, and a
  hint does not match its usage line. Most hints of ncp are already
  uppercase (problem 1).
- **Brackets in sentences too** ("is not `<KEY>=<VALUE>`"), so that a
  literal copy of any form fails. uv and mise write the word alone. A
  literal copy of `KEY=VALUE` or `ENV_VAR=VALUE` gives a valid value
  with a wrong name, as a copy of any example can. Only the header
  value gets a concrete example, because a wrong header value goes to
  the server with a credential (determination 4).
- **`< FILE` or a sample path in the file hints.** A word with no
  brackets is literal text. In `< <PATH>`, the first `<` is the redirect
  of the shell. The rule of brackets tells which `<` to remove.
- **Uppercase types in the example call** (`<STRING>`). `string or null`
  has spaces, so it cannot be a name.

### Usage lines

- **One line with alternatives**, as uv:
  `<SERVER> <URL|-- <COMMAND>...>`. The user can copy and fill in each
  of the two lines. A required group in the grammar also gives the
  wrong order: clap 4.6.7 writes `<URL|COMMAND> <SERVER>`.
- **A required `SERVER` in the grammar of `tool call`.** clap then gives
  its own error, with no hint `ncp server list`. Also,
  `ncp tool call --help` with no server fails.
- **`[KEY=VALUE]...` for `tool call`.** The command also takes one JSON
  object or `-`, so this usage line hides two inputs.
- **`override_usage` for `tool call`**, to show
  `<SERVER> [TOOL] [ARGUMENTS]...`. It solves none of the problems
  above. Its usage line then differs from its list of arguments, which
  shows `[SERVER]`. A missing server already gets the hint
  `ncp server list`.

### Rule and test

- **The rule in STYLE.md.** The names come from the grammar, and a
  person who adds an option reads `docs/command-development.md`.
- **No test.** This design is simpler. But with no review, a new
  argument can bring back problem 7 or a word that is not in the table.
  A new hint can bring back problem 8 or a lowercase placeholder in
  brackets. The test does not check the README, a placeholder with no
  brackets such as `< FILE`, or a word of the table with another value
  (problem 4).
- **A scan of the README and of each bracket group in the output
  texts.** It needs a parser of Markdown code spans for a file that
  review checks. After the change, the output texts have only valid
  groups.

## Tests

- Update the snapshots that the change makes different, and review each
  one:
  - In `completion_snapshots.rs`: the zsh and fish completion of
    `--header` and `--env`, and the zsh help of the positional
    `arguments`. The zsh script has `--header` in one quoted string.
  - The three snapshots of the root help.
- Move each test that the change edits to `ncp_snapshot!`, in text and
  JSON:
  - In `cli.rs`: `unset_variable_in_args_gives_a_usage_error_with_the_fix`
    and `name_used_only_in_args_is_not_used_and_names_the_rule`.
  - In `server_edit.rs`:
    `a_literal_credential_header_is_an_error_that_shows_no_value`. This
    test keeps its raw check that the output does not hold the secret
    value.
- Add snapshots in text and JSON:
  - the usage lines of `server add` in `--help` and in a usage error,
  - the error of `server add` with a URL and a command,
  - the `--env` help and the after-help of `server add`,
  - the "Next:" line `export NAME=<VALUE>` of `server add`,
  - the error of a literal auth header,
  - the changed diagnostics of `KEY=VALUE` and `:=`.

  The JSON error keeps only the clap text before `Usage:`. Thus only
  the text snapshot shows the usage lines.
- The unit test `unset_variable_gives_a_hint_to_set_it` in `session.rs`
  changes to `export TOKEN=<VALUE>`.
- The file hint has no command, so it cannot drop an option of the run.
  - `read_value` gets its decision from a pure function of two facts:
    stdin is a terminal, and ncp can ask.
  - When stdin is a terminal and ncp cannot ask, this function returns
    the file hint. A unit test of the function checks this case and the
    hint text.
  - The checks of the real terminal stay in `read_value`. Thus the test
    needs no new hook or test variable.
- The style test of determination 6.

## Compatibility

Only text changes: the CLI output of determination 1 and the README.
The version rule of `docs/command-development.md` does not count these
changes as breaking changes. The commands, the options and the JSON keys
do not change.
