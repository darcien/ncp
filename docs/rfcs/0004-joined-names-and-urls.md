---
state: committed
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

This RFC calls the word in the place of a server name the server word.

Before this RFC, `session::split_joined` split a server word at its
first `.`, `:` or `/`, when that character had text on both sides. This
check came before the lookup. Its error, the joined error, was "`X`
joins a server name and a tool name". `server show`, `tool`, `resource`
and `prompt` did this check, and `tool call` had a second copy. The
other commands did only the lookup.

## Problems

The global file has the server `loc` with the URL
`http://127.0.0.1:9/mcp`, and the entry
`io.github.github/github-mcp-server`. Before this RFC, the test build
`ncp-cli` gave:

| # | Input | Output | Problem |
|---|---|---|---|
| 1 | `ncp tool list https://mcp.example/mcp` | The joined error. Hint: `ncp tool show https //mcp.example/mcp` | The error is false, and the hint fails. |
| 2 | `ncp server show loc.search`, also `resource list` and `prompt list` | The joined error. Hint: `ncp tool show loc search` | The command is not about a tool. |
| 3 | `ncp prompt get loc.review x` | The joined error. Hint: `ncp tool show loc review` | The word is a prompt name. |
| 4 | `ncp tool list io.github.github/github-mcp-server` | The joined error | The check hides the error of the entry. |
| 5 | `ncp tool show nope.search` | The joined error. Hint: `ncp tool show nope search` | No server `nope` exists, so the hint fails. |
| 6 | `ncp tool call loc.search query` | The joined error. Hint: `ncp tool call loc search query` | The hint fails: `query` is not `KEY=VALUE`. |
| 7 | `ncp server add https://x.example/mcp foo` | "ncp cannot add the server `https://x.example/mcp`" | No hint says that the name comes first. |
| 8 | `ncp prompt get loc` | The error of clap: "the following required arguments were not provided: <PROMPT>" | No hint names `ncp prompt list loc`. `ncp tool show loc` gives the hint `ncp tool list loc`. |
| 9 | `ncp tool call nope` | "The command line has no tool name". Hint: `ncp tool list nope` | No server `nope` exists, so the hint fails. `ncp tool show nope` gives "No server `nope` is configured". |

## Determinations

"The known names" are the names that `ncp server list` shows. It shows
the valid entries and the entries that are not valid, also those of an
untrusted or ignored project file.

1. **Lookup first.** Each command that uses an entry looks up the whole
   server word first. If the lookup finds an entry, the command uses
   that entry or gives its error, and ncp does not split the word. Thus
   problem 4 shows the error of the entry. The lookup also finds an
   entry of an untrusted or ignored project file. Then ncp asks the
   trust question or gives the error of that project file, as before
   this RFC.
2. **A URL.** A server word that starts with `http://` or `https://`, in
   any case, is a URL. `split_joined` never splits it. If the lookup
   does not find it, the error is "No server `X` is configured ...".
   The only hint is "To see the configured servers, run
   `ncp server list`", with no nearest name. The message shows the word
   as the user gave it. ncp adds no value to the known secrets for the
   word, because ncp does not compare the word with the URLs of the
   entries.
3. **A joined word.** A joined word is a server word that is not a URL,
   and the lookup does not find it. `split_joined` splits it, and its
   first part is a known name. In the examples, this name is `loc`. The
   commands other than those of determination 4 give "No server `X` is
   configured ...". The first hint is "Did you mean `loc`?", in place of
   the nearest name.
4. **ncp runs a joined word as the two words** in `tool show`,
   `tool call` and `prompt get`. It does this when the next word is
   missing or is an argument of the command. An argument of `tool call`
   is a word with `=`, a word that starts with `{`, or `-`. After the
   prompt name, an argument of `prompt get` is a word with `=`.
   `tool show` takes no argument.

   `tool call loc.search q=x` runs as `tool call loc search q=x`.
   `prompt get loc.review k=v` runs as `prompt get loc review k=v`.
   `prompt get loc.a=b` gets the prompt `a=b`, as `prompt get loc a=b`
   does (determination 9). The server part gets the lookup, the trust
   question and the checks of the entry. The help of the server argument
   of `tool show` and `tool call` is "The server name from the
   configuration. In front of a tool name, join them with ., : or /."
   The help of `prompt get` says "a prompt name" in place of "a tool
   name". The README shows one example.

   With another next word, ncp cannot know which name the user wants,
   because that word can also be a name. The error is "`X` joins a
   server name and a tool name" ("a prompt name" for `prompt get`). The
   hint is "To see the tools of the server, run `ncp tool list loc`" (or
   the prompts).
5. **Other words.** The error is "No server `X` is configured ...", with
   the nearest name and `ncp server list`, as before this RFC. Thus
   problem 5 gets "No server `nope.search` is configured". The nearest
   name now comes from the known names. Before this RFC, it came only
   from the global file and a trusted project file.
