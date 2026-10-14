# Design: errors, warnings and hints

Status: proposal, not accepted. Revision 4.

Date: 2026-10-13. Input:
[uv-output-messages.md](../research/uv-output-messages.md) (uv 0.13.0,
section 11 "Rules to copy"), the message inventory of 2026-10-13
(`.scratch/message-inventory.md`, not tracked), [cli.md](cli.md) sections
2, 4 and 9. The review findings and their resolution are in
`.scratch/review-output-messages-adversarial.md` and
`.scratch/review-output-messages-uv-ste.md` (not tracked).

The IDs E01 to E117, W01 to W27, H01 to H08 and C01 to C32 come from the
inventory. Each example in this document also gives the code location.

How to use this document:

| You need | Read |
|---|---|
| The rules | Section 4 |
| The text format and the standard hint sentences | Section 5 |
| The JSON format | Section 6 |
| The types and functions | Section 7 |
| Examples of each rule, with the code location | Section 9 |
| The changes to other design documents | Section 10 |
| The work, step by step | Sections 11 and 12 |
| The reasons for the differences from uv | Section 15 |

## 1. Purpose

`ncp` writes errors, warnings and hints on stderr. Today the text has
these problems:

- One line joins the context, the reason and the fix with `: ` and `; `.
  39 templates have a semicolon.
- The renderer puts `server NAME:` before the message. Three messages then
  name the server two times (E12, E13, E70).
- Four messages give the fix in the message and again in a hint (E09, E70,
  E75, E92).
- Messages start with a small letter. Text from rmcp starts with a capital
  letter, so one line mixes the two.
- The JSON error object holds only the first hint. Other hints follow as
  separate hint lines (B74).

The user decided to match uv as closely as the rules of `ncp` permit. uv is
the first reference for command-line behavior (AGENTS.md).

This proposal changes no MUST requirement. It adds no dependency. It does
not change how `ncp` stores or handles secrets. Section 14 gives its
place in the plan.

## 2. Terms

| Term | Meaning in this document |
|---|---|
| Diagnostic | An error, a warning, or a standalone hint on stderr. |
| Message | The text of the first line of a diagnostic, after its label. In JSON, the field `message`. |
| Data line | A line of data under the message, for example an entry of a project file. In JSON, one string of `data`. |
| Cause | One reason under the message, on a `cause:` line. In JSON, one string of `causes`. |
| Hint | One fix or one piece of advice, on a `hint:` line. In JSON, one object of `hints`. |
| Standalone hint | A hint that is not a part of an error or a warning (H01 to H05, H07). cli.md B3 calls it a "hint note". |
| `ncp` text | Text that the code of `ncp` writes, also the Display text of an `ncp` error type. |
| Foreign text | Text from rmcp, reqwest, hyper, `std::io`, serde, clap, the program `security`, or an MCP server. |
| Store | In a message or a hint: to put a credential into the native store or the credential file. |
| Delete | In a message or a hint: to take a stored item out of a store (`ncp secret delete`). |
| Remove | To take a part out of a structure. Examples: an entry or a key of a configuration file, a field or a function of the code. |
| Item | One record in a store. Its secret text is its "value". |

## 3. Scope

| Output | In this change | Reason |
|---|---|---|
| Errors (`Failure`), text and JSON | Yes | The user decision. |
| Warnings, text and JSON | Yes | The user decision. uv uses one format for both levels. |
| Hints of an error or a warning | Yes | The user decision. |
| Standalone hints (H01 to H05, H07) | Yes | They use the label `hint:`, so they obey the hint rules. |
| Display texts of the `ncp` error types | Yes | They become causes. |
| stdout fields that hold an error: `invalid[].error` of `server list` and of `ncp trust` | Yes | They hold the same Display texts (section 6.4). |
| clap usage errors in text mode | No | uv keeps the format of clap too (research section 8). |
| `debug:`, `trace:`, `progress:`, `timings:` lines | No | They are logs for each server, with a stable prefix that people search for. uv writes its debug lines in another format too. |
| Questions and prompts (`ask.rs`, `Trust? [y/N]`, P01 to P16) | No, later | They are a dialog, not a diagnostic. Most of them are already sentences with a capital letter. A second change keeps this diff small. |
| Success text on stdout (I01 to I12) | No, later | It is a result, not a diagnostic. It already uses sentences with periods. |
| Color | No | B60 removed color on 2026-10-10. This proposal does not open that decision again. |
| Exit codes | No | Requirement I3 and cli.md section 3. |

## 4. Rules

Each rule is a test on the output with no color. The reason follows each
rule.

1. **Labels.** An error starts with `error: `. Other labels are
   `warning: `, `hint: ` and `  cause: `. The label is a lowercase word, a
   colon and one space. There is no `note:` label.
   Reason: uv (research section 1).

2. **Order.** An error or a warning has these parts, in this order:
   - the message line,
   - zero or more data lines,
   - zero or more `cause:` lines,
   - zero or more `hint:` lines.

   Reason: uv (research sections 3 and 4).

3. **First letter.** Each message, cause and hint in `ncp` text starts
   with a capital letter, a backtick, or the word `ncp`.
   Reason: uv capitalizes messages and never capitalizes its own name
   (research section 2).

4. **Periods.** A line with one sentence has no final period. A line with
   two or more sentences has a period after each sentence. A question ends
   with `?`. A message that introduces data lines ends with `:`.
   Reason: uv `STYLE.md`. uv code does not always obey this rule, for
   example its most frequent hint. `ncp` obeys `STYLE.md`.

5. **No semicolons.** `ncp` text has no `;`.
   Reason: ASD-STE100 rule 8.1, the writing rule of `ncp`. uv permits
   semicolons, so this is a difference from uv.

6. **The message.** The message is a full sentence with a subject. It says
   what failed, and it names its object. It does not hold the reason of an
   inner error, and it does not hold a fix.
   Reason: uv puts the fix and the inner reason on other lines (research
   rules 5 and 7). uv starts most error lines with "Failed to" and no
   subject. ASD-STE100 rule 4.2 does not permit omitted words, and a
   statement with no subject omits a word. The order stays as in uv: what
   failed, then the object.

7. **Causes.** Each error in the source chain of the failure becomes one
   `cause:` line, outer first. Each layer of an `ncp` error type writes
   only its own part and gives the inner error as its source. Before the
   redaction of rule 21, `ncp` makes a list: the message, then the causes.
   If the last string in the list contains the text of a cause, that cause
   does not show.
   Reason: uv shows the chain of `source()`. The omission is specific to
   `ncp`, because rmcp and reqwest put the inner text into the outer text.
   Exception: a store error that passes through rmcp arrives as one text.
   It shows as one cause.

8. **Consequence.** A consequence tells what `ncp` did or did not do
   because of the failure. It is a second sentence on the message line.
   Examples: "ncp deleted nothing.", "ncp declines.", "ncp sent a cancel
   request.". A consequence is not a reason, so it is not a cause. It is
   not a fix, so it is not a hint.
   Reason: uv uses this form, for example "No `requires-python` value
   found in the workspace. Defaulting to `>=3.14`.".

9. **Hints.** Each fix is a `hint:` line. A hint is a sentence, never a
   bare command. A command in a hint is in backticks. A hint has one of
   these forms:
   - "To GOAL, run `COMMAND`" (the normal form),
   - "If CONDITION, run `COMMAND` to GOAL",
   - an instruction, with an optional condition, goal or event first:
     "If CONDITION, INSTRUCTION", "To GOAL, INSTRUCTION", "After EVENT,
     INSTRUCTION", or only "INSTRUCTION",
   - one or more sentences with a fact, before one of the forms above,
   - one of the forms above, then one or more instruction sentences, for
     example "Then run the command again.",
   - "Did you mean `NAME`?".

   The same fix shows only one time: not in the message and in a hint, and
   not in two hints.
   Reason: uv (research section 4). The user decided this form.

10. **Hint command.** In JSON, a hint can have the field `command`. Text
    shows only the sentence, which holds the same command in backticks. A
    hint has `command` only when all of these are true:
    - The command is an `ncp` command, and it is the whole fix.
    - The command only reads state or starts a login: a `list` or `show`
      command, `ncp trust --show`, `ncp auth login`, or `ncp --help`.
    - `ncp` builds the command from its own words and the words of the
      user, with `shell_word`.
    - The command has no placeholder (`<SERVER>`, `<value>`) and no value
      from a server.

    A command that stores, deletes, removes, changes trust or calls a tool
    has no `command` field. `ncp auth login S --credential-store file` also
    has none, because the opt-in to the credential file needs a decision
    of the user (S2). Its hint has only the sentence.
    Reason: specific to `ncp`, because uv has no JSON. Agents run a hint
    command (secret-binding.md rule 5). The sentence alone does not tell an
    agent which span is the command. A command that changes state needs a
    decision, so `ncp` does not give it as data that an agent runs with no
    review.

