---
state: accepted
supersedes: []
---

# 0001: Text output by default

## Summary

The default output is text, on a TTY and in a pipe. JSON comes only from
`--output-format json`. This replaces the MUST rule I1 of
`docs/requirements.md`. It breaks a program that reads JSON from `ncp` in
a pipe.

## Reason

- One command gives the same format in a terminal and in a pipe.
- A person who pipes the output into `less` or `grep` reads text.
- uv and mise write text in a pipe, and JSON only with a flag:
  `uv pip list --format json`, `mise ls --json`.

## Design

"TTY" and "pipe" refer to stdout.

- With no `--output-format`, the format is text.
- With `--output-format json`, stdout gets one JSON value and stderr gets
  JSON Lines.
- The TTY sets only the layout of JSON on stdout: indented on a TTY, one
  line in a pipe.
- The JSON keys and types do not change.
- In a pipe with no `--output-format`, `resource read` writes the raw bytes
  of the contents.
- The checks of stdin and stderr for questions and input do not change.
