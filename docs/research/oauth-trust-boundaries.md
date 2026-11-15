# OAuth trust boundaries

Date: 2026-11-15.

The review examined ncp, rmcp 3.10.0, and selected reference clients.
No exploit scenarios or external project programs ran.
This report recommends scope. It does not accept or implement RFC 7.

## Findings

These problems affect the same OAuth flow but cross different trust boundaries.
Each needs separate completion criteria.

| Problem | Boundary and evidence |
|---|---|
| Network admission | Server-selected URLs can reach unintended local or private services. The [RustSec notice][advisory] excludes custom OAuth HTTP clients from resolved-address protection. RFC 7 proposes that protection for ncp. |
| High: forged issuer metadata | A compromised MCP server can name an attacker metadata URL with an unrecognized `/.well-known/` path. rmcp can skip issuer validation. The document can claim the stored issuer but name a public HTTPS attacker token endpoint. A later refresh can disclose existing refresh tokens and client secrets. See [SDK discovery][sdk-discovery] and [stored-token setup][stored-login]. |
| Medium: mixed credential state | For the same server name and store, an interrupted secret-bearing login at issuer B can replace the client while A's tokens remain. When A's entry takes effect again, a shared client ID can select B's secret for A. This requires cached-client lookup, not a configured client or CIMD. A hostile A receives B's secret. An ordinary A can reject the wrong secret. See [client storage][client-storage] and [secret lookup][secret-lookup]. |

Address permission does not establish issuer authority or credential ownership.
Neither address filtering nor TLS repairs the two credential defects.
[RFC 8414 §6.2][impersonation] describes issuer impersonation through metadata.
[RFC 6749 §2.2][client-id] scopes client IDs to an authorization server, not globally.

## Reference implementations

Discovery considered the TypeScript and Python MCP SDKs, VS Code, Codex,
uv, mise, AWS SDK for Rust, and openidconnect-rs.
Protocol relevance and boundary tests guided selection, not language or popularity.

| Reference | Transferable mechanism and limit |
|---|---|
| [TypeScript MCP SDK][typescript] | Metadata issuer validation and separate issuer stamps for clients and tokens. Its [forged-issuer tests][typescript-tests] cover the impersonation boundary. Opt-outs and legacy credential tolerance remain. |
| [VS Code][vscode] | The normal credential namespace includes the selected authorization server and resource. Useful state isolation, not a complete issuer-validation reference. |
| [uv][uv] | Cache keys include recipient context and username. Package authentication is not OAuth discovery. |
| [AWS SDK for Rust][aws] | Refresh obtains client ID, client secret, and refresh token from one record. Regional endpoints and session cache keys are not general MCP authority rules. |

These mechanisms are references, not claims of complete project security.

## Recommended scope

- Keep RFC 7 focused on address admission and its explicit connection limits.
- Track metadata provenance and credential-state isolation as separate defects.
- Use one trust model: authenticated metadata authorizes endpoints, matching issuer/client identity selects credentials, and network permission restricts connections.
- Add forged metadata during noninteractive refresh and interrupted cross-issuer login with a shared client ID to the regression scenarios.
- Review the fixes together. Do not require one abstraction, HTTP stack, or large refactor without evidence.
- Resolve any shared issuer-selection, storage-format, or permission change before implementation. A medium or large redesign needs an RFC.
- Publish the concept explanation with the implemented behavior, not with proposed protections.

Credential isolation already appears in [requirements A5 and A13](../requirements.md).
The address correction alone does not complete that contract.

[advisory]: https://raw.githubusercontent.com/RustSec/advisory-db/c5ed55613b44ac8c641bb7e86120afb659930e58/crates/rmcp/RUSTSEC-2026-0457.md
[sdk-discovery]: https://docs.rs/rmcp/3.10.0/src/rmcp/transport/auth.rs.html#2745-2937
[stored-login]: ../../crates/ncp/src/cli/auth.rs#L544-L638
[client-storage]: ../../crates/ncp/src/cli/auth.rs#L1104-L1160
[secret-lookup]: ../../crates/ncp/src/cli/auth.rs#L643-L666
[impersonation]: https://www.rfc-editor.org/rfc/rfc8414#section-6.2
[client-id]: https://www.rfc-editor.org/rfc/rfc6749#section-2.2
[typescript]: https://github.com/modelcontextprotocol/typescript-sdk/blob/919bb8ea88b6e10e72c2f8b6218fad4cfdc40b2c/packages/client/src/client/auth.ts#L1311-L1427
[typescript-tests]: https://github.com/modelcontextprotocol/typescript-sdk/blob/919bb8ea88b6e10e72c2f8b6218fad4cfdc40b2c/packages/client/test/client/auth.test.ts#L1174-L1239
[vscode]: https://github.com/microsoft/vscode/blob/3ea3ca1d231cae6826355b86b03635ebf0d93068/src/vs/workbench/api/common/extHostAuthentication.ts#L516-L540
[uv]: https://github.com/astral-sh/uv/blob/d5c3f174417418c24ec3c276e01b0f62b961b13c/crates/uv-auth/src/cache.rs#L34-L42
[aws]: https://github.com/awslabs/aws-sdk-rust/blob/c88e7f9c26a66d0500bc1cc820ce73b76fa22136/sdk/aws-config/src/sso/token.rs#L71-L136