11. **Hint order.** The hints come in the order to try them. The most
    probable fix comes first.
    Reason: B74. uv orders hints in groups, with general hints last.

12. **Server name.** When a diagnostic is about one server, its message
    names the server: "the server `NAME`". The renderer writes no
    `server NAME:` prefix. The JSON object keeps the field `server`.
    Reason: uv puts the object of the failure in the sentence (research
    rule 5). The name then shows one time in the text.

13. **Articles.** A kind word before a name takes an article: "The server
    `web`", "the tool `add`", "the variable `ARG_ONLY`".
    Reason: ASD-STE100 rule 4.5. uv mostly writes "Package `x`" with no
    article, and sometimes "The package `x`".

14. **Backticks.** These values are in backticks: commands, flags, file
    paths, URLs, URL schemes and hosts. Configuration keys, field names,
    method names and the names of servers, tools, secrets and variables
    are also in backticks. Examples of schemes: `https`, `http`,
    `javascript:`. This applies also to a value at the end of a line. `ncp` does not use the uv form `MESSAGE: value`.
    A value that contains a backtick or a control character shows in
    double quotes, with the escapes of Rust `Debug`.
    Reason: an agent finds each value by its backticks. A period after a
    value without backticks makes the value unclear (`a.b.` or `a.b`). A
    value from a server can contain a backtick, and the quotes then keep
    the spans in pairs.

15. **Foreign text.** Free foreign text is a message or a body from a
    server, or the text of a library error. It goes on a `cause:` line or
    a data line. It never goes into the message or a hint. A value from a server
    (a URL, a host, a tool name) can be in a message or a hint, in
    backticks, after `printable` (B15). `ncp` does not change the case or
    the punctuation of foreign text.
    Reason: uv shows library text as it is. A change of case can change a
    code, for example `access_denied`. Text from a server never starts a
    labeled line, so it cannot look like a diagnostic of `ncp` (B60). The
    JSON `message` then holds only `ncp` text and values.

16. **Length.** `ncp` cuts a cause that has more than 20 lines or more
    than 2048 bytes. It shows the first lines of the cause, at most 20
    lines and at most 2048 bytes. It cuts at a line end. If one line is
    longer than 2048 bytes, it cuts that line at a character boundary.
    Then it adds a last line to the cause: `(ncp did not show N bytes)`. N
    is the number of bytes that `ncp` did not show. The same limit applies
    to all data lines of one diagnostic together, and the mark is then the
    last data line. `ncp` cuts after the redaction of rule 21.
    Place of the mark:
    - Text, under a cause: a second line of the cause, at 9 spaces.
    - Text, under data lines: a data line, at the data indent.
    - JSON: the last line of the cause string, after a new line, or the
      last string of `data`.

    Reason: rmcp puts the whole body of a non-2xx answer into the error
    (rmcp 3.5.1, `streamable_http_client.rs`). The pages of E26 and the
    entries of E13 come from a server or a project file. A long text then
    hides the hints.

17. **Data lines.** A list of data goes under the message that introduces
    it. Examples: the entries of a project file, the pages to open, the
    problems of a schema check and the expected arguments, a list of
    destinations. Rules 3, 4 and 14 do not apply to a data line.
    Reason: data is not a sentence.

18. **Indent.** A `cause:` line starts with 2 spaces. A second line of a
    cause starts with 9 spaces. An empty line inside a cause stays empty. A
    data line under an error starts with 7 spaces, under a warning with 9
    spaces. A hint is always one line. Only a label or an empty line starts
    at column 0.
    Reason: the cause indent is as in uv (research section 3). uv puts the
    lines that the author writes at the indent of the author. `ncp` puts
    data lines at the column of the message text, so that a line from a
    server cannot start at column 0 (B60). A hint is `ncp` text, and rule
    14 puts a value with a new line in quotes. Thus a hint needs no second
    line.

19. **Empty line before a hint.** In text, one empty line comes before each
    hint of an error or a warning. A standalone hint has no empty line
    before it. JSON has no empty line.
    Reason: uv (research section 4, and `uv-tool-commands`
    `upgrade.rs` for a standalone hint).

20. **Warnings.** A warning has the format of an error: `warning:`, data
    lines, `cause:` lines and hints. The hints that follow a warning today
    (H06 after W21, H08 after W26) become hints of that warning. The
    elicitation of one request shows each full warning (server, message,
    causes and hints) only one time.
    Reason: uv shows its `_once` warnings one time in a run. `ncp` needs
    this only for an elicitation, which can run 8 rounds (B63) and give
    the same warning in each round. A set in the elicitor of one request
    needs no shared state in `Ui`.

21. **Redaction of each part.** Today `ncp` hides secrets in the message of
    an error about an http server with `hide_expanded_url` (session.rs).
    This function writes the server URL as the file writes it, with its
    user information and its OAuth query values hidden. It also hides each
    `${NAME}` value of the server URL in each other place, also in its
    encoded forms. After this change, the same function runs on the
    message, on each data line, on each cause and on each hint, at the same
    places. B70 applies to each part: no part names a stored URL or issuer.
    Reason: requirement S3. Causes now hold the text of reqwest and rmcp,
    which can hold the server URL. This rule keeps the redaction of today
    for the new parts. A wider redaction changes the handling of secrets.
    It is not in the scope of this proposal.

22. **Quiet.** `-q` hides warnings with their causes and hints, standalone
    hints, and progress. An error always shows with its causes and its
    hints.
    Reason: uv keeps the causes and hints of an error under `-q` (research
    section 5). B74 already keeps them.

23. **No wrap.** `ncp` does not wrap lines, on a terminal or in a pipe.
    Reason: a wrap breaks a long command that a person copies. Without a
    wrap, `grep` finds a whole message on one line. uv itself wraps only
    messages and causes, not hints. A wrap on a terminal needs the width
    of the terminal, from `COLUMNS` or from a new dependency such as
    `terminal_size`. A wrap on a terminal is optional later work.

24. **Same parts in JSON.** Text and JSON come from the same data: the
    message, the data lines, the causes and the hints. Each string in JSON
    is the same as in text, with no label and no indent. In JSON,
    `message` holds only the message line, and `data` holds the data
    lines. A test can compare the two forms.
    Reason: specific to `ncp`, because uv writes no JSON errors. A
    separate `data` keeps server text out of `message` (rule 15).

25. **Usage errors of clap.** In text mode, a parse error keeps the format
    of clap. In a pipe, the JSON error object (B21) has an `ncp` message.
    The clap lines up to the line `Usage:`, with no empty line and no
    `tip:` line, are one cause. When clap gives a suggested subcommand or
    argument, the error has the hint "Did you mean `X`?".
    Reason: uv does not change clap errors (research section 8). The clap
    lines are foreign text, so rule 15 puts them on a cause. Some clap
    errors put the names of the missing arguments on the lines after the
    first line.

## 5. Text format

The layout:

```
error: MESSAGE
       DATA LINE
  cause: OUTER CAUSE
         SECOND LINE OF THE CAUSE
  cause: INNER CAUSE

hint: FIRST HINT

hint: SECOND HINT
```

A warning has the same layout with `warning: ` and a data indent of 9
spaces. A standalone hint is one `hint:` line (section 5.2). Section 9
gives full examples: causes and a hint (9.1, 9.5), data lines (9.8,
9.11) and warnings (9.12, 9.13).

A cause with two lines, from a message of a server:

```
error: The server `fx` returned the error -32603 for `tools/call`
  cause: Internal error
         error: run this
```

The second line of the cause starts at column 9, so it cannot look like an
error of `ncp`.

A long cause that `ncp` cut (rule 16):

```
error: The handshake with the server `docs` failed in `server/discover`
  cause: unexpected server response: HTTP 502 Bad Gateway: <!DOCTYPE html>
         <html>
         ...
         (ncp did not show 31744 bytes)
```

The line `...` stands for 18 lines that this document does not show.

### 5.1 Standard hint sentences

