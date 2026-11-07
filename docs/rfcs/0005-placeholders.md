---
state: accepted
supersedes: []
---

# 0005: One form and one name for each placeholder

## Background

A placeholder is a word that the user replaces with a value, for example
`SERVER` in `ncp tool list <SERVER>`. The CLI and the README have these
placeholders now:

| Place | Examples | Source |
|---|---|---|
| Usage line and list of arguments | `<SERVER>`, `[URL]`, `<PROMPT>`, `[ARGUMENTS]...`, `--header <'NAME: VALUE'>`, `--env <NAME[=VALUE]>` | clap: field names and `value_name` |
| Command line in the help | `ncp tool call <SERVER> <TOOL> [key=value...]` | `AFTER_HELP` in `cli/args.rs` |
| Help sentence | "NAME=VALUE writes VALUE", "'<scheme> ${NAME}'", `key="$(cat FILE)"` | doc comments in `cli/args.rs` |
| Diagnostic | "is not `key=value`", "`<scheme> ${NAME}`", "The word for `<NAME>`" | `tool_args.rs`, `commands.rs`, `add_entry.rs`, `secrets.rs` |
| Command in a hint or a "Next:" line | `ncp server add <SERVER> -- <COMMAND> [ARG...]`, `export TOKEN=<value>`, `ncp secret set TOKEN < FILE`, `ncp server add web ... --secret TOKEN < FILE` | `server_edit.rs`, `session.rs`, `secrets.rs` |
| Example call of `tool show` | `'query=<string>'` | `render.rs` |
| README | `<server-name>`, `<tool-name>`, `[<key>=<value>...]`, `SERVER_VAR` | `README.md` |

uv and mise are the first references for names
(`docs/command-development.md`). Their output (uv 0.13.7, mise
2026.11.3):

| Question | uv | mise |
|---|---|---|
| Case in a usage line | Uppercase: `<PACKAGE>`, `[TARGETS]...` | Uppercase: `[TOOL@VERSION]…`, `[TASK]` |
| Case in a message | Lowercase: "`uv run <command>`" | Both: "`mise dot save <path>`", "`mise run <TASK>`" |
| Case in a help sentence | Uppercase: "specified as `KEY=VALUE` pairs" | Uppercase: "TOOL@VERSION arguments add tools" |
| Name of an object | The noun: `uv tool install <PACKAGE>` | The noun: `mise run [TASK]`, `mise settings get <SETTING>` |
| Two words | `uv self update [TARGET_VERSION]`, `--index-url <INDEX_URL>` | To tell two values apart: `[NEW_PLUGIN] [GIT_URL]`. A variable: `mise set [ENV_VAR]…` (a `NAME=VALUE` pair), `mise unset [ENV_KEY]…` (a name) |
| A program and its arguments | `uv run [COMMAND]` | `mise exec [TOOL@VERSION]… [-- COMMAND]…` |
| Two usages of one command | One line: `<PACKAGES\|--requirements <REQUIREMENTS>>`. The list of arguments shows `[PACKAGES]...` | No example |

## Problems

The test build `ncp-cli` and the README show these problems.

1. **Three forms of case.** Uppercase in the usage lines and most hints,
   lowercase in some hints (`<value>`) and the diagnostics (`key=value`),
   and lowercase with `-` in the README.
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
5. **Two names can mean another value.**
   - `server add --help` writes `<scheme>` for an auth scheme, and "the
     scheme, host and port" for the origin of a URL.
   - Claude Code 2.1.319 writes `claude [options] [command] [prompt]`,
     where `prompt` is the text for the model. An agent can give such
     text to `prompt get <SERVER> <PROMPT>`.
6. **The usage line of `server add` is not correct.** It is
   `<SERVER> [URL] [-- <COMMAND>...]`, so `ncp server add web` looks
   valid. The code needs a URL or a command, but not both.
7. **The zsh completion of `--header` fails.** The script holds
   `'*--header=[...]:'NAME: VALUE':_default'`. The `'` of the value name
   ends the quoted string, so zsh gives `_arguments` two strings:
   `*--header=[...]:NAME:` and `VALUE:_default`.
8. **A hint has `...` for the rest of a command.** A user cannot copy
   `ncp server add web ... --secret TOKEN < FILE`.

## Determinations

### 1. Scope

The rules apply to all output of the CLI and to the README, so that both
show the same placeholders. The output of the CLI is the usage lines,
the help, the diagnostics, the hints, the "Next:" lines and the
completion scripts. The rules do not apply to the other text of `docs/`
or to code comments.

### 2. Form

- A placeholder is uppercase, with `_` between words.
- In a command line, the placeholder is in `< >` when it is required and
  in `[ ]` when it is optional. `...` after the brackets means that the
  user can repeat it. clap writes this form.