6. **Order.** `tool show`, `tool call` and `prompt get` look up the
   server word first. Then they check the tool word or the prompt word,
   and the arguments. Thus the hint of a usage error names a server that
   exists (problem 9). A joined word that ncp runs as the two words
   gives the same output, in the same order, as the two words. The
   lookup can ask the trust question before a usage error.

   `tool call --help` with no tool word shows the help before the load
   of the configuration, as before this RFC. The exception is a server
   word that `split_joined` splits. ncp loads the configuration for such
   a word first. If the word is joined (determination 3), ncp runs it as
   the two words with `--help`. Thus `tool call loc.search --help` shows
   the tool, as `tool call loc search --help` does, and
   `tool call loc.a=b --help` gives the error of
   `tool call loc a=b --help`. In the other cases, also when the lookup
   finds the whole word, it shows the help. If the whole word is an
   entry of an untrusted project file, the load can ask the trust
   question first. A load error, for example of a configuration file
   that is not valid, comes first.
7. **One place.** The not-found branch of `session::select` calls one
   pure function for determinations 2 to 5. Its input is the server
   word, the kind of command, the next word and the loaded
   configuration. Its output is the failure, or the two names to run.
   `session::load` calls it too, to ask the trust question for the
   server part of a joined word that runs. A joined word that ncp does
   not run asks no trust question. The build removes the second check of
   `tool call`.
8. **`server add` with the URL first.** If the server word is a URL that
   `server add` accepts, and the next word is a valid server name `foo`,
   ncp adds a hint. The hint is "If `foo` is the server name, give it
   before the URL". It is the first hint of the error "ncp cannot add
   the server `X`". The hint has no command, because a command without
   an option such as `--global` can write to another file.
9. **One grammar for a name after the server.** The prompt word is
   optional in the grammar, as the tool word of `tool show` and
   `tool call` is. It is optional so that ncp gives its own error, and a
   joined word can run with no next word. The usage line shows
   `[PROMPT]`, as the usage line of `tool show` shows `[TOOL]`. If ncp
   does not run the server word as a joined word, the prompt word is the
   prompt name, as before this RFC. This includes a word with `=`, a
   word that starts with `{`, and `-`. Only a missing prompt word gives
   "The command line has no prompt name". The hint is "To see the
   prompts of the server, run `ncp prompt list loc`".

   MCP 2026-07-28 and 2025-11-25 give a prompt name no rule for its
   characters (server/prompts, "Data Types"). For an "Invalid prompt
   name", a server SHOULD return `-32602` (server/prompts, "Error
   Handling"). ncp then gives "The server `loc` refused `prompts/get`",
   with the same hint. MCP 2026-07-28 and 2025-11-25 say that a tool
   name SHOULD have only letters, digits, `_`, `-` and `.`
   (server/tools, "Tool Names"). Thus a tool word with `=` stays a usage
   error.

## Alternatives

The reasons use the order of choice in `docs/command-development.md`.

### The joined word in `tool show`, `tool call` and `prompt get`

- **An error with the two-word command as a hint.** This is nearest to
  the behavior before this RFC, and it is simpler: one form in the help,
  less code and fewer tests. It is as secure as determination 4. A
  server name has no `.`, `:` or `/`, so the split is unique, and the
  server part gets the same checks. But it needs one more run, and the
  hint of `tool call` has no `command` field, so an agent must type the
  command again. uv and mise take a joined word next to a second form,
  and show it in the help. uv has
  `uv tool run ruff@0.3.0` or `--from` (`uv help tool run`, uv 0.13.7).
  mise has `mise ls-remote TOOL@VERSION` or `TOOL PREFIX`
  (`mise ls-remote --help`, mise 2026.11.4). Ease of use and their
  practice come before the simplest design.
- **Run the joined word with no help text.** A reader of `loc.search` in
  a script cannot find the form.
- **Remove the joined error.** "Did you mean `loc`?" for each joined
  word loses the tool name. `tool call loc.search q=x` then leads to
  `tool call loc q=x`.
- **A hint for `mcp__web__search`.** A server name can have `__`, so the
  split is not unique.

### The joined word in other commands

- **The joined error.** These commands take no tool name or prompt name.
  "Did you mean `loc`?" gives its fix, with no false error.

### The URL

- **Hints for a URL** ("Did you mean" for an entry with the URL, or a
  hint to add the server). To name the right entry, ncp must also
  compare the URLs of entries that do not run. Another entry of the same
  name can hide such an entry, and its URL can hold `${NAME}` values.
  For each rule for these entries, the reviews found a case with a bad
  hint. The hint led to a second entry or to the wrong entry, or showed
  a secret. The plain error is true and safe. Secure by default comes
  before ease of use.
