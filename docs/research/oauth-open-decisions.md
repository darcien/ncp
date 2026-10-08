# OAuth: three open decisions

Date: 2026-10-09. Status: proposal. The user decides. Input: design
sections 2.4 and 5, decisions C1, C1b, C2, C4, B47, B49, B51 and B55;
requirements A5 to A7, S1, S2 and S7; [conformance.md](conformance.md);
[macos-keychain.md](macos-keychain.md); rmcp 3.5.1 and oauth2 5.0.0 source.

| # | Decision | Recommendation |
|---|---|---|
| 1 | Place and content of the Client ID Metadata Document (CIMD) | A subdomain of a domain that the user owns, served by a static host. Fixed path, no version in the path, loopback redirect only. |
| 2 | Pre-registered client secret | Accept `oauth.clientSecret` only as one `${NAME}`. The value comes from the environment, the native store or the opt-in file (C1). Bind the client to its issuer. |
| 3 | Token sets longer than one line of `security -i` | Split the value into parts inside the macOS store, with a head item as the commit point. Do not keep the `id_token`. |

The B numbers below are proposals. B55 and B56 are already in use.

## 1. Client ID Metadata Document

### Facts

- Spec order: pre-registered client, CIMD when the server sets
  `client_id_metadata_document_supported`, then DCR. 2026-07-28 marks DCR as
  deprecated. A CIMD client ID needs no binding to an issuer. The document
  is at an HTTPS URL with a path, `client_id` is that URL exactly, and
  `client_name` and `redirect_uris` are required [reg-2026].
- CIMD draft 02: no fragment, no dot segment, no user information; a query
  SHOULD NOT be used; a stable URL is RECOMMENDED (§3). No `client_secret`
  and no shared-secret method (§4.1). The redirect URI matches exactly (§4.2).
  The server MUST NOT follow a redirect (§5). It caches with the HTTP cache
  headers and does not cache errors (§5.2). A new URL is a new client with no
  relation to the old one (§8.3). Read limit: 5 KB (§8.7) [cimd-02].
- The draft does not mention loopback ports. RFC 8252 §7.3 says that the
  server MUST allow any port for a loopback IP redirect URI. Claude Code
  lists port-less loopback URIs, so a server that accepts Claude Code also
  accepts `ncp`.
- rmcp: `AuthorizationRequest::with_client_metadata_url(url)`.
  `AuthorizationSession::new` uses it only with no client ID and with the
  flag set. It refuses a URL that is not HTTPS or has the root path.
  `ncp` registers by DCR itself in `client_request` (B47), so `ncp` must
  check the flag itself before it uses a client item or registers.
- `auth/basic-cimd` expects the client ID
  `https://conformance-test.local/client-metadata.json` and fetches nothing.

Documents of other clients (fetched 2026-10-09):

| Client | URL | `redirect_uris` | Cache |
|---|---|---|---|
| Claude Code | `https://claude.ai/oauth/claude-code-client-metadata` | `http://localhost/callback`, `http://127.0.0.1/callback` | `public, max-age=300` |
| VS Code | `https://vscode.dev/oauth/client-metadata.json` | `http://127.0.0.1:33418/`, `https://vscode.dev/redirect` | `no-store` |

Both use the product domain, a fixed path with no version, and the method
`none`. VS Code adds `application_type` `native`.

### Options for the place

| Option | Example URL | For | Against |
|---|---|---|---|
| A. Own domain, any static host behind it | `https://ncp.example.org/oauth/client-metadata.json` | The URL does not depend on the forge, the mirror or the host. The consent page shows the host of the project. | A domain to renew. An expired domain lets another party publish redirect URIs for the name `ncp`. |
| B. GitHub Pages of the mirror | `https://USER.github.io/ncp/oauth/client-metadata.json` | Free, HTTPS, JSON content type, `max-age=600`. | The URL depends on the account name and the mirror. A rename breaks it, and the server does not follow redirects. |
| C. Codeberg Pages or a Forgejo pages server | `https://USER.codeberg.page/ncp/oauth/client-metadata.json` | Near the source. | The same account lock-in. A self-hosted Forgejo has no built-in pages. When the host is down, logins fail. |
| D. Raw file of the forge | `https://FORGE/USER/ncp/raw/branch/main/...` | No setup. | `text/plain` (checked on Codeberg), the branch name is in the URL, `private` cache. |
| E. No CIMD (today) | — | Nothing to host. | A server with CIMD and no DCR fails. The conformance check fails. |

### Recommendation