- A command line is a usage line, a line of an example or a code block,
  a "Next:" line, or a command in backticks.
- The brackets go around only the text that the user replaces:
  `[KEY=VALUE]...`, `--env <ENV_VAR[=VALUE]>`, `export TOKEN=<VALUE>`,
  `--env '<ENV_VAR>=${API_KEY}'`.
- In a command line, an uppercase word with no brackets, such as the
  sample name `API_KEY`, is not a placeholder.
- In a sentence, the placeholder is the word alone: "ENV_VAR=VALUE
  writes VALUE". A diagnostic puts it in backticks: "is not
  `KEY=VALUE`".
- In `${NAME}`, `NAME` has no brackets, because `${NAME}` is the syntax of
  the configuration file.
- A value name of the grammar has only uppercase letters, `_`, `[`, `]`
  and `=`. Thus the completion scripts can hold it.
- Two texts keep their form:
  - A text that names an argument writes it as clap does: "The word for
    `<NAME>`". clap writes `<NAME>` in its own errors.
  - The example call of `tool show` writes the JSON Schema type of each
    argument: `<string>`, `<string or null>`.

### 3. Names

A placeholder is the noun of the value. A second word comes only when
the noun alone has another value on the same screen, or in the help of
another MCP client. A new value takes the name that clap makes, when
that name obeys these rules. These are all the words:

| Word | Value |
|---|---|
| `SERVER` | Server name |
| `URL` | URL of an HTTP server |
| `COMMAND` | A stdio server: its command and its arguments, after `--`. Also an ncp command in `ncp [OPTIONS] <COMMAND>`. |
| `TOOL` | Tool name |
| `PROMPT_NAME` | Prompt name |
| `URI` | Resource URI |
| `ARGUMENTS` | Arguments of a tool: `KEY=VALUE` pairs, one JSON object, or `-` |
| `KEY` | Key of one argument of a tool or a prompt: `KEY=VALUE` |
| `NAME` | The name in `${NAME}`: a secret or a variable of the environment of ncp |
| `ENV_VAR` | Environment variable of a stdio server: `--env <ENV_VAR[=VALUE]>` |
| `HEADER` | A header of `--header`: its name, `:` and its value |
| `AUTH_SCHEME` | Auth scheme of a header: `AUTH_SCHEME ${NAME}`. RFC 9110 §11.1 calls it `auth-scheme`. |
| `VALUE` | The value of the word before it |
| `PATH` | Path of a file |
| `SECONDS` | Value of `--timeout` |
| `FORMAT` | Value of `--output-format` |
| `CREDENTIAL_STORE` | Value of `--credential-store` |
| `SHELL` | Argument of `generate-shell-completion` |
| `OPTIONS` | Options of a command: `[OPTIONS]` |

### 4. Changes to the CLI

- `--header <HEADER>`, with the help "A header of an HTTP server, for
  example 'X-Api-Key: ${API_KEY}'". The help and the error of
  `add_entry.rs` write `AUTH_SCHEME ${NAME}`.
- `--env <ENV_VAR[=VALUE]>`, with the help "ENV_VAR writes ${ENV_VAR},
  and ENV_VAR=VALUE writes VALUE with no change". The after-help of
  `server add` changes in the same way.
- `prompt get [OPTIONS] <SERVER> <PROMPT_NAME> [KEY=VALUE]...`, with a
  `value_name` on both arguments, because `prompt get` takes only pairs.
- `key=value` becomes `KEY=VALUE` in the help and the diagnostics. "A
  key=value value is text" becomes "The VALUE of KEY=VALUE is text".
  `key="$(cat FILE)"` becomes `KEY="$(cat PATH)"`. The after-help of
  `ncp` writes `[KEY=VALUE]...`.
- `<value>` becomes `<VALUE>` in `session.rs`, `secrets.rs` and
  `server_edit.rs`.
- The hint of `server add` in `server_edit.rs` writes `-- <COMMAND>...`.
- The file hints in `secrets.rs`: "To give the value from a file, run
  `ncp secret set TOKEN < <PATH>`". For `server add`: "To give the value
  from a file, add `< <PATH>` to the command". `read_value` then takes
  the whole hint, not only the command.
- Two commands set their usage with `override_usage`:

  ```
  ncp server add [OPTIONS] <SERVER> <URL>
  ncp server add [OPTIONS] <SERVER> -- <COMMAND>...
  ncp tool call [OPTIONS] <SERVER> [TOOL] [ARGUMENTS]...
  ```

  The second line of `server add` starts with 7 spaces, so that it is
  under the first line after `Usage: `. The list of arguments of
  `server add` keeps `[URL]` and `[COMMAND]...`, and the list of
  `tool call` keeps `[SERVER]`. Each one alone is optional in the
  grammar. `[TOOL]` stays, because after RFC 0004
  `ncp tool call web.search q=x` runs.

