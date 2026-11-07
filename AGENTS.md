# AGENTS.md

Before any work, read [CONTRIBUTING.md](CONTRIBUTING.md) and the documents
that it names for the task.

## Workflow

### Start from the problem

A backlog item is a rough issue. It can name a symptom and not the
problem, propose a fix too early, or be out of date. Before you write
code:

1. Show the problem with evidence, for example a failing test, the
   output of a command, or the code. If you cannot show it, tell the
   maintainer.
2. Find the cause and all the places that it affects: other commands,
   texts, documents and tests. A fix in one place can leave the problem
   in another.
3. List the fair solutions. Do not take the fix in the item as given.
   Compare the solutions with the order of choice.
4. Decide if the change needs an RFC. If you are not sure, ask.
5. Tell the maintainer the problem, the cause and the path that you
   chose.

### Write an RFC

1. Put the chosen solution in Determinations and the others in
   Alternatives.
2. For prior art, run uv and mise, and quote their output with the
   version. Help text changes between versions, so memory is not a
   source.
3. Save each revision as `.scratch/rfcNNNN/draft-N.md`, so that a
   reviewer can compare two revisions.
4. Get reviews (see "Reviews").
5. Give the maintainer your recommendation. Name each choice that does
   not follow uv and mise, because the maintainer must decide it.
6. When the maintainer accepts or abandons the RFC, set its `state`.

### Reviews

- Get an adversarial review from a subagent for an RFC before the
  maintainer decides. Do the same for a change to a command, an option,
  an output field or a message before you commit it.
- Use a new subagent for each round, so that it reads the work fresh.
  Give it the paths of `STYLE.md`, `CONTRIBUTING.md` and the documents
  of its task. Do not summarize the rules for it, because a summary can
  differ from the rules.
- Ask for findings with a severity, the evidence and a fix. Problems
  that a user meets, and false facts, come before the wording.
- Stop when a round finds no high or medium finding, or after 3 rounds,
  unless the maintainer asks for more.
- After you shorten a draft, ask a reviewer to compare it with the
  previous revision, so that no fact is lost.

## Stop and ask

Stop and ask before you:

- Add a network destination to `ncp`, its build, its tests or its
  tools, other than crates.io, the Rust toolchain, and pinned packages
  from the MCP organization. A pinned package includes the dependencies
  that its lock file pins.
- Run code that does not come from these sources.

The settings of the CI platform control the network of the CI.

To read a public page for research or a citation, for example a spec,
an RFD or the documentation of a tool, you do not need to ask.

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
