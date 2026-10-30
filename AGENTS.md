# AGENTS.md

Before any work, read [CONTRIBUTING.md](CONTRIBUTING.md) and the documents
that it names for the task. They apply to agents too. This file adds the
rules for agents only.

## Workflow

- The backlog is in `.scratch/backlog/` (not tracked by git). Do the item
  that the maintainer names. Do not choose the next item yourself.
- Keep notes in `.scratch/` (not tracked by git).
- Before you commit a change to a command, an option, an output field or
  a message, get an adversarial review from a subagent. Do 3 rounds at
  most. Put the findings that remain in the backlog, and tell the
  maintainer about each high finding.
- When you brief a subagent, give it the paths of `STYLE.md`,
  `CONTRIBUTING.md` and the documents of its task. Do not give it a rule
  list of your own. Two rule lists give two answers for one text.

## Stop and ask

Stop and ask before you:

- Add a network destination other than crates.io, the Rust toolchain, and
  pinned packages from the MCP organization. A pinned package includes the
  dependencies that its lock file pins.
- Run code that does not come from these sources.

These rules apply to `ncp` and to the development on this machine. The
settings of the CI platform control the network of the CI.

## Secrets

Do not propose a detection of secrets. Do not ask the maintainer about it
again.

## Host credential stores

These commands use the Keychain of the maintainer: `ncp secret`,
`ncp auth`, `ncp server add --secret`, `ncp server remove` for an HTTP
server, `security`, and a command for a server that asks for a login or has
a `${NAME}` that is not in the environment. They can open a dialog that
only a person can answer. To test them, use the test build `ncp-cli`
(`crates/testkit/src/bin/ncp-cli.rs`). It has a fake Keychain.

## Commits

You can commit. Use the git identity of the maintainer. Do not add a
`Co-Authored-By` line or any other AI attribution.