The same fix uses the same sentence in each place. The column "Command"
tells if the JSON hint has the field `command` (rule 10). S, T, N, I and P
are values that `ncp` puts in.

| ID | Fix today | Hint sentence | Command |
|---|---|---|---|
| E12, E13, E76, E90, E109 | `ncp trust` | To trust the file, run `ncp trust` | No: changes trust |
| E13 | `ncp trust --ignore` | To ignore the file and use the entry of the global file, run `ncp trust --ignore` | No: changes trust |
| E101 | `ncp trust --show` | To see the entries of the file, run `ncp trust --show` | Yes |
| E16, E20, E28, E43 | `ncp auth login S` | To log in, run `ncp auth login S` | Yes |
| E24, E36, E41, E45 | `ncp auth login S --credential-store file` | To store the tokens in the credential file, run `ncp auth login S --credential-store file` | No: stores in the file |
| E16 (B51) | `ncp secret set N --server S` | If the value of `N` changed, run `ncp secret set N --server S` to store the new value | No: stores |
| E70 | `ncp secret set N --server S` | If the URL is correct, run `ncp secret set N --server S` to store the value | No: stores |
| E85 | `ncp secret set N --server S` | To store a value for the server `S` only, run `ncp secret set N --server S` | No: stores |
| E71, E78 | `... --credential-store file` | To store the value in the credential file, run `ncp secret set N --server S --credential-store file` | No: stores |
| E84 | `give --credential-store file` | To store the value in the credential file, run the command again with `--credential-store file` | No |
| E75 | `ncp secret set N --server <SERVER>` | To store the value for one server, run `ncp secret set N --server <SERVER>` | No |
| E70, E86, E89 | `ncp secret list` | To see the stored items and their destinations, run `ncp secret list` | Yes |
| E87, E88 | `ncp secret delete N --server <SERVER>` | To delete only the item of one server, run `ncp secret delete N --server <SERVER>` | No |
| E74 | `ncp secret delete N --all` | To delete all items of `N`, run `ncp secret delete N --all` | No: deletes |
| W21 (H06) | `ncp secret delete N --server S` | To delete the items of this destination, run `ncp secret delete N --server S`. Then store the value again. | No: deletes |
| E10, E53 | `ncp server list` | To see the configured servers, run `ncp server list` | Yes |
| E51, E52 | `ncp tool list S` | To see the tools of the server, run `ncp tool list S` | Yes |
| E60 | `ncp tool show S T` | To see the arguments of the tool, run `ncp tool show S T` | Yes |
| E09 (`tool call`) | the corrected command | To give the server and the tool as two words, run `ncp tool call S T ARGS` | No: calls a tool |
| E09 (other commands) | `ncp tool show S T` | To see the tool, run `ncp tool show S T` | Yes |
| E62 | the example call | To call the tool, run `ncp tool call S T NAME=<TYPE> ...` | No: calls a tool |
| E55 | `ncp resource list S`, `ncp prompt list S` | To see the resources of the server, run `ncp resource list S` (the same for prompts) | Yes |
| E107 | `ncp server remove N[ --global]` | To remove the entry, run `ncp server remove N` (with `--global` when the global file has it) | No: removes |
| E110 | `ncp server remove N` or `ncp trust` | To remove the entry of the project file, run `ncp server remove N` | No: removes |
| E96, E99, E100 | `--global` | To change the global file, run the command again with `--global` | No |
| E92 | two `ncp server add` forms | Two hints: "To add an http server, run `ncp server add <SERVER> <URL>`" and "To add a stdio server, run `ncp server add <SERVER> -- <COMMAND> [ARG...]`" | No |
| E94 | `--header 'H: Bearer ${NAME}' --secret NAME` | Give the header as `--header 'H: Bearer ${NAME}' --secret NAME` | No |
| E27, E31, E80, E117, E15 | `give a longer limit with --timeout <SECONDS>` | To wait longer, run the command again with `--timeout <SECONDS>` | No |
| E14, E77 | `export N=<value>` | To set the variable, run `export N=<value>`. Then run the command again. | No |
| E21 | `export NO_PROXY=...` | ncp sent the request to the proxy `URL` from `HTTP_PROXY`. To send requests to a loopback host directly, run `export NO_PROXY=localhost,127.0.0.1,::1`. | No: not an `ncp` command |
| E05 | `ncp --help` | To see the commands, run `ncp --help` | Yes |
| E56 | `use --output-format json` | To get all the contents, run the command again with `--output-format json` | No |
| E79 | `give the value on stdin: CMD < FILE` | To give the value from a file, run `ncp secret set N < FILE` | No |
| E20 (exited) | `run the command in a shell to see why: CMD` | To see why the server stopped, run `CMD` in a shell | No: not an `ncp` command |
| E50 | `run the command in a terminal to answer the question` | To answer the question, run the command in a terminal | No |
| E105 | `run the command again` | Another process changed `FILE`. To apply your change to the new file, run the command again. | No |
| E26 | `open the page, then run the command again` | After you complete the steps on the page, run the command again | No |
| E18 | `put the new URL in the entry when you trust it` | If you trust the new URL, put it in the entry | No |
| E20 (404, B78) | the 404 hint | The server has no MCP endpoint at the path of the URL. Many servers use `/mcp`. Examine the path in the entry. | No |
| E17, E19, E20 (B50) | the TLS hint | `SSL_CERT_FILE` replaces the system roots. Put each root that you need in the file. Include the public roots. | No |
| E46 | `register a client at I (see P), then set ...` | Register a client at `I`. Then set `clientId`, `clientSecret` and `issuer` in the `oauth` object of the server `S`. A third sentence names the page `P` when it is safe (B63). | No |
| E47 | one hint with two branches and a `;` | First hint: "If the server moved on purpose, remove `clientId`, `clientSecret` and `issuer` from the `oauth` object of the server `S`. Then run `ncp auth login S`." When the server has no DCR, the first hint tells the user to register a client at `I`. Second hint: "If you did not expect the move, tell the owner of the server. Do not change the entry." | No |
| E48 | `when you registered the client at I, add "issuer": ...` | If you registered the client at `I`, add `"issuer": "I"` to the `oauth` object of the server `S` | No |
| E10, E51 | `did you mean X?` in the message | Did you mean `X`? | No |
| secret_service.rs:99 | `unlock the keyring to list them` | To list the locked items, unlock the keyring | No |

### 5.2 Standalone hints

A standalone hint gives information or advice. When a fix exists, it has
a form of rule 9.

| ID | Hint | Command |
|---|---|---|
| H01 | `PATH` has no server. To add a server, run `ncp server add <SERVER> <URL>`. | No |
| H01 (no file) | `PATH` does not exist, so ncp has no server. To add a server, run `ncp server add <SERVER> <URL>`. | No |
| H02 | The server `S` has instructions for clients. To see the instructions, run `ncp server show S`. | Yes |
| H03, H04 | The server `S` offers no resources in its handshake (the same for prompts) | No |
| H05 | ncp reads the JSON arguments from stdin. To end the input, push Ctrl-D. | No |
| H07 | ncp examined only the global file and the project file of this folder. A project in another folder can still use the item. | No |

## 6. JSON format

stderr stays JSON Lines in a pipe (cli.md section 4). On stderr each
object is on one line. This document shows some objects on more lines
only to make them easy to read. The order of the keys is not a part of the
format.

### 6.1 The error object

```
{"error": {"code", "exit", "server"?, "message", "data", "causes",
  "hints", "destination"?, "pages"?}}
```

- `data` and `causes` are arrays of strings.
- `hints` is an array of objects `{"message", "command"?}`.
- `data`, `causes` and `hints` are always present. An empty array is also
  present.
- `ncp` no longer writes the field `hint`. An error writes no separate
  hint line.
- `destination` is present on an error about a stored value that no place
  has. Its value is the origin, or `null` for a stdio value. Today the code
  writes this field, but cli.md section 4 does not list it.

Example:

```json
{"error": {"code": "usage", "exit": 2, "server": "fx",
  "message": "The entry of the server `fx` in `/home/u/.config/ncp/mcp.json` is not valid",
  "data": [],
  "causes": ["The field `args[0]` is not valid", "The variable `ARG_ONLY` is not set"],
  "hints": [{"message": "ncp puts a stored secret only into `env` and `headers` values. To set the variable, run `export ARG_ONLY=<value>`. Then run the command again."}]}}
```

An error with two hints. Before, the second hint was a separate line:

