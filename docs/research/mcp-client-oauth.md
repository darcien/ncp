# Study: pre-registered OAuth clients and issuer binding in MCP clients

Date: 2026-10-09. Input for design B67 and B68.

**Method.** Three agents read the source of open clients (`gh api` at a
fixed commit) and the official documents of closed clients. Nothing was
cloned, installed or run. The main session checked the key lines again (the
lines with "checked" below). "Not found" means that the source that was read
does not state the fact.

**Question.** How does a client get a pre-registered client ID and secret,
and does it bind them to the issuer of the authorization server? MCP
2026-07-28, client registration, Authorization Server Binding: clients "MUST
associate those credentials with the specific authorization server that
issued them, keyed by the authorization server's `issuer` identifier". For
pre-registered credentials, clients "SHOULD surface an error rather than
silently attempting to use mismatched credentials".

## 1. Results

| Client | Client ID and secret | Issuer in the configuration | Behavior on another issuer |
|---|---|---|---|
| Claude Desktop, managed servers | `oauth.clientId`; secret by helper script, or inline by MDM | `oauth.authorizationServer` (`string[]`), required with a secret (checked) | "Claude Desktop sends the secret only to the token endpoint of the issuer you name there" (checked) |
| MCP TypeScript SDK | `ClientCredentialsProvider({clientId, clientSecret, expectedIssuer})`; host provider for interactive flows | `expectedIssuer`; to leave it out is deprecated (`authExtensions.ts:104`, checked) | Refuses: "refusing to present them to a different authorization server" (`authErrors.ts:206`, checked). An old value with no stamp is accepted one time and stamped (`auth.ts:117-123`, checked). |
| MCP Go SDK | `PreregisteredClient {ClientID, ClientSecretAuth, Issuer}` | `Issuer`, optional | Error when `Issuer` is set and does not match (`authorization_code.go:559`, checked). No check when it is empty. |
| MCP Python SDK | `TokenStorage.get_client_info()` | `issuer` field of the client information | Pre-registered credentials "carry no binding to enforce" (`utils.py:340-358`, checked). DCR credentials of another issuer are removed, and the SDK registers again. |
| rmcp 3.5.1 | `with_preregistered_client`, `with_client_secret` | None | Checks the issuer only of stored tokens, and clears them (`auth.rs:1445-1497`, checked). |
| MCP Inspector | `--client-id`, `--client-secret`; catalog `oauth` | None | Calls pre-registered credentials "static, issuer-independent". Secrets go to the OS keychain. |
| VS Code | `oauth.clientId`; secret by a code lens to OS storage | None | Not found. DCR and tokens are kept apart for each authorization server. Secret always in the body. |
| Gemini CLI | `oauth.clientId`, `oauth.clientSecret` (literal) | `oauth.issuer`, but discovery overwrites it; only the `iss` check of the callback uses it | No binding. |
| OpenAI Codex CLI | `client_id`, `client_secret` in `config.toml` | Only for enterprise token exchange | Tokens of another issuer: "authorization required" (`issuer_binding.rs:117`, checked). The client ID is not bound. |
| Goose | `client_id`, `client_secret_key` (a reference, never inline) | None | rmcp clears the tokens, then a new browser login. |
| Zed | `oauth.client_id`, `client_secret` or a prompt to the keychain | None | Not found. Logout removes the secret too. Secret always in the body. |
| Claude Code | `--client-id`, `--client-secret` (masked prompt, to the keychain) | None (`authServerMetadataUrl` only replaces discovery) | Only the RFC 9207 `iss` check. |
| Cursor | `auth.CLIENT_ID`, `auth.CLIENT_SECRET`, `${env:}` allowed | None | Not found. |
| GitHub Copilot CLI | `oauthClientId`; secret in the keychain | None | Not found. |
| Factory droid (minor tool) | `oauth.clientId`, `oauth.clientSecret` | `authorizationServerIssuer`, required with a client ID or secret | Not found. |

## 2. Findings

- No client records the issuer at the first use for pre-registered
  credentials. The TypeScript SDK stamps an old stored value one time, only
  as a migration.
- The clients that bind a secret bind it to an issuer that the user names:
  Claude Desktop (required with a secret) and the TypeScript SDK (required by
  deprecation). The Go SDK binds when the caller sets the issuer.
- For a public client ID, most clients do not bind. They bind only tokens
  and DCR records, and clear them or ask for a new login on another issuer.
- Good error texts name both issuers, say that the secret was not sent, and
  give no one-step way to continue (TypeScript SDK, Inspector).
- Claude Desktop compares the issuer "exactly as its metadata states it ...
  (no trailing slash)". The TypeScript SDK accepts a trailing slash.
- Token authentication: rmcp and the TypeScript SDK prefer HTTP Basic. VS
  Code, Gemini CLI and Zed always send the secret in the body.

## 3. Names

The MCP spec defines no client configuration file. The value is an issuer
identifier: RFC 8414 §2 names the metadata field `issuer`, RFC 9728 §2
defines `authorization_servers` as a list of issuer identifiers, and the
Python and Go SDKs use `issuer`. No configuration key is common to many
clients.

## 4. Sources

- MCP 2026-07-28, client registration: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
- RFC 8414 §2, RFC 9728 §2, RFC 9207, RFC 6749 §2.3.1.
- Commits read: typescript-sdk b022522, go-sdk da6ad80, python-sdk 91941ed,
  rust-sdk 08e0211, inspector 0d2bc1d, vscode e9ed7cc, gemini-cli 2ce1a69,
  openai/codex 2351d9e, aaif-goose/goose cd1da62, zed f0b38ac.
- Claude Desktop: https://claude.com/docs/third-party/claude-desktop/mcp-sign-in
  and https://claude.com/docs/third-party/claude-desktop/configuration
- Claude Code: https://code.claude.com/docs/en/mcp
- Cursor: https://cursor.com/docs/context/mcp
- GitHub Copilot CLI: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference
- Factory droid: https://docs.factory.com/cli/configuration/mcp
