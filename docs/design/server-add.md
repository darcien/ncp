# Proposal: `ncp server add` and `ncp server remove`

Status: proposal v6 (2026-10-11). Two review rounds (adversarial and grug)
on v2 and v3. Not accepted: the user must accept it before the build
([plan.md](../plan.md) milestone 19, gate). Section 8 lists the decisions.

Version 6 closes the open point of version 5: `--env NAME` shows the other
stdio entries that use NAME. A stdio value has no destination (S9), so all
stdio entries with `${NAME}` share one item. The `secrets` row of the
result (section 4.6) lists them in `servers`, as `secret list` does.

Sources of truth that this proposal follows, and does not repeat:
- Target file, `--global`, creation of `.mcp.json`, trust before a write,
  and `server remove` of the effective entry:
  docs/design/project-config.md rule 9 and decision G (milestone 18).
- Secrets bound to an origin: requirement S9, docs/design/secret-binding.md,
  B72 (milestone 17, done).
- Credential headers: requirement S10 (RFC 9110 only).
- JSONC read: plan milestone 18 item 1.

## 1. Problem

Server docs tell users to run `npx add-mcp <url>` or `claude mcp add ...`.
With `ncp`, the user edits `~/.config/ncp/mcp.json` by hand, then runs
`ncp secret set` and `ncp auth login`. A user or an agent can miss a step.
The user asked for one flow: entry, secrets and login in one step.

## 2. Prior art

docs/research/add-command.md §1-2 (Claude Code, Codex, Gemini CLI, gh,
add-mcp). Common parts: a name first, `-- <command>` for stdio, repeatable
env and header flags, `remove <name>`.

## 3. Command

```
ncp [OPTIONS] server add <SERVER> <URL> [--global] [--header 'NAME: VALUE']... [--secret <NAME> [--credential-store <native|file>]]
ncp [OPTIONS] server add <SERVER> [--global] [--env NAME | --env NAME=VALUE]... [--secret <NAME> [--credential-store <native|file>]] -- <COMMAND> [ARG...]
ncp [OPTIONS] server remove <SERVER> [--global]
```

The `--help` of `server add` shows two forms for a secret: the `--secret`
form and the two-command form. One line says that a pipe also works, and one
line says why the single quotes are there: the shell must keep `${NAME}`.
One line says that a stored value is shared only by entries with the same
origin (S9). The help shows `--env NAME` first, and says that
`--env NAME=VALUE` writes VALUE into the file as it is.

```
ncp server add cf-docs https://docs.mcp.cloudflare.com/mcp
ncp server add cf https://mcp.cloudflare.com/mcp
ncp server add acme https://mcp.acme.dev/mcp --header 'X-Tenant: acme'

# A secret in one command: ncp asks for LINEAR_KEY (hidden), stores it,
# and the file keeps ${LINEAR_KEY}. A pipe gives the value too.
ncp server add linear https://mcp.linear.app/mcp \
  --header 'Authorization: Bearer ${LINEAR_KEY}' --secret LINEAR_KEY

# The same in two commands: the entry first (B72), then the value.
ncp server add linear https://mcp.linear.app/mcp --header 'Authorization: Bearer ${LINEAR_KEY}'
ncp secret set LINEAR_KEY

# A stdio server.
ncp server add github --env GITHUB_TOKEN --secret GITHUB_TOKEN -- github-mcp-server stdio
```

## 4. Rules of `server add`

### 4.1 Entry

1. **Transport by form.** An `ncp` rule, with no guess: one word after the
   name is the URL of an `http` entry; words after `--` are the command of a
   `stdio` entry. Any other form is a usage error that shows both forms. No
   `--transport`: `sse` is not a choice, because `ncp` cannot connect to it
   (P5). The file always gets `"type"`.
2. **Same validation as the loader.** The command builds the entry and runs
   it through the domain parser of the configuration. No second set of
   rules.
3. **Used name.** The same entry (equal after the parse) is already in the
   file: exit 0, no change, so that a retry after a failure works. Another
   entry with the name: usage error with the hint `ncp server remove NAME`.
4. **`--env NAME`** writes `"NAME": "${NAME}"`. Reason: the short form is
   also the safe form (Docker `-e NAME`; Codex takes only a variable name for
   a bearer token). `--env NAME=VALUE` writes the value as given.
