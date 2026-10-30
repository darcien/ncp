# Command development

This document gives the rules for the commands, the options, the output
and the secrets of `ncp`. [STYLE.md](../STYLE.md) gives the rules for the
text. [docs/release.md](release.md), "Version rule", gives the parts that a
release keeps.

## Order of choice

When two goals disagree, choose in this order: the MCP spec, secure by
default, ease of use, the practice of uv and mise, the simplest design.

For names, options, files and trust, uv and mise are the first references.
Use another tool (for example direnv) only when uv and mise have no answer,
and say so.

## Commands and options

- Names follow uv: full words, one name for each action, no aliases.
- Short options are only `-h`, `-q`, `-v` and `-V`. `ncp` reads no `NCP_*`
  variables.
- No command waits for input that it did not ask for. Each wait has a time
  limit, so that `ncp` cannot block an agent.
- Only `auth login` starts an OAuth login.

## Streams and formats

In this section, "in a pipe" means that stdout is not a TTY, for example
a pipe or a file.

- A result is the output on stdout for the request of a command. Results go
  to stdout, and the output format applies to each result.
- `-h`, `--help`, `-V`, `--version`, the completion script and the hidden
  commands that the script runs write to stdout too. Their output is not a
  result, so it is the same in both output formats: a person or a shell
  reads it. `tool call --help` with no tool name writes the help text.
  `tool call SERVER TOOL --help` shows the tool, which is a result.
- All other output goes to stderr. With no command or no subcommand, the
  text output writes the help text as a usage error on stderr.
- One CLI for people and agents. `ncp` has two output formats: the text
  output and the JSON output. The text output is the default, also in a
  pipe. A script then gets the data that a person sees.
- `--output-format json` selects the JSON output: one JSON value on
  stdout, and JSON Lines on stderr. Interactive text on stderr stays plain
  text, because a person answers it.
- When stdout is a TTY, `ncp` indents the JSON value on stdout. In a pipe,
  the value is on one line, because a program reads it and the indent adds
  no data. A TTY on stdout changes nothing else in the JSON output.
- In the text output, `ncp` indents a JSON value on its own lines, also in
  a pipe. A JSON value in a line with other text stays on one line, for
  example `Default: 5` in `tool show`.
- `resource read` is the one command that writes other data in a pipe than
  on a TTY. In a pipe, it writes the contents with no change and no escapes:
  the text, or the bytes of a blob. Thus
  `ncp resource read SERVER URI > FILE` gives the file when the resource
  has one content.
- `ncp` does not wrap lines. The terminal and the reader know their own
  width. clap joins the lines of a doc-comment paragraph into one line. A
  help string such as `AFTER_HELP` keeps its line breaks, so wrap it in the
  source.

## MCP objects

In the JSON output, the value on stdout holds each MCP object as rmcp
reads it. `ncp` does not hide or change a value in the object. A key that
the rmcp type does not have is not in the value. A free JSON field, for
example `structuredContent` or `inputSchema`, keeps each key.

## Secrets

### No detection

`ncp` knows that a value is a secret only from one of these facts:

- The value comes from `${NAME}` or from a credential store.
- The value is in a header field that RFC 9110 defines as credentials:
  `Authorization` (§11.6.2) or `Proxy-Authorization` (§11.7.2).
- The value is an OAuth token or an OAuth client credential.

Every other value is not a secret, also when it looks like one. A guess
hides some secrets and not others, and the user cannot know which. Do not
propose a detection of secrets by value, by pattern, or by a list of
names. This rule is final.

### Where `ncp` hides a known secret

`ncp` hides each secret value that it knows in the run, also in its
percent, form and JSON encodings. A hidden value shows as `****`. An empty
value shows as empty.

- These places hide it:
  - Each diagnostic, with its data lines, causes and hints.
  - Each log line. In a line of `-vv`, this includes each string, key,
    number and boolean of a JSON-RPC message, the URLs, the HTTP bodies
    and errors, the value of a credential header, and the MCP session ID.
- These places do not hide it: the results on stdout (see "MCP objects"),
  the lines of a server, progress, interactive text, and the login URL
  block of `auth login`.

A change to this scope, or to how `ncp` stores or handles secrets, needs
the maintainer to agree first.
