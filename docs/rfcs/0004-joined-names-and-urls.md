---
state: accepted
supersedes: []
---

# 0004: Joined names and URLs in place of a server name

## Background

A server name has only letters, digits, `-` and `_`. A user or an agent
can type another word in its place:

- A server name and a tool name in one word: `web.search`, `web:search`
  or `web/search`.
- The URL of the server.
- The key of an entry from another client, for example
  `io.github.github/github-mcp-server`.

Today `session::split_joined` splits a word at the first `.`, `:` or `/`
that has text on both sides, before the lookup. The error is "`X` joins
a server name and a tool name". `server show`, `tool`, `resource` and
`prompt` do this check, and `tool call` has a second copy. The other
commands do only the lookup.

## Problems

The global file has the server `loc` with the URL
`http://127.0.0.1:9/mcp`, and the entry
`io.github.github/github-mcp-server`. The test build `ncp-cli` gives:

| # | Input | Output | Problem |
|---|---|---|---|
| 1 | `ncp tool list https://mcp.example/mcp` | The joined error. Hint: `ncp tool show https //mcp.example/mcp` | The error is false, and the hint fails. |
| 2 | `ncp server show loc.search`, also `resource list` and `prompt list` | The joined error. Hint: `ncp tool show loc search` | The command is not about a tool. |
| 3 | `ncp prompt get loc.review x` | The joined error. Hint: `ncp tool show loc review` | The word is a prompt name. |
| 4 | `ncp tool list io.github.github/github-mcp-server` | The joined error | The check hides the error of the entry. |
| 5 | `ncp tool list http://127.0.0.1:9/mcp` | The joined error | The server `loc` has this URL. |
| 6 | `ncp tool show nope.search` | The joined error. Hint: `ncp tool show nope search` | No server `nope` exists, so the hint fails. |
| 7 | `ncp tool call loc.search query` | The joined error. Hint: `ncp tool call loc search query` | The hint fails: `query` is not `KEY=VALUE`. |
| 8 | `ncp server remove https://x.example/mcp`, also `auth login` | "No server ... is configured". Hint: `ncp server list` | The hint does not help. |
| 9 | `ncp server add https://x.example/mcp foo` | "ncp cannot add the server `https://x.example/mcp`" | No hint says that the name comes first. |

## Determinations

"The known names" are the names that `ncp server list` shows: valid
entries, entries that are not valid, and entries of a project file that
is not trusted or that ncp ignores.

1. **Lookup first.** Each command looks up the whole word first. If the
   lookup finds an entry, the result does not change. Thus problem 4
   shows the error of the entry.
2. **A URL.** A word that starts with `http://` or `https://`, in any
   case, is a URL. ncp never splits it. The error stays "No server `X`
   is configured ...". ncp adds one of these hints:
   1. If configured HTTP servers have the URL `X`: "Did you mean
      `loc`?", in the order of `ncp server list`. ncp compares the
      parsed URLs with no query and no fragment, so `/mcp` and `/mcp/`
      differ. A configured HTTP server is a valid `http` entry of the
      global file or a trusted project file. ncp adds the `${NAME}`
      values of each matched URL to the known secrets before it shows
      the error.
   2. Else, if `ncp server add` accepts `X`, and the command is not
      `server remove`, `auth logout` or `secret delete --server`: "To
      add the server to the global file, run
      `ncp server add --global <SERVER> X`". With `--config-file`: "To
      add the server to `F`, run
      `ncp --config-file F server add <SERVER> X`", as the hint of
      `server remove` does. `F` and `X` get shell quotes when they need
      them. The hint has no `command` field. It names the global file, so
      that a token in the URL does not go to a shared project file by
      default.

   The hint `ncp server list` comes last. A URL gets no nearest name.
3. **A joined word.** A word is joined when it is not a URL,
   `split_joined` splits it, and the first part is a known name, for
   example `loc`. If the lookup finds nothing, each command other than
   those of determination 4 gives "No server `X` is configured ...",
   with the first hint "Did you mean `loc`?" in place of the nearest
   name.
