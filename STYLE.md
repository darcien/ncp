# STYLE.md

This document holds the rules for all text of `ncp`: output text (help,
errors, causes, warnings, hints, prompts, text on stdout for people, and
the lines of `-v` and `-vv`), documents, code comments and commit
messages. It is permanent. People and agents obey it in the same way.

The code and its tests hold each sentence. This document holds the rules.
A rule has a reason, so that a reader can decide a new case. An example
comes from the current code.

## Changing output text

- Change a text only in one of these cases: the text breaks a rule of this
  document, of CONTRIBUTING.md or a MUST rule of `docs/requirements.md`,
  the text is false for the code, or a change of behavior makes the text
  false.
- A text that obeys the rules stays, also when another wording seems
  better. Each unneeded change makes work for the next reader and starts
  a new sweep.
- New text copies the form of the nearest existing text. The nearest
  existing text is a text of the same kind (error, hint, prompt, help line
  or line on stdout) in the same file. If the file has none, it is a text of
  the same kind in the same module directory.
- To change a rule, change this document first, after the maintainer
  agrees. Put a rule change and the changes of the texts that it makes
  false in one commit.
- A review uses only this document, CONTRIBUTING.md and the MUST rules of
  `docs/requirements.md`. A review does not make or use another rule list.

## Language

- Write in ASD-STE100 Simplified Technical English. These rules apply:
  - A sentence has at most 20 words in an instruction and at most 25 words
    in a description.
  - Use the active voice.
  - Use simple tenses.
  - Use one word for one meaning.
  - Put a condition before the instruction.
- The STE dictionary and the other STE rules guide new text. They are not a
  reason to change a text that obeys this document.
- A phrase that uv and mise use comes before the STE word list. The STE
  word list does not cover the command line, and users know these phrases.
  Examples: "Did you mean `X`?", "press Enter", "press Ctrl-D", "Usage:".
- Output text names no other tool as the reason for a rule. It never names
  uv, mise or rmcp, because a user of `ncp` does not need to know them.
  Output can name `security`, a shell command, or a server program in an
  example.
- A code comment can name a library that the code calls. It gives no tool
  as the reason for a rule. It states the reason in words, because a
  reader cannot check a tool that the comment only names.

## Words

A test fails when a sentence of output text has one of these phrases.

| Do not use | Use |
|---|---|
| `invalid` | not valid |
| `folder` | directory |
| `please` | (leave it out) |
| `simply` | (leave it out) |
| `basically` | (leave it out) |
| `e.g.` | for example |
| `i.e.` | that is |
| `etc.` | the full list |
| `If you mean` | Did you mean |
| `push Ctrl` | press Ctrl |
| `push Enter` | press Enter |
| `uv` | the reason in words |
| `mise` | the reason in words |
| `rmcp` | the reason in words |

The test checks each string literal with a space in the non-test code of
`crates/ncp/src`, and the help text in the doc comments of
`crates/ncp/src/cli/args.rs`. The match is case-insensitive and by whole
word. The module doc of `crates/ncp/src/style.rs` lists what the test does
not check. A text that the test does not check obeys the table too.

## Terms

One word has one meaning. Use the word of this table for its meaning, and
no other word for it.

A term is a domain noun or a noun with a qualifier, not a plain English word
alone. A text needs plain words such as "value" and "server" in their usual
meaning, so the table cannot reserve them.

| Term | Meaning |
|---|---|
| entry | The object of a server in a configuration file: "the entry of the server `web`". |
| global file | `~/.config/ncp/mcp.json`, in the XDG configuration directory. |
| project file | The `.mcp.json` of a project. |
| configuration file | The global file or the file of `--config-file`. Use it when the text can mean both. |
| native store | The macOS Keychain or the Linux Secret Service. |
| credential file | The opt-in file that holds credentials in place of a native store. |
| credential store | The native store or the credential file. |
| store (verb) | Put a credential into a credential store. Never "keep" or "place". As a noun, "the store" can stand for a credential store when the diagnostic already names it. |
| delete | Take a stored item out of a credential store. |
| remove | Take a part out of a file, or take the trust out. |
| stored item | One record in a credential store. "The item" can stand for it when the text is about a credential store. |
| stored value | The secret text of a stored item. "The value" can stand for it when the text names the secret or the item. |
| destination | Where a stored value can go: an origin, an issuer, or the stdio servers. |
| origin | The scheme, host and port of a URL. |
| HTTP server | In a sentence. Write `http` only as the value of `type`. |
| directory | Never "folder". |

## Streams and formats

The streams and the formats of the output are in CONTRIBUTING.md, "Command-line
principles".

