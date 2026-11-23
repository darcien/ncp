# AGENTS.md

Before significant work, read [CONTRIBUTING.md](CONTRIBUTING.md) and the documents
that it names for the task.

## Evidence

Before you fix a problem, show it with evidence, for example a failing
test, the output of a command, or the code.

## Defaults

Do only the steps that a change needs.

- A backlog item is a rough issue. It can name a symptom and not the
  problem, propose a fix too early, or be out of date. Do not take the
  fix in the item as given.
- Find the cause and all the places that it affects: other commands,
  texts, documents and tests. A fix in one place can leave the problem
  in another.
- When you finish, tell the maintainer the problem, the cause and the
  change.

### Reviews

- Get an adversarial review from a subagent for an RFC before the
  maintainer decides. Do the same for a new or changed command, option
  or output field before you commit it.
- Use a new subagent for each round, so that it reads the work fresh.
  Give it just enough documents and references.
- Ask for findings with a severity, the evidence and a fix. Problems
  that a user meets, and false facts, come before the wording.
- Stop when a round finds no high or medium finding, or after 3 rounds,
  unless the maintainer asks for more.

## Stop and ask

Do not do these actions until the maintainer decides:

- Add a network destination to `ncp`, its build, its tests or its
  tools. These destinations need no decision:
  - crates.io and the Rust toolchain.
  - A pinned package from the MCP organization, with the dependencies
    that its lock file pins.
  - An action that the repository settings permit, with the
    destinations that it reaches.
- Run code that does not come from these sources.

To read a public page for research or a citation, for example a spec,
an RFD or the documentation of a tool, you do not need a decision.

Before you ask for a decision, research it. Then show:

1. The problem, and the reason that it needs the maintainer.
2. The options, their tradeoffs and the evidence. Verify each fact about
   prior art with a command or a source. Name each claim that you did
   not verify.
3. Your recommendation, with its label. For an action in the list
   above, keep the recommendation neutral and set no default answer.

Do not end with a request for a yes or no. The maintainer decides when
to decide, and can ask more questions first. If you find a false
statement in an earlier message, correct it first.

### GitHub Actions

The repository settings are the only list of permitted actions. Do not
copy the list into a file, because a copy becomes stale. To see the
settings, run:

```sh
gh api repos/{owner}/{repo}/actions/permissions
gh api repos/{owner}/{repo}/actions/permissions/selected-actions
```

- Pin each action to a full commit SHA, with the version tag in a
  comment. The settings stop a workflow with a tag or a branch.
- To use an action that the settings do not permit, ask the maintainer.
  The maintainer must change the settings first.

## Secrets

Do not propose a detection of secrets. Do not ask the maintainer about it
again.

## Commands that use the Keychain

Run `ncp` for your work. The commands below are the exception. They can
read or change the Keychain of the maintainer:

- `ncp secret`
- `ncp auth`
- `ncp server add --secret`
- `ncp server remove` for an HTTP server
- `security`
- A command for a server that asks for a login or has a `${NAME}` that
  is not in the environment

The Keychain can open a dialog that only a person can answer. The command
waits until the person answers.

- If the task needs the real Keychain, run the command with `ncp`. Before
  you run it, tell the maintainer that a dialog can open.
- If the task is a test of the code, run the command with the test build
  `ncp-cli` (`crates/testkit/src/bin/ncp-cli.rs`). It has a fake Keychain.

## Commits

You can commit. Use the git identity of the maintainer. Do not add a
`Co-Authored-By` line or any other AI attribution.