```json
{"error":{"code":"auth","exit":4,"server":"web","message":"the server needs a login or more scopes","hint":"ncp secret set WEB_TOKEN --server web"}}
{"hint":{"message":"ncp auth login web","server":"web"}}
```

After:

```json
{"error":{"code":"auth","exit":4,"server":"web","message":"The server `web` needs a login or more scopes","data":[],"causes":[],"hints":[{"message":"If the value of `WEB_TOKEN` changed, run `ncp secret set WEB_TOKEN --server web` to store the new value"},{"message":"To log in, run `ncp auth login web`","command":"ncp auth login web"}]}}
```

A miss of a stored value, with `destination`:

```json
{"error":{"code":"usage","exit":2,"server":"linear","message":"The variable `LINEAR_KEY` is not set, and no store has a value for `https://mcp.linear.app`. ncp sent nothing to the server `linear`.","data":[],"causes":[],"hints":[{"message":"If the URL is correct, run `ncp secret set LINEAR_KEY --server linear` to store the value"},{"message":"To see the stored items and their destinations, run `ncp secret list`","command":"ncp secret list"}],"destination":"https://mcp.linear.app"}}
```

A page error (B65) keeps `pages`. The page line from the server is in
`data`, not in `message`:

```json
{"error":{"code":"input_required","exit":5,"server":"fx","message":"The server `fx` asks you to open a page before it answers `tools/call`:","data":["https://login.example.test/start (Sign in first)"],"causes":["Authorization needed"],"hints":[{"message":"After you complete the steps on the page, run the command again"}],"pages":[{"url":"https://login.example.test/start","message":"Sign in first"}]}}
```

### 6.2 Warnings and standalone hints

A warning has the same arrays as an error:

```json
{"warning":{"server":"fx","message":"The server `fx` sent a question that ncp cannot show. ncp declines.","data":[],"causes":["The request has no `url` field"],"hints":[]}}
```

A standalone hint keeps the shape of B3, with the optional `command` of a
hint object:

```json
{"hint":{"server":"fx","message":"The server `fx` has instructions for clients. To see the instructions, run `ncp server show fx`.","command":"ncp server show fx"}}
```

### 6.3 Parse errors (B21)

With no command:

```json
{"error":{"code":"usage","exit":2,"message":"The command line has no command","data":[],"causes":[],"hints":[{"message":"To see the commands, run `ncp --help`","command":"ncp --help"}]}}
```

With a command and no subcommand (`ncp tool`), the message is "The
command `ncp tool` has no subcommand".

A missing argument (`ncp tool list`). The clap lines up to `Usage:` are
one cause:

```json
{"error":{"code":"usage","exit":2,"message":"The command line is not valid","data":[],"causes":["the following required arguments were not provided:\n  <SERVER>"],"hints":[{"message":"To see the commands, run `ncp --help`","command":"ncp --help"}]}}
```

A subcommand with a typing error (`ncp servr list`). clap suggests a
name, so the error has a hint:

```json
{"error":{"code":"usage","exit":2,"message":"The command line is not valid","data":[],"causes":["unrecognized subcommand 'servr'"],"hints":[{"message":"Did you mean `server`?"},{"message":"To see the commands, run `ncp --help`","command":"ncp --help"}]}}
```

### 6.4 Error fields on stdout

`server list` and `ncp trust` write an invalid entry as `{"name",
"error"}` in `invalid` (cli.md section 4, B76 (2)). After this change:

- JSON: `{"name", "error", "causes"}`. `error` is the Display text of the
  outer error. `causes` is the source chain, with rule 7.
- Text: one table cell `not valid: TEXT`. TEXT joins the error and its
  causes with `: `, with rule 7. One pure function `chain_text` makes
  TEXT. The same function makes the reason of an unusable project file
  where one line is necessary.

Example for `"url": "http://example.com/mcp"`:

```json
{"name":"web","error":"The field `url` is not valid","causes":["ncp permits `http` only for a loopback host"]}
```

### 6.5 Compatibility

`ncp` has no release: the version is 0.1.0, and git has no tag. The rule
of cli.md section 4 ("The removal or change of a field needs a major
version") protects users of a release. Thus this change replaces `hint`
with `hints` and writes no transition field. One shape is simpler for each
reader. A reader of the old shape reads `error.hint` and gets nothing. It
does not fail on a parse, because each line is still one JSON object with
the same key.

These JSON shapes change:

- the error object: no `hint`, new `data`, `causes` and `hints`, a new
  `message` text,
- the warning object: new `data`, `causes` and `hints`, a new `message`
  text,
- the standalone hint: new `command`, a new `message` text,
- `invalid[]` of `server list` and `ncp trust`: new `causes`, a shorter
  `error` text.

cli.md section 4 has no written exception for the time before the first
release. Section 10.1 adds it.

## 7. Model

- `Failure` gets private fields: `exit`, `server`, `message`, `causes:
  Box<[String]>`, `hints: Box<[Hint]>` and `extra: Option<Box<Extra>>`.
  `Extra` holds the rare fields `data`, `login_challenge`, `pages` and
  `destination`. Thus `Failure` stays below the 128 bytes of
  `clippy::result_large_err`.
- Read methods: `exit()`, `server()`, `message()`, `hints()`, `pages()`
  and `login_challenge()`. No code outside the module reads or writes a
  field.
- Build methods: `data(line)`, `cause(text)`, `caused_by(&dyn Error)`,
  `hint(Hint)`, `with_hints(hints)`, `with_pages(pages)`,
  `with_login_challenge(challenge)`, `with_exit(exit)` and
  `about(server)`. `with_hints` replaces all hints. To put a hint first, a
  call site reads `hints()` and gives the new list. session.rs:863 does
  this for the `secret set` hints of B51 (rule 11).
- `Failure::context(message)` gives a failure a new message. The old
  message becomes the first cause. The server, the data lines, the other
  causes, the hints and `Extra` stay.
- `Warning::from(Failure)` keeps the server, the message, the data lines,
  the causes and the hints.
  Reason: today some code copies only `failure.message` into a warning or
  a new failure. After the split, the reason is in `causes`, so a copy of
  the message loses it, and no test fails. These places use
  `Warning::from` or `context`: W10 (auth.rs:591), W23 (secrets.rs:981),
  W24 (secrets.rs:1009), W26 (server_edit.rs:475) and E91
  (secrets.rs:1209). E91 also uses `with_exit`.
- A shared failure that gets a server names it in the message (rule 12):
  - `store_failure` at auth.rs:519, 1342 and 1355 (step 7): `context`
    with a message that names the server and the operation, then `about`.
    Examples: "ncp cannot read the OAuth client of the server `S`", "ncp
    cannot delete the tokens of the server `S`". The store text becomes a
    cause.
  - `token_store_failure` (session.rs:884, step 7): "ncp cannot keep the
    OAuth tokens of the server `S`". The store text is a cause.
  - `interrupted(server: Option<&str>)` (step 2): "ncp stopped the work
    with the server `S` because of an interrupt", or "ncp stopped because
    of an interrupt" with no server. The call sites with a server
    (session.rs:701 and 971, ask.rs:295 and 390, auth.rs:828) give it and
    use `about`.
- `Hint` has `message` and `command: Option<String>`, with the names of
  the JSON fields. The constructor `Hint::run(goal, command)` makes "To
  GOAL, run `COMMAND`" and sets `command`. The constructor
  `Hint::run_text(goal, command)` makes the same sentence with no
  `command`, for a command that changes state (rule 10).
- `chain_text(&dyn Error)` replaces `causes()` (session.rs:838). It joins
  an error and its source chain with `: `, with the omission of rule 7.
- `Failure::caused_by(&dyn Error)` adds the causes of a source chain, with
  rule 7. One function builds the causes of a transport error. It keeps
  the manual unwrap of rmcp (`ClientInitializeError::TransportError`,
  `ServiceError::TransportSend`), because rmcp gives no `source()` there.
  It then downcasts to `StreamableHttpError<reqwest::Error>`, takes the
  `reqwest::Error` of `Client`, and walks its chain (hyper, io). `Client`
  has no `source()` in rmcp 3.5.1.
- `Failure::map_text(f)` applies `f` to the message, each data line, each
  cause, each hint message and each hint command. Each redaction place
  uses it. The fields are private, so a new place cannot copy causes with
  no redaction.
- `Warning` has `server`, `message`, `data`, `causes` and `hints`.
- One pure function renders an error or a warning in text, and one in
  JSON. uv also uses one function for both levels.
- During the migration, `about(server)` on a `Failure` or a `Warning`
  marks a diagnostic whose message already names the server. The renderer
  then writes no prefix. `Failure::server(name)` and the server argument
  of `Ui::warning` keep the old prefix until the last step.

## 8. Decisions on the open points

Each open point has a decision. This table gives the place that states
it.

| Point | Decision | Place |
|---|---|---|
| JSON object | The arrays `data`, `causes` and `hints`. No field `hint` and no transition field. `destination` stays. | Sections 6.1, 6.5 and 10.1 |
| Server name | The message names the server. The renderer writes no prefix. JSON keeps `server`. | Rule 12, the style check of section 11 |
| `-q` | An error keeps its causes and hints. | Rule 22 |
| Consequence tails | A second sentence on the message line | Rule 8 |
| Library text | One `cause:` line for each error of the source chain, with no change | Rules 7 and 15 |
| Warnings | Data lines, causes and hints. One time in an elicitation. | Rule 20 |
| Periods | As uv `STYLE.md` | Rule 4 |
| Line wrap | None | Rule 23 |
| Scope | Errors, warnings, hints and the error fields on stdout | Section 3 |
| Migration | Ten steps in a fixed order | Section 11 |

## 9. Before and after

Each example shows a different rule. Section 9.13 gives more messages of
the same forms. The foreign text in these examples comes from rmcp 3.5.1,
reqwest, hyper, std and the `security` program. It can change with a new
version of a library. `/home/u` is the home folder of the user.

### 9.1 The deepest chain (C01, session.rs `entry_failure`)

`ncp tool list fx` with `"args": ["${ARG_ONLY}"]` and no `ARG_ONLY`.

Before:

```
error: server fx: the entry in /home/u/.config/ncp/mcp.json is not valid: "args[0]": variable ARG_ONLY is not set; a stored secret can go only in env and headers values
hint: export ARG_ONLY=<value>
```

After:

```
error: The entry of the server `fx` in `/home/u/.config/ncp/mcp.json` is not valid
  cause: The field `args[0]` is not valid
  cause: The variable `ARG_ONLY` is not set

