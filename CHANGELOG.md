# Changelog

All notable changes to `ncp` are in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and `ncp` uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.2 - 2026-11-17

This release corrects RUSTSEC-2026-0457. A hostile server can no longer send
the OAuth requests of `ncp` to a service in your network or on your computer.
If the authorization server of a server is in a private network, set
`oauth.allowPrivateAddresses` to `true` in the entry of the server.

### Added

- cli: Run a joined server and tool name, and look up the server first
- oauth: Check the address of each OAuth request and add oauth.allowPrivateAddresses

### Fixed

- cli: Show no word that is not a secret name
- deps: Update rmcp to 3.10.0 and rustix to 1.1.6

## 0.2.1 - 2026-10-30

### Added

- cli: Show structuredContent in the text output of tool call

### Fixed

- cli: Stop the wait for the token lock at Ctrl-C
- cli: Use "directory" in place of "folder"

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