4. **ncp runs a joined word as two words** in `tool show`, `tool call`
   and `prompt get`, when the next word is:
   - `tool show`: missing.
   - `tool call`: missing, or an argument (a word with `=`, a word that
     starts with `{`, or `-`).
   - `prompt get`: an argument (a word with `=`).

   `ncp tool call loc.search q=x` runs as `ncp tool call loc search q=x`,
   with the same output. The server part gets the lookup, the trust
   question and the checks of the entry. The help of the server argument
   names the form: "In front of a tool name, join them with `.`, `:` or
   `/`" (for `prompt get`: "In front of a prompt name and its
   arguments"). The README shows one example.

   With another next word, ncp cannot know which name the user wants.
   The error is "`X` joins a server name and a tool name" ("a prompt
   name" for `prompt get`), with the hint "To see the tools of the
   server, run `ncp tool list loc`" (or the prompts).
5. **Other words.** No change: "No server `X` is configured ...", the
   nearest known name, and `ncp server list`. Thus problem 6 gets "No
   server `nope.search` is configured".
6. **Order in `tool call`.** For a URL or a server word with `.`, `:` or
   `/`, `tool call` loads the configuration and decides on the server
   word before it reads the tool word. For `--help` with no tool word, a
   joined word loads the configuration first. A known server part shows
   the tool, as `tool call loc search --help` does. An unknown one shows
   the help. A load error comes first. A URL and each other word show the
   help before any load, as today.
7. **One place.** The not-found branch of `session::select` calls one
   pure function for determinations 2 to 5. Its input is the word, the
   kind of command, the next words and the loaded configuration. Its
   output is the failure and the matched servers of 2.1, or the two names
   to run. `session::load` calls it too, to ask the trust question for
   the server part. The second check of `tool call` goes away.
8. **`server add` with the URL first.** If the name word is a URL that
   `server add` accepts, and the next word is a valid server name `foo`,
   ncp adds the hint "If `foo` is the server name, give it before the
   URL". The hint has no command, because a command that drops an option
   such as `--global` can write to another file.

## Alternatives

The order of choice: the MCP spec, secure by default, ease of use, the
practice of uv and mise, the simplest design.

### The joined word in `tool show`, `tool call` and `prompt get`

- **An error with the two-word command as a hint.** This is nearest to
  today, and it is simpler: one form in the help, less code and fewer
  tests. It is as secure as determination 4: a server name has no `.`,
  `:` or `/`, so the split is unique, and the server part gets the same
  checks. But it costs a run, and the hint of `tool call` has no
  `command` field, so an agent must type the command again. uv and mise
  take a joined word next to a second form, and show it in the help:
  `uv tool run ruff@0.3.0` or `--from` (`uv help tool run`, uv 0.13.7),
  `mise ls-remote TOOL@VERSION` or `TOOL PREFIX`
  (`mise ls-remote --help`, mise 2026.11.3). Ease of use and their
  practice come before the simplest design.
- **Run the joined word with no help text.** A reader of `loc.search` in
  a script cannot find the form.
- **Remove the joined error.** "Did you mean `loc`?" for each word drops
  the tool name. `tool call loc.search q=x` then leads to
  `tool call loc q=x`.
- **A hint for `mcp__web__search`.** A server name can have `__`, so the
  split is not unique.

### The joined word in other commands

- **The joined error.** A list command has no next name. "Did you mean
  `loc`?" gives its fix, with no false error.

### The URL

- **Use the server that has the URL.** The entry selects the headers and
  the OAuth client. A project file can add a second entry with the same
  URL, and the same command then selects another entry.
- **A new error for a URL.** The "not configured" error is true. A hint
  gives the fix.
- **Connect to a URL with no entry.** New rules for trust and secrets.
  It needs its own RFC.
- **An add hint with no file option.** `server add` then writes to the
  project file. ncp does not detect a token in a URL
  (`docs/command-development.md`, "No detection").
- **No add hint.** Simpler, but the user must find the next step in
  `ncp server add --help`.

### `server add` with the URL first

- **A hint with the full command.** ncp must write each option again,
  and can lose one.

## Tests

A behavior test for each problem row, in the text and the JSON output.
Also:

- A URL that matches an entry with `${NAME}` in its URL shows `****`.
- `tool call loc.search q=x` asks "Trust this file?" on a TTY, for a
  project file that is not trusted.
- `tool call nope.search --help` with a configuration file that is not
  valid gives the error of the file.
- `tool show loc.search extra` gives the hint `ncp tool list loc`.

In `crates/testkit/tests/cli.rs`,
`joined_server_and_tool_name_gives_the_two_word_command` has no server
`github`. It adds one, and checks the call with a stub server.

## Compatibility

The commands, the options, the files, the JSON keys, exit 2 and the code
`usage` do not change. Messages and hints change. A joined word that
gave an error now runs. This is not a breaking change.

If RFC 0005 is accepted, `<SERVER>` placeholder might change.
