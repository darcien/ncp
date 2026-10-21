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

## Install

With [mise](https://mise.jdx.dev):

```
mise use -g github:darcien/ncp
```

Or download the archive for your platform from
[Releases](https://github.com/darcien/ncp/releases), and put `ncp` on your
`PATH`.

## Build from source

```
cargo install --locked --path crates/ncp
```

## Development

Install the tools and the git hook:

```
mise install
hk install --mise
```

Do the static checks and the tests:

```
mise run check
mise run test
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) and [STYLE.md](STYLE.md) before you
change the project.

## License

MIT or Apache-2.0, at your choice.