Option A: a subdomain of a domain that the user already owns and renews.
GitHub Pages and Codeberg Pages accept a custom domain, so the files can
stay on B or C, and the host can change with no change to the URL. `ncp`
does not fetch the document, so N1 does not change. To publish it is an
upload, so it is part of the release gate (plan M13).

When the URL must move: keep the old URL with the same content, at least for
the longest refresh-token life, and better for ever. Do not redirect. The
tokens item keeps the client ID, so a refresh still uses the old URL; only a
new login uses the new URL, with a new consent. Do not change
`redirect_uris` without need: a server can then cancel the grants (§8.4).

The document (exact content; replace the host):

```json
{
  "client_id": "https://ncp.example.org/oauth/client-metadata.json",
  "client_name": "ncp",
  "client_uri": "https://ncp.example.org/",
  "redirect_uris": ["http://127.0.0.1/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "token_endpoint_auth_method": "none",
  "application_type": "native"
}
```

Only `127.0.0.1` with no port: `ncp` uses only this redirect (RFC 8252
§7.3, §8.3). No `logo_uri` and no `software_version`, so the document does
not change for each release.

### Proposed design text

| ID | Decision | Reason |
|---|---|---|
| proposed entry D1 | When the authorization server metadata sets `client_id_metadata_document_supported` to `true` and the entry has no `oauth.clientId`, `auth login` uses the URL of the `ncp` document as the client ID, with no client secret. It does not read or write a client item and does not register. Else the order of B47 stays. The URL is a constant in the code. The test binary can replace it; `ncp` reads no variable for it. | The order of the spec. A CIMD client ID is portable, so it needs no binding to an issuer. |

Design section 5 gets the URL. Requirements: A7 becomes "`ncp` has a CIMD
document at `<URL>`. The repository keeps it, and a test checks its
`client_id` against the URL in the code."

### Test plan

- `Platform` gets the URL; `ncp-cli` reads `NCP_TEST_CLIENT_METADATA_URL`,
  as it reads `NCP_TEST_BROWSER`. The auth fixture gets the metadata flag.
- `uses_the_metadata_document_url_as_client_id_when_the_server_supports_it`
  (no `/register`, no client item),
  `registers_dynamically_when_the_server_has_no_metadata_document_support`,
  `preregistered_client_id_comes_before_the_metadata_document`,
  `refreshes_a_metadata_document_client_with_no_secret`,
  `published_metadata_document_matches_the_client_id_in_code` (`include_str!`).
- Conformance: the adapter sets the suite URL. Remove
  `auth/basic-cimd:cimd-client-id-used` from `expected-failures.yaml`.

## 2. Pre-registered client secret

### Facts

- Spec: clients SHOULD support static client credentials. They MUST bind
  them to the issuer, and SHOULD show an error on another issuer [reg-2026].
- RFC 6749 §2.3.1: the server MUST support HTTP Basic, with the ID and the
  secret form-encoded first; the secret in the body is NOT RECOMMENDED.
  RFC 8414 §2: the default method is `client_secret_basic`.
- rmcp `configure_client` uses Basic, and the body only when the metadata
  lists `client_secret_post` and not `client_secret_basic`. oauth2 5.0.0
  encodes as RFC 6749 §2.3.1 asks.
- `auth/pre-registration`: no DCR, only `client_secret_basic`, and Basic
  with `pre-registered-client:pre-registered-secret`.

| Tool | Shape | Place of the secret |
|---|---|---|
| Claude Code | `oauth.clientId` in the file; `--client-secret` at `mcp add` (prompt, or `MCP_CLIENT_SECRET`) | macOS Keychain or a credentials file. Set only when the server is added. |
| Gemini CLI | `oauth.clientId`, `oauth.clientSecret` | In the file. |
| Cursor | `auth.CLIENT_ID`, `auth.CLIENT_SECRET` | In the file; `${env:NAME}` can replace the value. |

### Options

| Option | For | Against |
|---|---|---|
| A. Keep C4: no secret | Simple. | Confidential clients fail, and the conformance check. |
| B. Value in the file | Common (Gemini CLI, Cursor). | A secret in a plain file (S2). |
| C. One `${NAME}` | Key of Gemini CLI. Resolution of `headers` (C1, B31). Environment for CI. No new command. | The user stores the value first. |
| D. New command, secret in the client item | The file names no secret. | A new command, a second place. |

Recommendation: option C. It meets the spec and S2 with no new command.

### Proposed design text