hint: ncp puts a stored secret only into `env` and `headers` values. To set the variable, run `export ARG_ONLY=<value>`. Then run the command again.
```

Five levels become a message, two causes and a hint. The rule about stored
secrets explains the fix, so it goes into the hint.

### 9.2 A fix after a semicolon (E14, server.rs `UrlError::PlainHttp`)

Before:

```
error: server web: the entry in /home/u/.config/ncp/mcp.json is not valid: "url": plain http is permitted only for a loopback host; use https
```

After:

```
error: The entry of the server `web` in `/home/u/.config/ncp/mcp.json` is not valid
  cause: The field `url` is not valid
  cause: ncp permits `http` only for a loopback host

hint: Use an `https` URL
```

A pure function of the CLI layer gives the hint. It matches on the error
type, as `hints_for_error` of uv does. The Display text holds no fix.

### 9.3 A duplicate fix (E09, session.rs `joined_failure`)

Before:

```
error: "github.search_issues" joins a server name and a tool name; give them as two words
hint: ncp tool call github search_issues q=x
```

After:

```
error: `github.search_issues` joins a server name and a tool name

hint: To give the server and the tool as two words, run `ncp tool call github search_issues q=x`
```

The JSON hint has no `command`, because `tool call` calls a tool. Each
other command that takes a server name gives the hint "To see the tool,
run `ncp tool show github search_issues`", with `command`.

### 9.4 A duplicate fix and the server name two times (E70, secrets.rs `miss`)

Before:

```
error: server linear: LINEAR_KEY is not set, and not stored for https://mcp.linear.app. ncp sent nothing to server linear. If this URL is correct, store the value for it.
hint: ncp secret set LINEAR_KEY --server linear
hint: ncp secret list
```

After:

```
error: The variable `LINEAR_KEY` is not set, and no store has a value for `https://mcp.linear.app`. ncp sent nothing to the server `linear`.

hint: If the URL is correct, run `ncp secret set LINEAR_KEY --server linear` to store the value

hint: To see the stored items and their destinations, run `ncp secret list`
```

### 9.5 A consequence tail (E87, secrets.rs `secret delete` with no flag)

Before:

```
error: ncp cannot list the native store: access to the credential store is denied: security dump-keychain exited with code 51; ncp deleted nothing
hint: ncp secret delete A --server <SERVER>
```

After:

```
error: ncp cannot list the items in the native store. ncp deleted nothing.
  cause: The credential store denied access
  cause: `security dump-keychain` exited with code 51

hint: The native store can have an item of `A` for another destination. To delete only the item of one server, run `ncp secret delete A --server <SERVER>`.
```

Both causes are `ncp` text. The second cause comes from the type that
`ncp` makes for an exit of `security`. When `security` writes on stderr,
that text is a third cause, as foreign text.

### 9.6 An rmcp cause in small letters, with the 404 hint (E20, B78, session.rs `connect_failure`)

The cache has a protocol version for the `initialize` handshake.

Before:

```
error: server docs: initialize failed: unexpected server response: HTTP 404 Not Found: Not Found
hint: check the path of the URL in the entry: the server has no MCP endpoint there, and many servers use /mcp
```

After:

```
error: The handshake with the server `docs` failed in `initialize`
  cause: unexpected server response: HTTP 404 Not Found: Not Found