### 5. Changes to the README and the documents

- The README obeys determinations 2 and 3, for example
  `ncp server add <SERVER> -- <COMMAND>...` and
  `ncp tool call <SERVER> <TOOL> [KEY=VALUE]...`. A list item writes the
  placeholder as the code block does: "`<SERVER>`: a name that you
  choose". The stdio example has `--env '<ENV_VAR>=${API_KEY}'`, and the
  README says that the user can keep the sample name `API_KEY`. The
  rule sentence is: "Replace each word in `< >` or `[ ]` with a value,
  and remove the brackets. Keep the other characters, such as `=`." The
  sentence about `[ ]` and `...` stays.
- `docs/command-development.md` gets a section "Placeholders". It holds
  determinations 1 to 3. A new argument or option takes its value name
  from the table. A new word gets a row in the same commit.
  CONTRIBUTING.md sends each change to a command, an option or an output
  to this document, so the next change finds the rule.
- STYLE.md links to the new section from "Hints" and "Help text", and
  does not copy it. In "Plain text", `[image ...]` becomes "a
  description in brackets", so that "placeholder" has one meaning.

### 6. Test

The style test of `crates/ncp/src/style.rs` reads the table of
determination 3 from the section "Placeholders" of
`docs/command-development.md`. It reads the table of "Words" from
STYLE.md in the same way. It fails when:

- A value name of the grammar has a character that determination 2 does
  not permit, or a word that is not in the table. A word is a run of
  uppercase letters and `_`. With no `value_name`, the value name is the
  field name in uppercase.
- A checked text of the "Words" test matches `<[a-z][a-z0-9 _-]*>` or
  has `...]`. The one exception is the HTML of the login page in
  `auth.rs`.

## Alternatives

This RFC rejects each option below. The reason follows each one. The
order of choice is: the MCP spec, secure by default, ease of use, the
practice of uv and mise, the simplest design.

### Names

- **`<SERVER_NAME>` and `<TOOL_NAME>`**, as the README has now. uv and
  mise use the noun. The help already says "The server name", and RFC
  0004 gives a hint for a URL in place of a server name.
- **`<PROMPT>`**, with the help "The prompt name, from ncp prompt list".
  An agent reads the usage line first, and problem 5 shows another
  meaning. Ease of use comes before the practice of uv and mise. `TOOL`
  stays, because the help of uv, mise and Claude Code shows no other
  meaning for it.
- **`SCHEME`.** The same screen has the scheme of a URL.
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
  uppercase.
- **Brackets in sentences too** ("is not `<KEY>=<VALUE>`"). uv and mise
  write the word alone.
- **`< FILE` or a sample path in the file hints.** A word with no
  brackets is literal text. In `< <PATH>`, the first `<` is the redirect
  of the shell. The rule of brackets tells which `<` to remove.
- **Uppercase types in the example call** (`<STRING>`). `string or null`
  has spaces, so it cannot be a name.

### Usage lines

- **One line with alternatives**, as uv:
  `<SERVER> <URL|-- <COMMAND>...>`. The user can copy and fill in each
  of the two lines. Also, a required group in the grammar gives the
  wrong order: clap 4.6.7 writes `<URL|COMMAND> <SERVER>`.
- **A required `SERVER` in the grammar of `tool call`.** clap then gives
  its own error, with no hint `ncp server list`, and
  `ncp tool call --help` with no server fails.
- **`[KEY=VALUE]...` for `tool call`.** It also takes one JSON object or
  `-`, so the usage line hides two inputs.

### Rule and test

- **The rule in STYLE.md.** The names come from the grammar, and a
  person who adds an option reads `docs/command-development.md`.
- **No test.** Simpler, but a new argument can bring back problem 4, 5
  or 7 with no review.

## Tests

- The usage lines of `server add` and `tool call` in `--help` and in a
  usage error. They start with `ncp`, also in the test build `ncp-cli`,
  because `override_usage` writes the name. The usage error of
  `tool call` has no line "For more information", because the command
  has its own `--help`.
- The usage line of `prompt get` in `--help`.
- The zsh completion script has `--header` in one quoted string.
- The style test of determination 6.
- A behavior test for each changed diagnostic and hint, in the text and
  the JSON output.

## Compatibility

Only text changes: the CLI output of determination 1 and the README.
The version rule of `docs/command-development.md` does not count these
as breaking changes. The commands, the options and the JSON keys do not
change.