- **Use the server that has the URL.** The entry selects the headers and
  the OAuth client. A project file can add a second entry with the same
  URL, and the same command then selects another entry.
- **A new error for a URL.** The "not configured" error is true.
- **Connect to a URL with no entry.** This needs new rules for trust and
  secrets, and thus its own RFC.

### The order

- **Check the usage before the lookup for a server word with no `.`, `:`
  or `/`.** The hint of a usage error can then name a server that does
  not exist (problem 9). The two forms of one command then give their
  errors in different orders.

### The prompt word

- **Keep `<PROMPT>` required in the grammar.** This is simpler. But clap
  then gives its error with no hint (problem 8), and
  `prompt get loc.review` cannot run.
- **Refuse a prompt word with `=`, as a tool word.** MCP permits such a
  prompt name (determination 9), so a server can have the prompt `a=b`.
  ncp then cannot get it.
  Before this RFC, `prompt get loc a=b` sent `a=b` as the prompt name.
  Thus the refusal is a breaking change under the version rule. The MCP
  spec comes first in the order of choice.

### `server add` with the URL first

- **A hint with the full command.** ncp must write each option again,
  and can lose one.

## Tests

A behavior test for each problem row, in the text and the JSON output.
In these tests, `pj` is a server of an untrusted project file. Also:

- A URL gets "No server `X` is configured ..." with only the hint
  `ncp server list`. This includes the URL of the server `loc`.
- `tool list HTTPS://x.example/mcp` gets "No server
  `HTTPS://x.example/mcp` is configured ..." with only the hint
  `ncp server list`, as the lowercase URL does.
- An unknown word near the name of an entry of an untrusted project file
  gets "Did you mean".
- `tool call pj.search q=x` asks "Trust this file?" on a TTY.
- `server show pj.search` and `tool show pj.search extra` ask no trust
  question on a TTY.
- `tool call loc.search -` and `tool call loc.search '{"q":"x"}'` run as
  the two words.
- `tool call nope.search --help` gives the load error for a
  configuration file that is not valid, and the help of `tool call` for
  a valid one.
- `tool call https://x.example/mcp --help` shows the help of
  `tool call`, also when the configuration file is not valid.
- `tool call loc.search --help` shows the same output as
  `tool show loc search`.
- `tool call` with `--help` and the key of an entry, with a known name
  as the first part of the key, shows the help.
- `tool show loc.search` runs as `tool show loc search`.
- `tool show loc.search extra` gives the hint `ncp tool list loc`.
- `prompt get loc.review` runs as `prompt get loc review`.
- `prompt get loc.review k=v` runs as `prompt get loc review k=v`.
- `prompt get loc.a=b` gets the prompt `a=b`, as `prompt get loc a=b`
  does.
- `prompt get loc -` and `prompt get loc '{"a":1}'` send `-` and
  `{"a":1}` as the prompt name.
- `prompt get loc.review -` and `prompt get loc.review '{"a":1}'` give
  "`loc.review` joins a server name and a prompt name".
- `tool call loc.a=b` gives "The command line has no tool name", as
  `tool call loc a=b` does.
- `tool call loc.a=b --help` gives "The command line has no tool name",
  as `tool call loc a=b --help` does.
- `prompt get --help` shows `[PROMPT]`.
- `prompt get nope`, `prompt get nope review bad` and
  `tool call nope search bad` give "No server `nope` is configured".
- `tool call pj q=x` asks "Trust this file?" on a TTY before the usage
  error.

In `crates/testkit/tests/cli.rs`, the test
`joined_server_and_tool_name_gives_the_two_word_command` gets the name
`joined_server_and_tool_name_runs_as_two_words`. It checks the joined
call with a stub server.
`argument_in_place_of_the_tool_gives_missing_tool_name` adds the server
`github`, because the lookup now comes first.

## Compatibility

The commands, the options, the files, the JSON keys, and the meaning of
exit 2 and of the code `usage` do not change. Messages, hints, and the
help and the usage line of `prompt get` change. A joined word that gave
an error now runs. `prompt get` with no prompt name gives the error of
ncp in place of the error of clap. Some inputs gave a usage error or
the help of `tool call`. Now they give first the load error, an error
of the lookup or the trust question. For example,
`prompt get nope review bad` now gives "No server `nope` is configured".
`tool call SERVER.TOOL --help`, with a known SERVER, showed the help of
`tool call`. Now it gives the output of `tool call SERVER TOOL --help`.
For a tool name, it shows the tool and connects to the server. The hint
"Did you mean" can name an entry of an untrusted or ignored project
file. These are not breaking changes.

The commit of this RFC changes determination 4 of RFC 0005. There, the
usage line of `prompt get` gets `[PROMPT_NAME]` in place of
`<PROMPT_NAME>`. This agrees with `[TOOL]`, which RFC 0005 keeps for
`tool call`.