| ID | Decision | Reason |
|---|---|---|
| proposed entry D2 | `oauth.clientSecret` is a string that is exactly one `${NAME}` or `${env:NAME}`. Another value is an entry error (exit 2): "oauth.clientSecret must name a secret, for example `${NAME}`; the file must not hold the value", with the hint `ncp secret set NAME`. The error never shows the value. `clientSecret` with no `clientId` is an entry error: "oauth.clientSecret needs oauth.clientId". `ncp` gets the value as for `headers`: the environment, the native store, then the file (C1, B31). It gets it at `auth login` before the browser opens, and after a `401` only when the stored tokens have the client ID of `oauth.clientId`. The token request uses HTTP Basic. It uses the body only when the metadata lists `client_secret_post` and not `client_secret_basic`. When the metadata lists methods and has neither, `auth login` stops with exit 4: "the authorization server accepts no client secret", with the methods. At the first login, `ncp` keeps a client item with the issuer and the client ID, and no secret. A later login or refresh at another issuer with the same `clientId` stops with exit 4: "oauth.clientId is registered at A, but the server now names B". The hint: register a client at B and change `oauth.clientId`, or run `ncp auth logout SERVER` when the change is expected. `auth logout` removes this item, because it holds no secret. `secret list` marks the name as used. | MCP pre-registration and issuer binding; RFC 6749 §2.3.1; S2. |

C4 changes from "only `clientId` in the MVP" to "only `clientId` and
`clientSecret`". B47 changes: with `oauth.clientId`, `ncp` keeps the binding
item. A client item from DCR stays as it is.

Requirements, section 3.5: "The MVP reads only `oauth.clientId`" becomes
"The MVP reads `oauth.clientId` and `oauth.clientSecret`. `clientSecret` is
one `${NAME}`; a value in the file is an error."

### Test plan

- The auth fixture gets settings for DCR off and for the token methods.
- `logs_in_with_a_preregistered_secret_from_the_environment_with_basic_auth`,
  `sends_the_secret_in_the_body_when_the_server_supports_only_post`,
  `refreshes_with_the_preregistered_secret_from_the_native_store`,
  `refuses_a_client_secret_value_in_the_configuration_file` (no value in
  the output), `refuses_a_client_secret_without_a_client_id`,
  `stops_with_the_secret_set_hint_when_the_client_secret_name_has_no_value`,
  `stops_when_the_server_names_another_issuer_for_a_preregistered_client`,
  `stops_when_the_server_accepts_no_client_secret`,
  `verbose_trace_hides_the_basic_authorization_header`,
  `secret_list_marks_the_client_secret_name_as_used`.
- Conformance: the adapter writes `"${NCP_CONFORMANCE_CLIENT_SECRET}"` and
  sets it. Remove `auth/pre-registration:pre-registration-auth`.

## 3. Token sets longer than one line of `security -i`

### Facts

- A line has at most 4095 bytes; the command takes 133 bytes for a server
  name of 64 characters. Thus the value has at most about 2970 bytes.
- The item is the JSON of rmcp `StoredCredentials`, with all extra fields of
  the token response, also `id_token`. Other fields: about 300 to 500 bytes.
- A D-Bus message (Secret Service) can have 128 MiB, and the file has no
  limit. Only the macOS store has the limit.

| Provider | Access token | Refresh token | `id_token` | Item | Source |
|---|---|---|---|---|---|
| Entra ID | 1.5 KB typical, 4 KB for planning; up to 200 groups add about 8 KB | 1 KB typical, 2 KB for planning | 2 KB for planning | 4 to 8 KB; more with groups | [msal], [msqa], [groups] |
| Google | at most 2048 B | at most 512 B | about 1 KB (estimate) | at most about 4 KB | [google] |
| GitHub | 40 characters now; plan for 255 | the same | none (OAuth only) | below 1 KB | [github] |
| Okta, Auth0 | JWT, about 1 KB (estimate) | short opaque value (estimate) | about 1 KB (estimate) | 2 to 3 KB | estimate: about 15 claims and an RS256 signature of 342 characters |

Entra ID, the usual provider of companies, does not fit, also with no
`id_token`. Okta and Auth0 are near the limit.

### Options

| Option | Atomicity and failure | For | Against |
|---|---|---|---|
| (a) Keep the error with the `--credential-store file` hint | One item. | No code. | Entra ID users must use a plain file. That is S2 in name only. |
| (b) Split into parts with a head item | The head is the commit point (below). A failed write keeps the old tokens. | Any size. Linux and the file do not change. Only the macOS store module changes (S7). | 1 + n runs of `security` for each read. More code in one module. |
| (c) Keep fewer fields (no `id_token`) | One item. | Less personal data at rest. | Not sufficient for Entra ID. |
| (d) Deflate before base64 | One item. | — | Base64 text has 6 bits of data in 8, and a signature does not compress: about 20 % less. A new dependency. Not sufficient. |
| (e) Encrypted file, key in the Keychain (the Electron `safeStorage` method) | Atomic by rename. | Any size. | A crypto dependency. Token data on disk. Changes S1 and S2. |