5. **Credential header: exact template.** A `--header` with the name
   `Authorization` or `Proxy-Authorization` (requirement S10: RFC 9110
   only) must have the value
   `${NAME}` or `<scheme> ${NAME}` and nothing more. Else it is a usage
   error. The error names the header, never the value (a custom value
   parser, so that clap cannot echo it). The hint gives the exact fix with
   `--secret NAME` and single quotes. Reason: RFC 9110 §11.6.2 defines the
   `Authorization` field as credentials (§11.7.2 `Proxy-Authorization`
   too), and B43 already treats these names as secrets by fact. This is not
   a guess from a value. All other headers and `--env` values can hold a
   literal, with no warning: no test can tell a secret from another value
   (user, 2026-10-10).
6. **Empty value: usage error** (`--env K=`, `--header 'K:'`). It catches an
   unset `$NAME` in double quotes.
7. **All usage checks first.** Rules 1 to 6, 12, 13 and the checks of the
   flags run before stdin is read. Thus a piped value
   is not read and lost.

### 4.2 No contact with the server at add (settled, from prior art)

Prior art (checked 2026-10-10):
- Claude Code: add saves the configuration with no validation; the user
  checks with `claude mcp get <name>` or `/mcp`.
- Gemini CLI: no connection at add (source).
- Codex: writes the entry first, then one OAuth discovery request for a
  URL entry, only to decide if it starts a login. A failed discovery prints
  "may or may not require login" and keeps the entry.
- No vendor refuses to write when the server cannot be reached.

Decision (2026-10-10, prior art answers it): `server add` does not contact
the server.
The result lists `ncp auth login NAME` as a step when the entry has no
credential header ("if the server needs a login"), and `ncp server show
NAME` to check the connection. N1 stays as it is.

8. **No start of a `stdio` command** in any option: `npx -y pkg` would
   download and run code at a config write, and a slow start would cache
   the `initialize` protocol version (C3).

Later improvement (backlog, with `--login`), from v4:
- Codex style: write first, then one request to the typed URL with no
  stored value; `401` with `WWW-Authenticate: Bearer` makes the login step
  exact. No metadata fetch, so no other host gets a request.
- v4 style: the same request before the write; a failure writes nothing.
  Outcomes per status class: an answer or `401`/`403` with a `${NAME}`
  header left out = reachable; `401` with Bearer = login needed; other
  `401`/`403` = exit 4 (B10); other `4xx`, `5xx`, DNS, TLS, timeout = exit 3;
  `3xx` = exit 3 with the hint of B69. Needs new N1 text.

### 4.3 Secrets

Scope of a secret: requirement S9 and B72. The store keeps one item for
each name and destination (account `secret/ORIGIN/NAME`; `secret/NAME`
for a stdio value). A value goes only to the origin of its entry, with no
redirects (N6).

