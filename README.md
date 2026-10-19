# ncp

`ncp` is a command-line client for the Model Context Protocol (MCP), for
macOS and Linux.

This is a toy project to answer these questions:
- How well can MCP specs + prior art from other Rust projects (like mise or uv) can guide LLM?
- Can MCP as CLI tool truly replace MCP tool injected into agent context?

In this repo, generally:
- the code and docs are generated output (maybe excluding some part of this README.md),
- decisions follow these order: MCP spec > prior art > common sense,
- (trying to) secure by default (which sometimes conflict with ease of use).

Target MCP spec versions:
- `2026-07-28`
- `2025-11-25`

## Build

```
cargo build --release -p ncp
```

## Development

The rules for contributors and agents are in `AGENTS.md`. The design is in
`docs/design/cli.md`, and the requirements are in
`docs/requirements.md`.

## License

MIT or Apache-2.0, at your choice.
