# Candidate evaluation (history)

Date: 2026-10-06. Result: no existing CLI is acceptable as a fork base. The
decision is a new build in Rust on `rmcp`. See `rust-clients.md`,
`go-clients.md`, `vendor-clients.md`, `stack-rust.md` and `stack-go.md`.

## Method

Give each requirement one result for a candidate:

- **Has**: the candidate meets the requirement.
- **Fork fix**: a fork can add it with a small change.
- **Structural gap**: the design of the candidate prevents it.

A candidate is acceptable when no MUST requirement is a structural gap.

## Selection criteria

| ID | Level | Criterion |
|---|---|---|
| C1 | MUST | Established project with a known maintainer. No code from small, unknown single-author projects. Decide before any download or test. |
| C2 | MUST | The project follows spec revisions. Its client layer can change to the stateless `2026-07-28` model. |
| C3 | MUST | The license permits personal and company use. |

## avelino/mcp

[avelino/mcp](https://github.com/avelino/mcp) v0.6.2 (Rust) is a reference,
not a base. It fails S1: it keeps tokens in a plain `auth.json` with no `0600`
mode. It fails N1: at first use, it downloads `libchrondb.dylib` and does not
use `HTTPS_PROXY` for this download. N2 and N5 pass. `--version` takes 2.7 ms.