11. **`--secret NAME`, once per command.** `ncp` reads the value with the
    code of `secret set`: the store check before the read (B39), no new
    line at the end (B29), an empty value is a usage error,
    `--credential-store` gives the credential store. When stdin and stderr are a TTY,
    it asks on stderr with the name and the host ("Value of LINEAR_KEY for
    mcp.linear.app:") and hides the input. When stdin is not a TTY, it
    reads stdin to its end. A TTY stdin with a stderr that is not a TTY is
    a usage error with the hint to pipe the value. Both reads have the
    `--timeout` limit. Reason: the principle "a command reads stdin only
    when an argument says so" holds with no new exception; Claude Code
    `--client-secret` and `gh secret set` do the same. One flag and one
    value need no rule for pipes; a second secret goes to `nextSteps`.
12. **Must be used.** NAME must be in a `${NAME}` of an `env` or `headers`
    value of the new entry, else usage error. It catches a typo in the
    name, and a value that cannot expand (section 2.4) does not go to the
    store. The error does not echo the word after `--secret`, because a
    user can type the value there by mistake: "the name after --secret is
    not a ${NAME} of this entry".
13. **No silent change of a shared value.** When a credential store already
    has the item of NAME for the destination of the new entry, with another
    value, `--secret NAME` is a usage error: "NAME is already stored for
    DESTINATION in STORE; other entries with this origin use it". Hint:
    `ncp secret set NAME --server SERVER` replaces it. The same value
    passes (a retry after a failure). A store that cannot answer (locked
    store) gives exit 4: `ncp` does not treat it as "no value".
14. **`--credential-store` needs `--secret`**, else usage error (one job for
    each flag).
15. **No question without `--secret`.** Each other `${NAME}` that no store
    has goes into `nextSteps` (4.6).
16. **Order:** usage checks, read the value, store, write the entry.
    A store failure writes no entry (exit 4, hint `--credential-store
    file`). An entry write that fails after the store keeps the secret;
    the same command then passes rules 3 and 13.

### 4.4 Login

17. Login hint (user, 2026-10-10): `server add` starts no login; `nextSteps`
    gives `ncp auth login NAME`, so the user does not have to find the
    command. A later `--login` runs the login in the same step and makes the
    hint unnecessary for that run.

### 4.5 No TTY (agents)

`server add` opens no browser; it reads stdin only for `--secret`. It
writes the entry and gives exit 0, because the add is done. A non-zero
code would make an agent try again. The trust check of the target comes
first (project-config rule 9): an untrusted project file gives exit 2 with
the entries and the hint `ncp trust`; an ignored file gives exit 2 with
the hint `--global`. With a TTY, an untrusted file gets the trust
question of rule 5.

### 4.6 Result

```json
{"server": "linear", "transport": "http", "target": "https://mcp.linear.app/mcp",
 "file": "/Users/u/.config/ncp/mcp.json",
 "secrets": [{"name": "LINEAR_KEY", "destination": "https://mcp.linear.app", "credentialStores": ["native"], "servers": ["linear"]}],
 "nextSteps": [{"command": "ncp server show linear"}]}
```

- `secrets`: each `${NAME}` of the new entry, in the row shape of `secret
  list`: the destination, the credential stores that have it (empty when
  none), and the servers that use it. For a stdio value, `servers` gives
  each stdio entry of the merged view with `${NAME}`, because they share
  one item. Text: one line for each name, for example "LINEAR_KEY: in the native
  store, used by no other server." Reason: a reused value is visible in the
  result, so an agent or a person can see that an existing secret now also
  goes to this URL (review round 2, item 1).
- `nextSteps`: the steps that are left. Each item is `{"command": "..."}`
  (`ncp secret set NAME`, `ncp auth login NAME`, `ncp server show NAME`) or
  `{"setEnv": "NAME"}` for a `${NAME}` in `command`, `args`, `cwd` or `url`.
  A step that depends on the server has `"when"`, for example
  `{"command": "ncp auth login cf", "when": "the server needs a login"}`.
  It is on stdout, so `-q` does not hide it.
- Text: "Added linear to /work/app/.mcp.json." and the lines above. When
  the other file has the same name, a warning says which entry takes
  effect in this folder (project-config rule 9).

## 5. File write

- Target file: project-config rule 9 (the project file; `--global` for the
  global file; `--config-file` for a named file; `--global` with
  `--config-file` is a usage error). A file that does not parse as a JSON
  object gives exit 2 with no change: `ncp` never replaces content that it
  cannot read. A file with no `mcpServers` key gets one.
- Clear about the write, as uv: the first line of the help of `server add`
  and `server remove` names the files ("Add a server to .mcp.json of this
  folder, or with --global to ~/.config/ncp/mcp.json"; uv: "Dependencies
  are added to the project's `pyproject.toml` file"). The text result names
  the file ("Added linear to /work/app/.mcp.json.", "Created
  /work/app/.mcp.json." for a new file), and the JSON result has `file`.
- Principle (user, 2026-10-10): lenient when parsing, strict when writing;
  never break or clobber content that `ncp` does not know.
  - Read: as today. Unknown keys give a warning, an invalid entry does not
    stop the others.
  - Write: `ncp` writes only an entry that passes the strict rules of the
    loader, in one canonical form: `type` first, only known keys.
  - No clobber: each unknown key, each invalid entry of another server and
    each other value stays as it is. Self-check before the rename: parse the
    new text; it must equal the old document plus (or minus) the one entry,
    else no write and exit 3.
  - How (user, 2026-10-10): edit a concrete syntax tree with the crate
    `jsonc-parser` (dprint; feature `cst`; MIT; no new transitive crates
    with `cst` + `serde_json`). Parse, insert or remove the one entry, write
    the tree text. Every other byte stays. Prior art: `deno add` uses
    `jsonc_parser::cst` for `deno.json`; cargo, uv and Codex use `toml_edit`
    for TOML; VS Code uses Microsoft `jsonc-parser` edits. Reason for the
    dependency: without it, `ncp` must build the same format-preserving
    edit itself. Pin a version that is older than two weeks when it is
    added.
  - Read JSONC: done in milestone 18 (`jsonc-parser` 0.33.2 with the
    feature `serde`, only `allow_comments` and `allow_trailing_commas`).
    The edit adds the feature `cst`. The self-check parses the new text
    with the loader, so it needs no `serde_json` feature.
  - Facts from a test of jsonc-parser 0.33.2 (2026-10-10): an insert keeps
    every other byte, the comments, the indent (4 spaces stayed 4) and the
    trailing-comma style; a new or empty file gets 2 spaces. A remove keeps
    the commas correct but leaves a comment on the line above the removed
    entry; that is accepted (no clobber). Errors give "on line N column M".
    Duplicate keys: the last value wins, as `serde_json` today.
- The new entry goes at the end of `mcpServers`, with the indent of the
  entries next to it. A new file gets an indent of 2.
- Atomic: write a temporary file next to the real target, then rename. A
  symlink: resolve it first, so the link stays. A read-only target (for
  example a link into `/nix/store`) gives exit 3 with the path.
  `private_file::write` cannot be used as it is: it sets mode 0600 on each
  write.
- Mode: a new file 0600, a new directory 0700; an existing file keeps its
  mode.
- Lock: `$XDG_STATE_HOME/ncp/config.lock` (the `std` file lock of B55)
  around read, change and rename. Reason: agents run commands in parallel
  (B55), and two adds must not lose an entry. Before the rename, `ncp`
  also reads the file again and stops with exit 3 when it changed, because
  the lock cannot stop an editor or another tool.

## 6. `server remove`

- Target: the entry that `server list` shows for the name, or the global
  entry with `--global` (project-config rule 9). When the name is in both
  files, the result says that the global entry now takes effect.
- Remove the entry first, matched by its raw key, so that an invalid entry
  or a name outside the name rule can be removed too. Same lock and write
  rules as section 5.
- Then delete the OAuth tokens and the client credentials of the server
  from all places. A store failure (locked Keychain over SSH, no Secret
  Service) gives a warning, and exit 0. Reason: the left tokens cannot go
  to another server, because they are bound to the URL and the issuer
  (A13), and the remove must not depend on the store.
- OAuth tokens are keyed by the server name (project-config, "Not in this
  milestone"). Delete them only when their binding (B70: URL and issuer)
  equals the removed entry, and no other entry of the merged view has the
  name. Else keep them, and say so.
- `${NAME}` secrets stay: a value can serve other entries of its origin (S9).
  `secret list` already marks names that no entry uses.
- Caches stay: the protocol version cache and the list cache are keyed by the target and
  the digest of the entry, so a new entry with the same name starts fresh.
- Unknown name: usage error with the nearest name (section 2.1).
- Result: `{"server", "file", "credentialStores"}` (B27 names).

## 7. Changes to the docs (requirements first, AGENTS.md)

- Requirements I8: `ncp` changes the configuration file only through
  `server add` and `server remove`, with the rules of section 5; it can
  create the file. The literal exception ("a value that the user wrote")
  covers a value that the user gives to `server add`.
- Requirements N1: no change.
- Requirements I8: `ncp` writes strict JSON and keeps the rest of the file
  as it is. (The JSONC read is plan milestone 18 item 1.)
- Design section 2: the two commands, the table rows, help order starts with
  `server add`. Section 2.3: `--secret`, `--header`, `--env`.

## 8. Decisions

Keep this list current. Settled items keep their date.

Settled (user, 2026-10-10):
- Stdin: no implicit question. `--secret NAME` asks or reads stdin.
- Literal values: accepted as written, with no warning.
- Credential header rule (rule 5): kept. The spec and RFCs are the source of
  truth.
- Login: no login in `server add`; the result gives `ncp auth login NAME`.
  `--login` later (section 9).
- `--config-file`: the target file is treated as an `ncp` file, with no
  check of the owner. The help and the result name the file (section 5).
- No contact with the server at add (section 4.2): Claude Code and Gemini
  CLI do not contact it; the result lists the login step as conditional and
  `ncp server show NAME`. A Codex-style discovery request is a later
  improvement.
- Bind stored secrets to origins (option B): docs/design/secret-binding.md. Rule 13 and
  the `secrets` result follow it; `secrets[]` uses the row shape of
  `secret list` (`destination`, `credentialStores`, `servers`).
- Target file (user, 2026-10-11): project-config rule 9 and decision G.
- The configuration file stays the source of truth (uv model).
- Lenient when parsing, strict when writing, no clobber; write through a
  concrete syntax tree (`jsonc-parser`, feature `cst`).
- Read JSONC: plan milestone 18 item 1, project-config rule 1.

## 9. Not in this proposal

- Server name from the URL (add-mcp). A separate topic (user, 2026-10-10).
- OAuth client flags (`--oauth-client-id`, secret prompt): the `oauth` keys
  stay a hand edit until a user needs them (C4).
- `--json '<entry>'` (Claude Code `add-json`, VS Code `--add-mcp`).
- `--cwd`: no vendor has the flag; a hand edit.
- Registry search (add-mcp `find`): N1 permits no such network access.
- Package names (add-mcp writes `npx -y <pkg>`): `ncp` writes only the
  command that the user gives.
- Short forms `-e`, `-H`, `-t`, `-s`: the `ncp` rule permits only `-h`,
  `-q`, `-v`, `-V`.
- A `PATH` lookup of a stdio command at add: the next command shows the
  error.
- More than one `--secret` in one command: `nextSteps` gives the others.
- `--login` (run the flow of `auth login` after the write, in the style of
  `--secret`): a later improvement when users ask for it. It adds no break.

## 10. Tests

- Pure: build an entry from arguments; credential header shape; `--env
  NAME`; `--secret` must be used; same entry is a no-op; file change keeps
  order and unknown keys; remove by raw key.
- `--secret`: TTY and pipe (in-memory store), stderr not a TTY, time limit,
  an existing other value, the same value, a store that cannot answer, a
  store failure writes no entry.
- Integration (local only): used name, file with no `mcpServers`, symlink,
  file changed before the rename, two adds in parallel, no TTY path. The
  target tests of project-config section 6 ("Writes").
- Manual interop, by hand, after user approval of the hosts (AGENTS.md).
  Record the result in a new report in docs/research/.

| Server | Path of the flow |
|---|---|
| `https://docs.mcp.cloudflare.com/mcp` | No auth: write, `tool list`, `tool call` |
| `https://blog.mcp.cloudflare.com/mcp` | No auth: second entry, order kept |
| `https://mcp.cloudflare.com/mcp` | OAuth: `nextSteps` gives the login; `auth login`; `server remove` deletes the tokens |
| `https://docs.mcp.cloudflare.com` (no `/mcp`) | The add writes the entry; `server show` gives the error and the hint |

## 11. Review record

A record of history: it keeps the old names (`usedBy`, `place`, probe).

Round 1 (v2), taken from the adversarial review: stored values only for
`env` and `headers`; exact credential header shape; error never echoes a
value; no stdio start; probe with no stored credentials; `401`/`403` per
B10; store check before the read; `nextSteps` on stdout; remove entry first, by
raw key; reread before rename; `mcpServers` needed in the target; prior-art
wording fixed (transport by form is an `ncp` rule). Taken from grug: no
`--no-check`; no cache cleanup at remove; no unused-secret hint at remove;
exit 2 for a file that does not parse; indent of 2 accepted. Not taken:
grug "write first, then probe" (a typo stays in the file); grug "no network
with no TTY" (the probe sends no stored value).

Round 2 (v3), taken from the adversarial review: `usedBy` and `place` in the
result (item 1, minimum fix; the real fix is decision 5); same entry and
same value pass, so a retry works (2); time limit on the secret read, also
for `secret set` (3); TTY test of stdin and stderr, question names the name
and the host (4); literal headers in the probe, an outcome for each status
class (5); lock back, with the reread (6); a store that cannot answer gives
exit 4 (7); usage checks first, `--credential-store` needs `--secret`,
`nextSteps` items as objects, list of doc changes (8). Taken from grug:
`--secret` once per command, so no pipe rule; no environment hint in rule
13; two secret forms in the help; login default "next only". Not taken:
grug "cut `--secret`" (the user chose it; Claude Code `--client-secret` is
the same pattern of a secret flag inside an add); grug "no lock" (the
adversarial case of two parallel adds is real for agents, and the lock code
exists); the "drop `--secret`" hint of rule 13 is removed, because it
teaches the reuse of a shared value.