hint: The server has no MCP endpoint at the path of the URL. Many servers use `/mcp`. Examine the path in the entry.
```

The cause keeps the small letter of rmcp (rule 15).

### 9.7 A connection that the host refuses (E20, session.rs `connect_failure`)

The cache has no protocol version, so `ncp` probes with `server/discover`
first (client.rs). A refused connection is not a legacy answer, so the
probe gives its own error. Today `causes` stops at the reqwest error,
because the rmcp type `StreamableHttpError::Client` gives no `source()`.
The downcast of section 7 shows the reason.

Before:

```
error: server gh: server/discover failed: Client error: error sending request for url (http://127.0.0.1:9/mcp)
```

After:

```
error: The handshake with the server `gh` failed in `server/discover`
  cause: Client error: error sending request for url (http://127.0.0.1:9/mcp)
  cause: client error (Connect)
  cause: tcp connect error
  cause: Connection refused (os error 61)
```

### 9.8 A trust error (E13, session.rs `blocked`)

`fs` is in the untrusted project file and in the global file. Today
`project_entries` writes 2 spaces before each entry, and `indented` adds 2
more.

Before:

```
error: server fs: fs is in /work/app/.mcp.json, which is not trusted:
    fs  npx -y @modelcontextprotocol/server-filesystem .
hint: ncp trust
hint: ncp trust --ignore
```

After:

```
error: The server `fs` is in `/work/app/.mcp.json`, which you did not trust:
       fs  npx -y @modelcontextprotocol/server-filesystem .

hint: To trust the file, run `ncp trust`

hint: To ignore the file and use the entry of the global file, run `ncp trust --ignore`
```

`project_entries` gets a parameter for the indent. The error asks for no
indent, so the JSON `data` has the rows at column 0. The trust question
P01 keeps its indent of 2 spaces. After a change of a trusted file, the
message ends "which changed after you trusted it:".

### 9.9 A near name (E51, commands.rs `find_tool`)

Before:

```
error: server github: the server has no tool "search_issue"; did you mean "search_issues"?
hint: ncp tool list github
```

After:

```
error: The server `github` has no tool `search_issue`

hint: Did you mean `search_issues`?

hint: To see the tools of the server, run `ncp tool list github`
```

### 9.10 A stop on purpose (E43, auth.rs `binding_failure`, requirement A13)

Before:

```
error: server web: ncp did not send the stored token, on purpose: the tokens are for another URL than the entry names
hint: ncp auth login web
```

After:

```
error: ncp did not send the stored token of the server `web`. ncp did this on purpose.
  cause: The tokens are for a URL that is not the URL of the entry

hint: To log in, run `ncp auth login web`
```

The message still says that the stop is on purpose, as A13 requires.

### 9.11 A page to open (E26, session.rs `page_failure`)

Before:

```
error: server fx: tools/call: the server asks you to open a page first (Authorization needed):
  https://login.example.test/start (Sign in first)
hint: open the page, then run the command again
```

After:

```
error: The server `fx` asks you to open a page before it answers `tools/call`:
       https://login.example.test/start (Sign in first)
  cause: Authorization needed

hint: After you complete the steps on the page, run the command again
```

The message of the server error is free text, so it is a cause. The page
message stays on its data line.

### 9.12 A warning with a hint (W21 and H06, secrets.rs `secret set`)

Before:

```
warning: the native store also has A for https://web.example, and ncp reads it first: the new value is not used
hint: ncp secret delete A --server web
```

After:

```
warning: The native store also has an item of `A` for `https://web.example`. ncp reads the native store first and does not use the new value.

hint: To delete the items of this destination, run `ncp secret delete A --server web`. Then store the value again.
```

### 9.13 More messages of the same forms

Each item gives the inventory ID, the code location, the rule that it
shows, and the text after the change.

E30, session.rs `request_failure`. A cause from rmcp:

```
error: The request `tools/list` to the server `gh` failed
  cause: Transport closed
```

E33, auth.rs `auth_failure`. The cause keeps the text of rmcp with its
repeated word (rule 15):

```
error: The OAuth discovery for the server `gh` failed
  cause: Metadata error: Protected resource metadata missing required resource field
```

E27, session.rs `request_failure`. A time limit with a consequence
(rule 8):

```
error: The server `gh` gave no answer to `tools/call` in 300 s. ncp sent a cancel request.

hint: To wait longer, run the command again with `--timeout <SECONDS>`
```

E16 (B51), session.rs `open_with_login`. Two hints after a `401`, in
the order to try them (rule 11):

```
error: The server `web` needs a login or more scopes

hint: If the value of `WEB_TOKEN` changed, run `ncp secret set WEB_TOKEN --server web` to store the new value

hint: To log in, run `ncp auth login web`
```

W14, ask.rs `ask`. A warning with a cause and a consequence:

```
warning: The server `fx` sent a question that ncp cannot show. ncp declines.
  cause: The request has no `url` field
```

W15, ask.rs `ask`. A URL and a scheme in backticks (rule 14):

```
warning: The server `fx` asks ncp to open `javascript:alert(1)`. ncp declines.
  cause: ncp opens only `https` pages, not `javascript:` pages
```

W16, ask.rs `ask`. A value from a server in a message (rule 15):

```
warning: The page of the server `fx` is on the host `xn--pple-43d.com`. This host has a Punycode label (`xn--`), so its name can look like another name.
```

W03 (B76 (5)), session.rs `ProjectFile::Unusable`. The reason is a
cause:

```
warning: ncp does not use `/work/app/.mcp.json`
  cause: Another user owns the file
```

## 10. Changes to the design documents

Each step of the migration (section 11) changes the parts of the design
documents that describe the messages of its area. The column "Step" gives
the step. plan.md says that cli.md wins over the plan. Thus, after a
step, cli.md must not describe the old messages as the target. Step 2
adds B80, which also tells the reader which texts of cli.md are old
until their step.

### 10.1 cli.md sections 1 to 4

| Step | Place | Change |
|---|---|---|
| 2 | End of section 1 | New paragraph: "In this document, 'the hint `COMMAND`' means a hint sentence that gives `COMMAND`." The sentence covers each place that says "with the hint `COMMAND`". These places are section 2.4 (lines 189-190, 213-214, 229-231) and 15 decisions: B17, B28, B31, B32, B35, B51, B52, B58, B67, B71, B72, B73, B75, B77 and C5. |
| 2 | Section 2.3, row `-q` | "`-q` hides warnings, standalone hints and progress. An error shows with its causes and hints (B74)." |
| 2 | Section 4, error object | The text below. Its example gets the shape of the "after" object of section 6.1 and keeps its old message. |
| 2 | Section 4, after the rule of a major version | "Before the first release, the fields of a JSON object can change." |
| 3 | Section 4, row `server list` | `"invalid": [{"name", "error", "causes"}]` |
| 4 | Section 2.4, error block (lines 205-209) | The "after" text of section 9.4. |
| 4 | Section 2.4, lines 212-214 | Keep the meaning. Add that the store error is a cause. |
| 5 | Section 2.1, first item | Old: "shows the correct command, for example 'use `ncp tool call github search_issues`'". New: "shows the correct command in a hint" (section 9.3). |
| 6 | Section 4, error object | The message of the example becomes "The server `github` needs a login or more scopes". |
| 8 | Section 2.1, fourth item | Old: "no tool `search_issue`; did you mean `search_issues`?". New: "The server `github` has no tool `search_issue`", with the hint "Did you mean `search_issues`?". |
| 8 | Section 2.2, line 139 | "The command line has no tool name" |

The new text of the error object in section 4:

> Error object on stderr: `{"error": {"code", "exit", "server", "message",
> "data", "causes", "hints", "destination", "pages"}}`. `data` and
> `causes` are arrays of strings. `hints` is an array of `{"message",
> "command"}`. The three arrays are always present. `server`,
> `destination`, `pages` and `command` are optional. `destination` is the
> origin of a stored value that no place has, or `null` for a stdio value.
> The rules of the text are in [output-messages.md](output-messages.md).

### 10.2 Decisions of cli.md section 9

| Step | ID | New text |
|---|---|---|
| 2 | B3 | A warning on stderr in JSON mode: `{"warning": {"message", "server", "data", "causes", "hints"}}`. A standalone hint: `{"hint": {"message", "server", "command"}}`. `server` and `command` are optional. Text mode: `warning:` and `hint:` lines as uv writes them, with the rules of [output-messages.md](output-messages.md). The message names the server. No prefix names it. |
| 2 | B21 | A parse error of the command line gives the JSON error object in a pipe. The message is "The command line is not valid", and the clap lines up to `Usage:` are one cause. With no command, the message is "The command line has no command". A hint gives `ncp --help`. A clap suggestion gives the hint "Did you mean `X`?". |
| 2 | B56 | Add at the end: "`-q` hides warnings, standalone hints and progress. An error keeps its causes and hints (B74)." |
| 2 | B60 | No color (removed on 2026-10-10, with `--color`). A data line starts at the column of the message text. Each `cause:` line starts with 2 spaces, and its second line with 9. A hint is one line. Only a label or an empty line starts at column 0. Thus a server message cannot start a line that looks like a label line of `ncp`. |
| 2 | B74 | An error can have data lines, causes and more than one hint, in the order to try the hints. Text: the data lines, one `cause:` line for each cause, then one `hint:` line for each hint, each hint after an empty line. JSON: the arrays `data`, `causes` and `hints` in the error object. `-q` keeps them, because they are part of the error. |
| 3 | B76 (2) | Add: "An invalid entry in `invalid` has `error` and `causes`." |
| 4 | B35 | Add: "In JSON, the warning then has `hints: []`." |
| 4 | B38 | "unlock the keyring to list them" becomes a hint of the warning: "To list the locked items, unlock the keyring". |
| 6 | B10 | A `401` or `403` from an HTTP server gives exit 4. The message is "The server `SERVER` needs a login or more scopes". A hint gives `ncp auth login SERVER`. |
| 6 | B78 | An answer `404` to the handshake of an http server gives a hint. The hint says that the server has no MCP endpoint at the path, and that many servers use `/mcp`. The rest of the old text stays. |
| 7 | B70 | Add: "No part of the error (message, data, causes, hints) names a stored URL or issuer." |
| 8 | B7 | When no server is configured, `server list` writes a standalone hint on stderr. The hint names the path and gives `ncp server add`. The rest stays. |
| 8 | B23 | A word in the place of the tool that contains `=`, starts with `{`, or is `-`, gives "The command line has no tool name". |
| 8 | B63 | Add: "The warnings of an elicitation name the server in the message and give the reason as a cause. The elicitation of one request shows each warning one time." |
| 9 | B76 (5) | A project file that `ncp` does not use gives the warning "ncp does not use `FILE`", with the reason as a cause. |

New decision, added in step 2:

| ID | Decision | Reason |
|---|---|---|
| B80 | Errors, warnings and hints obey [output-messages.md](output-messages.md). The labels and the capital letters are as in uv. Each error in the source chain is one `cause:` line. Each fix is a hint sentence. The message names the server. Text and JSON have the same parts. During the migration, output-messages.md section 10 gives the step that changes each older text of this document. Until that step, the older text describes the code. | The user decided to match uv as closely as the rules of `ncp` permit. AGENTS.md makes uv the first reference for command-line behavior. |

### 10.3 Other documents

| Step | Document | Place | Change |
|---|---|---|---|
| 4 | secret-binding.md | Rule 5, error block | The "after" text of section 9.4. |
| 4 | secret-binding.md | Rule 5, "The condition is in the message, and each hint is only a command, because agents run a hint as a command (B3, B10)." | "The message says what is missing. The condition 'If the URL is correct' is in the hint. Each hint is a sentence. A hint that only reads state or starts a login has the JSON field `command`, because agents run a hint command (B3, B10)." |
| 4 | secret-binding.md | Line 229, "tells the user to add the entry first" | "gives a hint that tells the user to add the entry first" |
| 5 | project-config.md | Lines 167 to 173 | "with a hint that gives `ncp trust`", and the same for `ncp trust --ignore`. |
| 5 | project-config.md | Example at line 184 | The "after" text of section 9.8, with only the hint `ncp trust`. |
| 9 | server-add.md | Lines 98 to 100, 167, 175 to 177, 208 to 216, 343 to 345 | Quoted messages with a capital letter and backticks. Hints as sentences. |
| – | plan.md | Lines 158, 201, 204, 235, 300, 428 | No change. They describe milestones that are done. |

### 10.4 MUST requirements that state the content of a message

No requirement text changes. These requirements state what an error must
say. The new text keeps each content in the error, as the table shows.

| Requirement | Content | Place after the change |
|---|---|---|
| A13 | "the message says that the stop is on purpose" | Message (section 9.10). The tests cli.rs:1790 and 1812 keep reading `message`. |
| N6 | the error names the new location of a redirect | Message: "The server `S` answered 308 with a redirect to `URL`. ncp follows no redirect." The redirect is the failure itself, not an inner error, because `connect_failure` reads it from the notes of the HTTP client. Thus rule 6 permits the location in the message. |
| S2 | the error names the opt-in for the credential file | Hint with `--credential-store file`. |
| S8 | a store call with no answer in 60 s stops with an error | Message or cause: "The credential store gave no answer in 60 s". It is the message of `store_failure` when no server is involved. When a call site adds a server, it is a cause (section 7). It is also a cause in E45, E71 and W13. |
| P8, P9 | a clear error for `input_required`, and the pages in the error | Message and data lines, `pages` in JSON (section 9.11). |
| N4 | a bad `SSL_CERT_FILE` stops with an error | The message names the variable and the path. |

## 11. Migration plan

Each step is one or more commits. Each step changes the error types, the
call sites, the tests and the design documents of one area together.
After each step, `mise run precommit` passes. Do the steps in the order of
the list.

1. **Acceptance.** The user accepts this proposal.
2. **Model, renderer and the first area.** Do these tasks:
   - Make the model of section 7.
   - Make one renderer for text and one for JSON.
   - Change each `{error}` of an `ncp` error type in a message to
     `chain_text(&error)`. The Display texts do not change in this step,
     so each message keeps its text. When a later step changes a Display
     text, no message loses a reason.
   - Use `map_text` at each redaction place.
   - Change each struct literal and each field read of `Failure` to the
     methods of section 7. Struct literals: session.rs:384, 461, 774, 863,
     932, 954 and 971, secrets.rs:1148, ask.rs:301, auth.rs:115. Pattern:
     commands.rs:167. Field reads: cli.rs:112, 199 and 267, ask.rs:289
     and 424.
   - Change the places that copy only `failure.message` (section 7).
   - Change `interrupted` (section 7).
   - Give each existing hint its old text and no command.
   - Change the first area: cli.rs (E01 to E05 and the clap JSON of rule
     25) and ui.rs (E116).
   - Add the style check to `Run::error()` and to a new `Run::warnings()`
     in the testkit. Add `Run::old_error()` and `Run::old_warnings()`,
     which skip the check. Each test that reads a diagnostic of the old
     form uses an `old_` method.
   - Add a unit test helper that examines a Display text for rules 3 and
     5. Use it for the error types of the first area.
   - Change the design documents of step 2 (section 10).

   Change these tests in step 2:
   - In the testkit, add `Run::hints()` and `Run::error_text()`, which
     joins the message, the data lines and the causes.
   - Change each `error()["hint"]` check to `hints()` (section 12.1).
   - Change the checks of a separate hint line and of an absent hint
     (section 12.1).
   - Change cli.rs:70-72 (the JSON warning) and the unit tests of `ui.rs`.
   - Add tests for rule 21. One server has `?key=${APIKEY}` and refuses
     the connection. The same server then gives an OAuth discovery
     failure. The tests read the whole stderr, in text and in JSON.
   - Add a test of the reason of E94: `server add` with a URL that is not
     valid shows the reason of the URL.
3. **Configuration and entries.** `ConfigError`, `EntryError`,
   `UrlError`, `ExpandError` and the name errors. Call sites: E07, E08,
   E11, E14, E67, E68, the reason of E90, and the stdout field `invalid`
   (section 6.4). Hint functions for the fixes that the Display texts hold
   today.
4. **Stores and secrets.** `StoreError`, the `security` and Secret
   Service texts, `private_file`. Call sites: E66, E69 to E91, W18 to W25,
   H06 and H07. The OAuth store marks of `oauth/store.rs` use `chain_text`
   (the exception of rule 7).
5. **Load and selection.** E06, E09, E10, E12, E13, W02, W04 and W05 in
   session.rs. `project_entries` gets its indent parameter.
6. **Connect and requests.** `ConnectError`, `RequestError`, the HTTP
   client errors and the proxy route. Call sites: E15 to E30. The causes
   function for transport errors (section 7). `is_not_found` (B78) reads
   the whole source chain, because the Display text of `ConnectError` no
   longer holds the text of rmcp.
7. **OAuth.** `auth.rs`, `oauth/callback.rs`, `oauth/items.rs`: E31 to
   E49 and W08 to W13.
8. **Commands and elicitation.** `tool_args`, `elicit`, commands.rs and
   ask.rs: E50 to E65, E117, W01, W06, W07, W14 to W17 and H01 to H05. The
   warning set of rule 20 in the elicitor.
9. **Project files and edits.** `add_entry`, `config_edit`,
   `project_file`, server_edit.rs and trust.rs: E92 to E115, W03, W26, W27
   and H08.
10. **End.** Remove `Failure::server` and its prefix, so that only `about`
    stays. Remove `Run::old_error()` and `Run::old_warnings()`, so that
    the style check examines each diagnostic. Change the comment of `Failure` in
    `outcome.rs` ("each hint is one command") and the module comment of
    `ui.rs`. Update `.scratch/progress.md`.

Each step from 3 to 9 also does these tasks:

- Change each message of the area from the old form to the new form, in
  one commit. Thus no message shows the server two times between the
  steps.
- Use `about` in place of `server` for the failures of the area.
- Change the tests of the errors of the area (section 12.2).
- Change the checks in other areas that quote a Display text of the types
  of the area.
- Change each test of the area from `old_error()` and `old_warnings()`
  to `error()` and `warnings()`.
- Add unit tests that examine the Display texts of the types of the area
  for rules 3 and 5.
- Change the design documents of the step (section 10).

The style check examines the message and the hints of each diagnostic
that `Run::error()` or `Run::warnings()` returns, for these rules:

- rule 3: the first letter,
- rule 5: no `;`,
- rule 9: no hint two times,
- rule 12: if the object has `server`, the message names it in backticks,
- rule 14: no empty pair of backticks.

The testkit cannot tell `ncp` causes from foreign causes. Thus the unit
tests of the error types examine the causes that `ncp` writes.

## 12. Tests and docs to change

The line numbers are from commit `aa8d48d` (inventory section 14).

### 12.1 Checks of a hint (step 2, then the step of the area)

About 50 checks read `error()["hint"]`. Step 2 changes them to `hints()`.
The step of each area then changes the expected text to a sentence. It
also adds a check of `command` where rule 10 gives one.

- crates/testkit/tests/cli.rs: 210, 228, 237, 240, 249, 274, 290, 296,
  423, 516, 535, 591, 603, 676, 792, 886, 1066, 1114, 1188, 1520-1521,
  1532-1533, 1545, 1733, 1795, 1831, 1876, 2014, 2172, 2361, 2399, 2417, 3024,
  3091, 3202.
- crates/testkit/tests/project.rs: 61, 84, 166, 400, 619.
- crates/testkit/tests/server_edit.rs: 183, 225, 295, 312, 368, 692, 700, 730, 834.
- crates/ncp/src/cli/session.rs unit tests: 1029, 1056.

Step 2 changes these checks of a separate hint line to the `hints` array
of the error or the warning:

- cli.rs: 278 (`ncp secret list` after E70), 594 (`ncp auth login web`),
  1010 (H06 after W21).
- project.rs: 85 (`ncp trust --ignore`).

The checks of an absent hint become true with no change when `ncp` no
longer writes the field `hint`. Thus step 2 changes them to `hints ==
[]`:

- cli.rs: 636 and 2233 (`error.get("hint").is_none()`).
- cli.rs: 1092 (no hint after the failed copy check of B35). The check
  then reads `warning.hints`.
- cli.rs: 2118 (`assert_ne!(error["hint"], "ncp auth login web")`, B51).
  It becomes: no hint has the `command` `ncp auth login web`.

### 12.2 Checks of a message

Exact text (`assert_eq!`):

- cli.rs: 70-72 (W01, JSON warning), 270, 434, 465, 527, 578, 611, 657,
  923, 1041.
- project.rs: 162, 393, 491 and 650 (`invalid[0].error` of `server list`).

`starts_with`, `ends_with` and `contains` checks. Some of them change to
`error_text()`, because their text moves to a cause or a data line:

- cli.rs: 226, 512-514, 559, 650, 672, 703, 711, 713, 896, 1006, 1020,
  1073, 1087, 1117, 1223, 1513, 1516, 1568, 1614, 1873, 2186, 2300, 2325,
  2329, 2413, 2539, 2895, 3040, 3085, 3087.
- project.rs: 57, 60, 124, 256, 270, 284, 371-377, 478, 506-507, 523,
  611, 638.
- server_edit.rs: 203, 633, 682, 936.
- client.rs: 324. security_store.rs: 305.
- Unit tests: auth.rs 1441, 1453, 1471, oauth/callback.rs 242,
  client/http.rs 442.

These checks stay on `message`: cli.rs 1790 and 1812 (A13, "on
purpose"). The checks cli.rs 172-173 (E62) change to `data`, because the
problems and the expected arguments are data lines.

Text form: cli.rs 2582 (`hint: use --output-format json`), and the unit
tests of ui.rs at 456-464, 470-484, 489, 538-553 and 587-594.

These checks do not change:

- the checks of prompts (P01, P02, P09, P11),
- the checks of results (I01, I10, I11),
- the checks of `debug:`, `trace:`, `progress:` and `timings:` lines,
- the checks of exit codes.

### 12.3 Checks of wording that read only `message`

These checks make sure that a text does not show. The text can now be in a
data line, a cause or a hint. Thus each check changes to read the whole
stderr:

- project.rs: 354 (the server name `fs`).
- cli.rs: 1794 (the stored host, B70), 2120 ("needs a login"), 2235
  (the mark `local:`).

The leak checks of secret values already read the whole stderr: cli.rs
941-958, 1261, 1312, 1715 and 2251. No test has a secret in a cause yet,
because no cause exists yet. Step 2 adds those tests.

### 12.4 Docs that quote a message

| Place | Today | Change |
|---|---|---|
| cli.md:113 | "use `ncp tool call github search_issues`" | Section 10.1 |
| cli.md:122-123 | "no tool `search_issue`; did you mean `search_issues`?" | Section 10.1 |
| cli.md:139, 441 | "missing tool name" | "The command line has no tool name" |
| cli.md:146 | `-q` row | Section 10.1 |
| cli.md:205-209 | error block of E70 | Section 9.4 |
| cli.md:212-214 | "the login keyring is locked" | Section 10.1 |
| cli.md:313-317 | error object | Section 10.1 |
| cli.md:425, 428, 439, 453, 456, 477, 480, 488, 492, 494, 496 | B7, B10, B21, B35, B38, B60, B63, B70, B74, B76, B78 | Section 10.2 |
| project-config.md:167-173, 184-186 | trust errors | Section 10.3 |
| secret-binding.md:209-223, 229 | E70 block, rule 5, E77 | Section 10.3 |
| server-add.md:98-100, 167-168, 175-177, 208-216, 343-345 | E94, add_entry.rs:93, E85, outcomes table, E109 | Section 10.3 |

## 13. Risks

| Risk | Effect | Control |
|---|---|---|
| A script or an agent reads `error.hint`. | It gets nothing. | No release exists. The field `command` gives a command that reads state as data (rule 10). |
| An agent needs a command that changes state, for example `ncp trust`, `ncp secret set` or `ncp server remove`. | Today the agent gets it as a bare command. After the change, it must read the command from the sentence. | Rule 10 is the safe default. A change to this policy needs a separate proposal. |
| A secret shows in a data line, a cause or a hint. | A leak (S3). | Private fields and `map_text` in step 2. Rule 21. New tests read the whole stderr. |
| The redaction of today does not hide a `${NAME}` value of a header or of `env` in a body that a server echoes. | A leak in the text of a cause. | The same is true today in the message. A wider redaction needs a separate proposal, because it changes the handling of secrets. |
| A message does not name its server after `ncp` no longer writes the prefix. | The user cannot see which server failed. | The style check (rule 12) fails the test. |
| An rmcp or reqwest update changes a cause text. | A test of library text fails. | Tests examine the text of `ncp` and only a short part of library text, for example `HTTP 404`. |
| B78 reads `HTTP 404` from the Display text of the error. | The check fails when the Display text no longer holds the chain. | `is_not_found` reads the whole source chain. The test with a local `404` server stays. |
| A value from a server has a backtick. | The spans of the message do not pair. | Rule 14 shows such a value in double quotes with escapes. `printable` (B15) does not escape a backtick. |
| A body of a server or a list of pages is long. | Hints go out of view. | Rule 16 cuts a cause and the data lines at 20 lines or 2048 bytes. |
| About 150 checks change. | Review is slow, and a wrong text can pass. | Steps by area, the style check from step 2, and the hint sentences of section 5.1. |

## 14. Place in the plan

This work is milestone 20 of [plan.md](../plan.md). It comes after
milestone 19 and before the distribution, which is always the last
milestone.

## 15. Differences from uv

| Item | uv 0.13.0 | `ncp` | Reason |
|---|---|---|---|
| Message form | "Failed to VERB OBJECT", with no subject | A full sentence with a subject | ASD-STE100 rule 4.2: no omitted words |
| Article before a kind word | Mostly "Package `x`" | "The server `x`" | ASD-STE100 rule 4.5 |
| Semicolons | Permitted | Not permitted | ASD-STE100 rule 8.1 |
| Fix in the message | Four forms | Only a hint | One place for each fix, and JSON has `hints` |
| "Did you mean `X`?" | On the message line | A hint | A suggestion is a fix, and each fix is a hint |
| Value after a final colon | `MESSAGE: value`, with no backticks | Not used. The value is in the sentence, in backticks. | One rule for each value, and clear for an agent |
| A value with a backtick or a control character | In backticks, as it is | In double quotes, with the escapes of Rust `Debug` | Backtick spans stay in pairs. A new line cannot start a line. |
| Repeated cause text | Shown | Not shown | rmcp and reqwest repeat the inner text |
| Data lines of the author | The indent of the author | 7 spaces (error) or 9 (warning) | B60: a line from a server cannot start at column 0 |
| A new line in a hint | Column 0 | Not possible: a hint is one line | Hints are `ncp` text, and rule 14 quotes a value with a new line |
| Long cause or data | Shown in full | Cut at 20 lines or 2048 bytes, with a mark | Hints stay in view |
| Line wrap on a terminal | Messages and causes | None | Copy of a long command, and `grep` of a whole line. A wrap is optional later work. |
| Color | Labels in color | No color | B60 |
| JSON errors | None | JSON Lines in a pipe, with `data`, `causes`, `hints` and `command` | Requirement I1 and cli.md section 4 |
| clap errors in JSON | No JSON | An `ncp` message, the clap lines as a cause, and a hint for a clap suggestion | Rules 15 and 25 |
| Repeated warning | Only `_once` warnings show one time | Each warning of an elicitation shows one time | B63: up to 8 rounds of one request |
| `-qq` | No output, also no error | The same as `-q` | An agent reads the code, the server and the hints of the JSON error. The exit code alone does not tell which server or which fix. |
| Exit codes | 1 and 2 | 1 to 5 and 130 | Requirement I3. Not in the scope of this change. clap usage errors give 2 in both. |