## Stderr lines

A diagnostic is an error, a warning, or a standalone hint. A log line is a
line of `-v` or `-vv`. These are the kinds of line that `ncp` writes to
stderr. In JSON, each line is one object with the key of the kind.

A log line is `debug: ` or `trace: ` and then, for a server, `server NAME: `.
It is not a sentence.

| Kind | Text prefix | JSON key |
|---|---|---|
| error | `error: ` | `error` |
| warning | `warning: ` | `warning` |
| hint (standalone) | `hint: ` | `hint` |
| cause (a part of an error or a warning) | `  cause: ` | `causes`, inside the object of the error or the warning |
| debug (`-v`) | `debug: ` or `debug: server NAME: ` | `debug` |
| trace (`-vv`) | `trace: server NAME: ` | `trace` |
| progress | `progress: server NAME: ` | `progress` |
| timings (`--timings`) | `timings: ` | `timings` |
| login URL block | none. The first line is "Open this URL in a browser to log in to NAME:" | `login` |
| line of a server | none. The line passes as the server wrote it, with control characters as escapes | `serverLog` |

Interactive text (a prompt, "Type y, n or c.") also goes to stderr. It has no
prefix and no JSON key (see "Plain text").

A line that `ncp` writes with a prefix starts at column 0 only with that
prefix, and each continuation line is indented or empty. Thus a
line of a server cannot look like a line of `ncp` that has a prefix. A line
of a server can start with any text, because it passes with no change.

## Diagnostics

Text and JSON of a diagnostic come from the same parts, so a test can
compare them. Each string in JSON is the same as in text, with no label and
no indent.

### Parts and layout

- Labels: `error: `, `warning: `, `hint: ` and `  cause: `. A label is a
  lowercase word, a colon and one space. There is no `note:` label.
- Order: the message line, then data lines, then `cause:` lines, then
  `hint:` lines. In text, one empty line comes before each hint of an
  error or a warning. A standalone hint has no empty line before it.
- Indent: a `cause:` line starts with 2 spaces. A second line of a cause
  starts with 9 spaces. A data line under an error starts with 7 spaces,
  and under a warning with 9 spaces. A hint is one line. Only a label or
  an empty line starts at column 0 (see "Stderr lines").
- An empty line inside a cause stays empty.

### Sentences

- First letter: each message, cause and hint that `ncp` writes starts with
  a capital letter, a backtick, or the word `ncp`. A log line of `-v` or
  `-vv`, a cause from a library or a server, and a data line are exempt. A
  log line is not a sentence, and it has no backticks.
- Periods: a line with one sentence has no final period. A line with two
  or more sentences has a period after each one. A question ends with `?`.
  A message that introduces data lines ends with `:`.
- No semicolons. Make two sentences. A semicolon hides which part is the
  fact and which part is the fix.
- Articles: a kind word before a name takes an article: "the server
  `web`", "the tool `add`", "the header `Authorization`".

### Message, causes and consequence

- The message is a full sentence with a subject. It says what failed and
  names its object. It holds no fix. It holds no text of an inner error,
  because that text is a cause. A reason that `ncp` knows itself can follow
  "because" or "so": "ncp deleted nothing, because the credential stores
  have items of `X` for more than one destination:".
- A message about one server names it: "the server `NAME`". The renderer
  writes no `server NAME:` prefix. The JSON field `server` stays.
- Each error of the source chain is one cause, outer first. Each layer
  writes only its own part. A cause that the text above it already holds
  does not show again.
- A consequence tells what `ncp` did or did not do because of the failure.
  It is a second sentence of the message, not a cause and not a hint:
  "ncp sent nothing to the server `linear`."
- When `ncp` stops on purpose to keep a credential safe (it does not send
  a stored token, or it stops a login), the message says so. The user must
  know that the stop is a safety check and not a defect. The reason is a
  cause or a second sentence of facts, not a "because" clause. Examples: "ncp did not send
  the stored tokens of the server `web`. ncp did this on purpose." and "ncp
  stopped the login to the server `web` on purpose. The server names the
  authorization server `X`, and the entry names `Y`."

### Hints

- Each fix is a `hint:` line. A hint is a sentence, never a bare command.
  The same fix shows only one time: not in the message and in a hint, and
  not in two hints.
