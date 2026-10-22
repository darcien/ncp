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

## MCP feature support

| Area | Supported | Not supported |
|---|---|---|
| Transports | [stdio][stdio], [Streamable HTTP][http] | [HTTP+SSE][sse] (2024-11-05) |
| Authorization (HTTP) | [OAuth][oauth] with [pre-registration][pre-registration], [Client ID Metadata Documents][cimd], or [Dynamic Client Registration][dcr] | [OAuth Client Credentials][client-credentials] (`client_credentials` grant), [Enterprise-Managed Authorization][enterprise-auth], [DPoP proposal][dpop], [Workload Identity Federation proposal][wif] |
| Server features | [Tools][tools], [Resources][resources] (list, templates, read), [Prompts][prompts] | — |
| Server utilities | — | [Completion][completion], [Logging][logging] |
| Client features | [Elicitation][elicitation] (form and URL modes, with a TTY) | [Sampling][sampling], [Roots][roots] |
| Other features | [Progress][progress] | [Tasks][tasks], [Subscriptions][subscriptions] (resource updates and list changes) |

[stdio]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio
[http]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
[sse]: https://modelcontextprotocol.io/specification/2024-11-05/basic/transports#http-with-sse
[oauth]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
[pre-registration]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration#pre-registration
[cimd]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration#client-id-metadata-documents
[dcr]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration#dynamic-client-registration
[client-credentials]: https://modelcontextprotocol.io/extensions/auth/oauth-client-credentials
[enterprise-auth]: https://modelcontextprotocol.io/extensions/auth/enterprise-managed-authorization
[dpop]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1932
[wif]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1933
[tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools
[resources]: https://modelcontextprotocol.io/specification/2026-07-28/server/resources
[prompts]: https://modelcontextprotocol.io/specification/2026-07-28/server/prompts
[completion]: https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/completion
[logging]: https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/logging
[elicitation]: https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation
[sampling]: https://modelcontextprotocol.io/specification/2026-07-28/client/sampling
[roots]: https://modelcontextprotocol.io/specification/2026-07-28/client/roots
[progress]: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress
[tasks]: https://modelcontextprotocol.io/extensions/tasks/overview
[subscriptions]: https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/subscriptions

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
