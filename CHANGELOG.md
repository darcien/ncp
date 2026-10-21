# Changelog

All notable changes to `ncp` are in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and `ncp` uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - 2026-10-21

### Changed

- cli: Write text by default, also in a pipe
  - In a pipe, ncp writes text and not JSON. If a script or an agent reads JSON from ncp, add --output-format json to each command.

### Fixed

- cli: Make the help text self-contained and consistent
- cli: Use one form and one term for each output message
- deps: Update rustls-webpki to 0.103.16

## 0.1.0 - 2026-10-20

The first release: a command-line MCP client for macOS and Linux.

### Added

- Servers over stdio and Streamable HTTP, from the user configuration and
  from a trusted project `.mcp.json`.
- Commands for the tools, resources and prompts of a server, with text
  output on a TTY and JSON in a pipe.
- MCP `2026-07-28`, with fallback to `2025-11-25`.
- OAuth login of HTTP servers, with the client metadata document of `ncp`,
  dynamic registration, or a configured client ID.
- Secrets for `${NAME}` in the macOS Keychain, the Secret Service on
  Linux, or a credential file.
- Binaries for macOS, and static binaries for Linux, on x86_64 and aarch64.