Recommendation: (b) in the macOS store, and (c) for the `id_token` only.
(b) is the only option that fits Entra ID with no plain file, and it stays
behind the store interface (S7). (c) removes data that `ncp` never reads.

### Proposed design text

| ID | Decision | Reason |
|---|---|---|
| proposed entry D3 | The macOS store keeps a value whose command line is longer than 4095 bytes in parts. It encodes the whole value in base64 and cuts the text into slices of 3072 characters. Each slice is an item `ACCOUNT/GEN/I` (I from 1) with the value `ncp-part:` and the slice. The item `ACCOUNT` holds `ncp-parts:GEN:N`. GEN is new for each save, for example the time in nanoseconds as hexadecimal. A save writes each part in its own run and checks each exit code. Then it writes the head, and then it removes the parts of the old GEN. A failure before the head keeps the old value. A failure after it gives only a debug line. A read that does not find a part reads the head again one time, because another process can have saved. Then the error is "the stored tokens of SERVER are incomplete" (exit 4), with the hint `ncp auth login SERVER`. A delete removes the head, its parts, and each other account that starts with `ACCOUNT/`. The account list does not show parts. This applies to all values, also to secrets. Before it saves tokens, `ncp` removes `id_token` from the token response. | Entra ID token sets do not fit in one line. One `add-generic-password -U` changes one item atomically, so the head makes the save atomic for a reader. MCP uses OAuth, not OpenID Connect, and the `id_token` holds personal data. |

B51 loses the size example; the hint `--credential-store file` stays for a
store that cannot answer. Requirements: S7 gets "A value longer than one
command line of `security` is kept in several items." This changes how
secrets are stored, so the user decides (AGENTS.md).

### Test plan

- The fake `security` refuses a line over 4095 bytes and can fail at a
  given command.
- Pure plan: `keeps_a_value_that_fits_one_line_in_one_item`,
  `splits_a_long_value_into_parts_and_joins_them_back`.
- Fake program: `an_8_kib_token_set_round_trips_through_the_macos_store`,
  `a_new_save_removes_the_parts_of_the_old_generation`,
  `a_failed_part_write_keeps_the_old_tokens_readable`,
  `a_missing_part_reads_the_head_again_and_then_reports_incomplete_tokens`,
  `logout_removes_the_head_and_all_parts`, `secret_list_does_not_show_parts`,
  `login_does_not_keep_the_id_token`.
- Linux container: `an_8_kib_token_set_round_trips_through_the_secret_service`.

## Sources

- [reg-2026] MCP 2026-07-28, client registration: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
- MCP 2025-11-25, authorization, client registration approaches: https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization#client-registration-approaches
- [cimd-02] draft-ietf-oauth-client-id-metadata-document-02, 2026-07-06: https://www.ietf.org/archive/id/draft-ietf-oauth-client-id-metadata-document-02.html
- RFC 8252 §7.3 and §8.3; RFC 6749 §2.3.1; RFC 8414 §2.
- Conformance `auth/basic-cimd`, `auth/pre-registration` (0.2.0-alpha.12).
- CIMD of Claude Code and VS Code: the two URLs in the table of section 1.
- Claude Code, pre-configured OAuth credentials: https://code.claude.com/docs/en/mcp
- Gemini CLI, MCP servers: https://geminicli.com/docs/tools/mcp-server/
- Cursor static OAuth (vendor guide, not Cursor): https://docs.domaintools.com/mcp/oauth/
- [google] Google OAuth 2.0, token size: https://developers.google.com/identity/protocols/oauth2
- [msal] MSAL.NET token cache serialization: https://learn.microsoft.com/en-us/entra/msal/dotnet/how-to/token-cache-serialization
- [msqa] Microsoft Q&A, token length: https://learn.microsoft.com/en-us/answers/questions/1657854/oauth-2-0-refresh-token-and-access-token-max-length
- [groups] Entra group claims limit: https://learn.microsoft.com/en-us/security/zero-trust/develop/configure-tokens-group-claims-app-roles
- [github] GitHub token formats: https://github.blog/changelog/2021-03-04-authentication-token-format-updates/
- D-Bus specification, message size: https://dbus.freedesktop.org/doc/dbus-specification.html
- Electron `safeStorage`: https://www.electronjs.org/docs/latest/api/safe-storage
- rmcp 3.5.1 `src/transport/auth.rs`; oauth2 5.0.0 `src/endpoint.rs`.