- The most probable fix comes first.
- A hint has one of these forms:
  - "To GOAL, run `COMMAND`" (the normal form).
  - "If CONDITION, run `COMMAND` to GOAL".
  - An instruction with an optional condition, goal or event first: "If
    CONDITION, INSTRUCTION", "To GOAL, INSTRUCTION", "After EVENT,
    INSTRUCTION", or INSTRUCTION alone.
  - One or more fact sentences before one of the forms above.
  - One of the forms above and then more instruction or fact sentences: "Then run
    the command again."
  - "Did you mean `NAME`?" With more names: "Did you mean `a` or `b`?",
    "Did you mean `a`, `b` or `c`?".
- In JSON, a hint can have the field `command`. Text shows only the
  sentence. A hint has `command` only when the command is an `ncp`
  command, is the whole fix, has no placeholder, and only reads state or
  starts a login. An agent can run such a command with no review. A
  command that stores, deletes, removes, changes trust or calls a tool,
  other than `ncp auth login`, has
  no `command`.

### Backticks

- A diagnostic puts these values in backticks: commands, flags, paths,
  URLs, schemes, hosts, keys, field names, and the names of servers, tools,
  secrets and variables. An agent can then find each value. This includes
  a value at the end of a line, so `ncp` does not write "MESSAGE: value".
- A value that holds a backtick or a control character shows in double
  quotes, with the escapes of Rust `Debug`. The backticks of a line then
  stay in pairs.
- These texts have no backticks: plain text (see "Plain text"), the help
  text and the log lines. A table cell removes the backticks of an error
  text.

### Foreign text

- Foreign text is text that `ncp` does not write: the text of a library, a
  program, a clap error, or an MCP server. It goes on a `cause:` line or a
  data line. It never goes into the message or a hint.
- A value from a server (a URL, a host, a tool name) can be in a message
  or a hint, in backticks, after the escape of control characters.
- `ncp` does not change the case or the punctuation of foreign text.

### Length

- `ncp` cuts a cause of more than 20 lines or more than 2048 bytes. It
  adds a last line `(ncp did not show N bytes)`. All data lines of one
  diagnostic share one limit. A long body then cannot hide the hints.

### Warnings, quiet and usage errors

- A warning has the parts of an error: message, data lines, causes and
  hints. One request shows each full warning only one time.
- `-q` hides warnings with their causes and hints, standalone hints and
  progress. An error always shows with its causes and hints. `-qq` is
  the same as `-q`.
- A usage error of clap keeps the format of clap in text. In JSON, the
  message is "The command line is not valid", the clap lines up to
  `Usage:` (with no empty line and no `tip:` line) are one cause, and a
  suggestion of clap becomes "Did you mean `X`?".

## Plain text

Plain text is the text for people on stdout (tables, entry lines, the text
of `ncp trust`) and the interactive text on stderr (a prompt, "Type y, n or
c.", "The form has no field."). It has no labels and no backticks (see
"Backticks").

- A line is a full sentence with a final period, also when it has one
  sentence. A line with two or more sentences has a period after each one.
  There is no semicolon.
- A field line `Label: value` is not a sentence. It has no final period.
  Examples: "Project file: PATH", "Server: NAME", "Protocol version: 2026-07-28".
- A line that introduces a list ends with `:`. A question ends with `?`. A
  list line (for example a command under "Next:") and a table row are not
  sentences.
- Content that `ncp` does not show gets a placeholder in brackets, for
  example "[image image/png, 4 bytes as base64]". A placeholder is not a
  sentence.
- A yes or no question is "QUESTION? [y/N] ". It names its object: "Trust
  this file? [y/N] ". The capital letter marks the default, which is no.
  An empty line then never gives a trust.
- A question with three or more choices lists the words, with the key
  letter of each in brackets: "[y]es, [n]o, [c]ancel: ". When two words
  start with the same letter, the key is another letter of the word:
  "c[h]ange".
- A prompt for text is "LABEL: ", with no `[y/N]`.
- Tell a user to press a key with the name of the key: "press Enter",
  "press Ctrl-D".

## Help text

- The first line of a command is a short summary, because the command
  lists show only that line. Details come after a blank line.
- A command starts with a verb: "Show the completion script for a shell".
  A command group is a noun phrase: "Tools of a server".
- The help says only what the code does.
- clap shows the help. In the short help (`-h` and the command lists), it
  removes the last period of the first paragraph. Do not change a doc
  comment only for a final period. A detail paragraph after the summary has
  a period after each sentence.
- The help has no backticks (see "Backticks").

## Documents and comments

- A comment explains why, not what. Give the reason in words.
- CONTRIBUTING.md gives the rules for citations and for the history of a
  change.

## Commit messages

- CONTRIBUTING.md gives the format of a commit.
- Write the subject and the body in ASD-STE100.
- Describe the behavior and the reason for the change. Do not repeat the
  code. A code name alone is not a description.
