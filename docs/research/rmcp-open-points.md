# Research: rmcp open points

Status: 2026-10-06. Baseline: [requirements.md](../requirements.md).
Source report: [stack-rust.md](stack-rust.md). Comparison: [stack-go.md](stack-go.md).
Subject: [`rmcp`][rmcp] 3.5.1 (tag `rmcp-v3.5.1`, equal to `main` on 2026-10-06).

Method: I read rmcp source at tag `rmcp-v3.5.1` and issues, PRs, reviews,
timelines, advisories and discussions through `gh api`. I read the MCP spec
`2026-07-28` and the security best practices page from the source repository
(`modelcontextprotocol/modelcontextprotocol`, `main`). I read go-sdk v1.8.0
auth files through `gh api`. RFC 7591 and OAuth 2.1 (draft -16) text came from
WebFetch. WebFetch then stopped at a session limit. RFC 9728 text came from the
working-group source of the RFC (`oauth-wg/draft-ietf-oauth-resource-metadata`,
final draft XML); I did not compare it with the published RFC page. I did not
clone, build, install or run code. "Unknown" means not verified.

## Summary

| # | Point | Assessment | Spec position | Status | Workaround (no fork) |
|---|---|---|---|---|---|
| 1 | Era probe timeout fixed at 10 s | Deliberate choice, not security (not configurable) | Spec requires a fallback after "a reasonable timeout" on stdio. It gives no value. 10 s is an rmcp choice. | Closed: #1142 asked for "configurable or with a sane default"; #1149 (merged 2026-08-11) chose a constant. No open item. | Use `Discover` inside an own `tokio::time::timeout`. On timeout or legacy error, start the server again with `Initialize`. Keep the era per server. |
| 2 | `Discover` always sends `server/discover`; no era reuse | Deliberate choice, not security (not configurable) | `server/discover` is optional (MAY). On stdio the spec RECOMMENDS the probe. Clients MAY keep the era across restarts. | No issue or PR. Code comment: "pre-initialization probing is planned as follow-up work". | Known legacy server: use `Initialize` (no probe). Known modern server: no skip. Cost: 1 round trip. |
| 3 | DCR `client_secret` not kept | Known bug, fix stalled | MCP: DCR is deprecated and optional; persistence is not required. OAuth 2.1: a client with issued credentials MUST authenticate on refresh. | #784 open, P2, `Stale` since 2026-10-02 (auto-close in 14 days). PR #1255 open, review questions unanswered since 2026-09-11. | Call `register_client` yourself, keep the secret, call `configure_client` with the secret after `initialize_from_store`. Not tested. |
| 4 | Cross-origin `resource_metadata` rejected | Known bug, fix in progress | MCP: clients MUST use the `resource_metadata` URL when present. RFC 9728 has no same-origin rule. rmcp is not spec-correct. | #1298 open, P1, `ready for work`. PR #1302 draft, approved 2026-09-29. It waits for a private advisory fix (GHSA-5p52-q7pq-9fq2). | Fetch PRM and AS metadata yourself, then `set_metadata`. Caveat: `resource` falls back to the MCP URL. |
| 5 | AS metadata at private IP literal rejected | Deliberate security choice (not configurable) | Best practices: clients SHOULD block private ranges; protections "depend on your network environment". RFC 9728: SHOULD take SSRF precautions. | No issue. The private advisory fix moves checks to resolved addresses; private DNS names can then fail too. Design: Unknown. | Use a host name (passes today). Or fetch metadata yourself and call `set_metadata`. |
| 6 | 12 breaking releases in 12 months, no LTS | Deliberate policy | No spec rule for SDK versions. SEP-1730 Tier 1 requires "clear versioning", not LTS. | `VERSIONING.md` since 2026-07-30. No LTS, no backport branch, no support window. MSRV 1.88. | Pin `rmcp = "=3.5.x"`. Plan one major upgrade for each spec revision. |

## 1. Era probe timeout (10 s, private constant)

What the code does:

- `ClientLifecycleMode::Auto` doc: "falling back when the peer reports that it is legacy or does not respond within 10 seconds" ([client.rs L642][cl-mode]).
- `const DEFAULT_AUTO_DISCOVER_TIMEOUT: Duration = Duration::from_secs(10);` is private ([L657][cl-const]).
- All public entry points pass this constant ([L738–L758][cl-entry]). The inner function takes a `Duration`, but it is private.
- On timeout, `Auto` sends `initialize` on the same transport ([L799–L841][cl-auto]).
- The timer starts when the probe starts. Process start time of a stdio server counts against the 10 s.

Late reply (derived from code, not tested): after the timeout, `expect_response`
waits for the `initialize` response. A late `server/discover` response has
another ID. The function then returns `ConflictInitResponseId` ([L183][cl-expect]).
Result: a slow modern stdio server (for example a first `npx` download) fails
to connect. It does not fall back. go-sdk PR #1334 drops the late reply.

Why: issue [#1142][i1142] (2026-08-06, P1, label `2026-07-28`): "the probe needs
a bounded wait — likely a timeout around the `discover_startup` /
`expect_response` call, configurable or with a sane default." PR [#1149][p1149]
(Dale Seo, merged 2026-08-11): "This PR adds a 10-second timeout to the Auto
discovery probe." No review discussed the value or configurability.

Spec position ([stdio Backward Compatibility][spec-stdio-compat]): the client
falls back when the server "does not respond within a reasonable timeout". The
spec gives no value and no configurability rule. The compatibility matrix says
"the probe returns a non-modern error or times out, and the client falls back".
The timeout is spec-required. The value 10 s is an rmcp choice.

Status: #1142 closed by #1149. No open issue or PR asks for a setting.

go-sdk: no internal bound; `Connect` blocks until the caller deadline (open
[#1332][gs-1332]). Open PR [#1334][gs-1334] adds 3 s on stdio and drops a late
reply. go-sdk has the bigger gap today.

Workaround: do not use `Auto`. Use `ClientLifecycleMode::Discover` inside an own
`tokio::time::timeout`. `Discover` returns an error for a legacy server ("Discover
mode does not fall back"). On timeout or on a JSON-RPC error that is not a modern
code, kill the child and start it again with `Initialize`. Save the era for the
server configuration (spec: MAY persist). The next run uses the saved mode and
does not probe a legacy server.

Assessment: **Deliberate choice, not security (not configurable)**. A one-line
fork fix makes it a parameter.

## 2. `server/discover` always sent; no era reuse across processes

What the code does:

- `Discover` and `Auto` always send `server/discover` first ([`discover_startup` L950][cl-disc]).
- The per-request metadata setter `set_client_request_metadata` is `pub(crate)` ([service.rs L1038][svc-meta]). `ClientRequestMetadata` is `pub(crate)` ([L715][svc-crm]). Only `discover_startup` and `legacy_startup` fill it.
- `Peer::discover` caches the result in memory per `Peer` with `ttlMs` ([L1302][cl-peer-disc]). The cache type is `pub(crate)` ([cache.rs][cache]). No API seeds it from disk.
- Doc comment on `Peer::discover`: "pre-initialization probing is planned as follow-up work."
- `Initialize` mode skips the probe. It suits a known legacy server only.

Why: PR [#995][p995] (2026-07-15, merged 2026-07-16) added the modes: "auto mode
can fall back for legacy servers and the existing legacy lifecycle remains the
default." No review comment discussed a skip or a cached era. I found no issue
or PR that asks for it.

Spec position:

- [Versioning][spec-ver-compat]: "Clients **MAY** call it before sending any other requests ... but are not required to".
- Same page: "Clients **SHOULD** cache the result for the lifetime of the server process (stdio) or origin (HTTP), and **MAY** persist it across restarts of the same server configuration".
- [stdio][spec-stdio-compat]: a dual-era client "**SHOULD** probe with `server/discover` before sending any other request". For a modern-only client, "probing is still **RECOMMENDED**".
- [Streamable HTTP][spec-http-compat]: a dual-era client "**MAY** detect which era the server implements by attempting a modern request first".

Result: on stdio, the probe is the recommended behavior. On HTTP, the spec
permits a direct request. A skip API is a MAY feature. Its absence is not a
spec defect.

Status: no issue, no PR, no milestone.

go-sdk: same. A pinned `ProtocolVersion: "2025-11-25"` skips the probe for a
legacy server. A modern server costs one discover per process. No API injects a
cached result. Both SDKs comply.

Workaround: for a known legacy server, use `Initialize` with the saved version.
For a known modern server, use `Discover` with the saved version first in
`preferred_versions`. This avoids a `-32022` retry. The probe stays. Cost: one
pipe round trip (stdio) or one POST (HTTP). With an own pooled HTTP client, the
call after `discover` reuses the connection.

Assessment: **Deliberate choice, not security (not configurable)**. A fork fix
exposes the metadata setter (small change).

## 3. DCR client secret not saved (#784)

What the code does:

- `StoredCredentials` has `client_id`, `token_response`, `granted_scopes`, `token_received_at`, `issuer`. It has no `client_secret` ([auth.rs L225][a-sc]).
- `register_client` asks for a public client: `token_endpoint_auth_method: "none"` ([L1728][a-reg], [L1754][a-none]).
- It keeps a returned non-empty secret in `OAuthClientConfig` and calls `configure_client` ([L1643][a-cc]). Refresh in the same process sends the secret.
- `initialize_from_store` restores with `configure_client_id`, which sets `client_secret: None` ([L1445][a-init], [L1833][a-cid]). Refresh in a new process fails with `invalid_client`.

Why: no PR made this a decision. The DCR code asks for a public client and the
store was designed for public clients. Issue [#784][i784] (2026-04-01): "oauth
refresh fails due to missing client_secret. Error: invalid_client".

Spec position:

- MCP [Client Registration][spec-reg-dcr]: "Dynamic Client Registration is deprecated." DCR support is MAY. The [binding rule][spec-reg-bind] applies to "Clients that ... persist client credentials obtained via Dynamic Client Registration". The spec does not require persistence.
- [RFC 7591 §2][rfc7591]: "The authorization server MAY reject or replace any of the client's requested metadata values". An AS can issue a secret after a `"none"` request.
- [OAuth 2.1 draft-16 §4.3.1][oauth21]: "if the client type is confidential or the client was issued client credentials (or assigned other authentication requirements), the client MUST authenticate with the authorization server".

Result: in one process rmcp complies. After a restore it breaks the OAuth 2.1
MUST for an AS that issued a secret. Baseline S1 lists client secrets as
secrets to keep.

Status:

- #784: `enhancement`, `P2`. Alex Hancock, 2026-07-28: "We'll look at this and see if any other code changes or docs changes are still applicable here". Bot label `Stale` 2026-10-02: closes "in 14 days if there is no further activity".
- PR [#1255][p1255] (external, 2026-09-10): adds `client_secret` to `StoredCredentials`. Dale Seo review, 2026-09-11, two open questions: one restore path drops the secret, and the `get_credentials`/`set_credentials` path has no restore. No answer. State `blocked`.
- Downstream: openai/codex#43216 (open).

go-sdk: no persistence. DCR runs in each `Authorize`. The secret reaches the
caller in the `oauth2.Config` given to `NewTokenSource`
([authorization_code.go L583][gs-dcr]). The CLI keeps it. Spec-compliant.

Workaround (code read, not tested): on first login, call `register_client`
yourself; it returns `OAuthClientConfig` with `client_secret`. Keep the secret
in the OS store with the issuer. On later runs, call `initialize_from_store`,
then `configure_client` with the saved secret. This replaces the client without
a secret. The issue author used a similar method in 1.x: he re-implemented
`initialize_from_store`, `OAuthState::start_authorization` and
`AuthorizationSession::new`.

Assessment: **Known bug, fix stalled**.

## 4. Cross-origin `resource_metadata` rejected (#1298)

What the code does:

- `resolve_resource_metadata_url` keeps the URL only when it has the same scheme, host and port as the MCP URL. Otherwise it logs `warn!` and returns `None` ([auth.rs L1300][a-rrm], [L1224][a-so]). Call site: `WWW-Authenticate` parse ([L3050][a-www]).
- Discovery then falls back to well-known URLs on the MCP origin. A centralized PRM host is never read.
- No setting exists. The discovery helpers that take a URL are private.

Why: PR [#935][p935] "fix: block oauth metadata ssrf" (Dale Seo, 2026-06-27,
merged in 2 h): "The OAuth client now only accepts same-origin
protected-resource metadata URLs, blocks private and metadata-host
authorization server URLs discovered from that metadata, and follows discovery
redirects only when they stay on the same origin." Advisory
[GHSA-c9xm-49cp-xcr9][ghsa-c9xm]: the client fetched a server-controlled URL
"without same-origin or private-network validation". Fixed in 2.0.0.

Spec position:

- MCP [Authorization Server Discovery][spec-asd]: "MCP clients **MUST** support both discovery mechanisms and use the resource metadata URL from the parsed `WWW-Authenticate` headers when present". No origin limit.
- RFC 9728 §3.3 ([rfc9728-33]): for a URL from `resource_metadata`, "the resource value returned MUST be identical to the URL that the client used to make the request to the resource server. If these values are not identical, the data contained in the response MUST NOT be used." This check, not the origin, stops impersonation.
- RFC 9728 §7.7 ([rfc9728-77]): "Clients SHOULD take appropriate precautions against SSRF attacks, such as blocking requests to internal IP address ranges."

Result: the same-origin rule is stricter than the spec. It breaks a MUST.
This is an rmcp defect.

Status:

- [#1298][i1298] open since 2026-09-25: `bug`, `P1`, `T-security`, `ready for work`. No milestone.
- PR [#1302][p1302] (Dale Seo, draft, 2026-09-26): "a `resource_metadata` URL ... doesn't have to be on the same origin". It applies the host check of AS URLs. Alex Hancock asked for an HTTPS no-downgrade check (added). Approved 2026-09-29.
- Blocker: a reviewer (ndreno) reported that the host check does not resolve DNS ("for a DNS name it returns 'allowed'"). He filed private advisory GHSA-5p52-q7pq-9fq2. Dale Seo, 2026-09-29: "hold off on merging until the security fix is ready". The advisory fix is in a private fork. Release date: Unknown.
- Note: the issue's real example (`api.anthropic.com` → `claude.ai`) shows a PRM URL on the same origin as the MCP URL. Only the AS is cross-origin, and rmcp accepts a public cross-origin AS. I did not verify the live headers.

go-sdk: no same-origin rule. `GetProtectedResourceMetadata` checks HTTPS or
loopback, rejects private IP literals, and checks `resource` equality (RFC 9728
§3.3) ([resource_meta.go L127][gs-prm]). Spec-correct.

Workaround: on a 401, parse `WWW-Authenticate` in the CLI. Fetch the PRM
yourself with the same SSRF checks. Check `resource` against the MCP URL. Fetch
AS metadata, check `issuer` (RFC 8414 §3.3). Call `set_metadata` ([L1436][a-setmeta]).
Caveats: `resource` in requests falls back to the MCP URL ([L1898][a-res]); a
different PRM `resource` value is lost. PRM `scopes_supported` is not recorded;
pass scopes explicitly.

Assessment: **Known bug, fix in progress** (blocked by a private advisory).

## 5. Authorization server metadata at a private IP rejected

What the code does:

- `is_allowed_authorization_server_metadata_url` ([auth.rs L1281][a-asurl]) rejects some hosts ([L1228][a-ip4], [L37][a-cloud]):
  - IPv4 literals: private, loopback, link-local, CGNAT (`100.64/10`), benchmark (`198.18/15`), broadcast, multicast, unspecified.
  - IPv6 literals: loopback, unspecified, multicast, `fc00::/7`, `fe80::/10`.
  - Names: `localhost`, `*.localhost`, `metadata`, `metadata.google.internal`, `metadata.azure.internal`.
- Exception: loopback AS with a loopback MCP server.
- Only AS URLs from PRM `authorization_servers` go through this check ([L2662][a-asck]).
- A DNS name passes without resolution. `auth.corp.example` at `10.1.2.3` works today.
- No setting exists.

Why: same PR [#935][p935] and advisory [GHSA-c9xm-49cp-xcr9][ghsa-c9xm]. Goal:
stop a malicious MCP server from sending the client to internal or cloud
metadata endpoints.

Spec position:

- [Security best practices, SSRF][spec-ssrf]: "MCP clients deployed to a server **MUST** consider SSRF risks". "Which protections are appropriate depend on your network environment." "MCP clients **SHOULD** block requests to private and reserved IP address ranges as recommended by RFC 9728 Section 7.7". It lists `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `fc00::/7`, `fe80::/10`.
- The same page wants "an explicit opt-out mechanism for development/testing scenarios" for HTTPS. It states no opt-out rule for IP ranges.
- RFC 9728 §7.7: SHOULD take SSRF precautions (quote in section 4).
- The [authorization security considerations][spec-sec] page has no client SSRF rule.

Result: rmcp follows a SHOULD. The page is a best-practices page, not a
specification page. A company AS at a private IP literal fails. The spec text
accepts environment-based choices; rmcp gives none.

Status: no issue asks for an opt-out. The fix for GHSA-5p52-q7pq-9fq2 checks
resolved addresses (see section 4). Then a company AS host name with a private
address can also fail. The loopback rule is an open design question in that
fork. Final design and release: Unknown.

go-sdk: rejects private IP literals for PRM and AS URLs, no opt-out
([oauth2.go L88][gs-https]). Its default discovery transport also rejects
private addresses at dial time. It skips the dial check when a proxy is set or
the caller sets its own `DialContext` ([L105–L170][gs-dial]). go-sdk is
stricter by default but has an opt-out for DNS names. Both follow the SHOULD.

Workaround: use a DNS name for the AS (works in 3.5.1). Or fetch the AS
metadata yourself and call `set_metadata` (section 4). Watch the advisory fix
before an upgrade.

Assessment: **Deliberate security choice (not configurable)**.

## 6. Release policy

What the project states:

- [VERSIONING.md][ver] (added 2026-07-30, PR [#1087][p1087], "meeting requirements for tier 1"): SemVer 2.0; MAJOR for breaking API changes; checks with `cargo-semver-checks`; migration guides for big upgrades.
- MSRV: `rust-version = "1.88"` ([Cargo.toml][cargo]). Set in PR [#1034][p1034] (2026-07-23): "This is currently 1.88 based on how we use the language now". PR [#1316][p1316] (merged 2026-10-05) turned on the MSRV-aware resolver after `uuid` 1.27 needed Rust 1.89.
- Contradiction: VERSIONING.md lists an MSRV raise as a breaking change. It also says the raise "is only done in a MINOR or MAJOR release".
- No text on LTS, support windows, backports or security fixes for old majors.
- [ROADMAP.md][road]: Tier 1 items done. No stability or LTS item.
- Migration guides: 2.x ([discussion #926][d926]): "Over time the `rmcp` model drifted from the MCP Schema Reference." 3.x ([discussion #969][d969]): support for spec `2026-07-28`.

Release data (GitHub releases, 2025-10-06 to 2026-10-06): 12 breaking releases
(0.9 to 0.17, 1.0.0, 2.0.0, 3.0.0). All tags are on one line. No release on 2.x
after 3.0.0 (2026-07-28). No release on 1.x after 2.0.0 (2026-06-29). Advisory
fixes ship only in the newest major (GHSA-9pj6, GHSA-33f5, GHSA-c9xm in 2.0.0).
The only "backport" found is in a downstream fork (`mzeng-openai/rust-sdk`, PR #1285).

Spec position: no MCP spec rule on SDK versions. [SEP-1730 tiers][tiers]:
Tier 1 needs a "Stable Release ... Required with clear versioning", meaning
"documented breaking change policies". It needs no LTS. Spec revisions bring
breaking changes ("`YYYY-MM-DD` ... the last date backwards incompatible
changes were made", [learn/versioning][learn-ver]). rmcp maps each breaking
spec revision to a major release (3.0 for `2026-07-28`).

go-sdk: v1.0.0 on 2025-09-30, now v1.8.0. No breaking major in 12 months. A new
spec revision comes in a minor release. Behavior changes get a temporary
`MCPGODEBUG` opt-out ([CONTRIBUTING.md][gs-contrib], [mcpgodebug.md][gs-godebug]).

Workaround: pin an exact minor (`=3.5`). Read release notes before each update.
Expect a new major at the next spec revision. Budget one migration per spec
revision and for model clean-ups such as 2.0.

Assessment: **Deliberate policy** (documented SemVer, no LTS).

## Sources

- rmcp source (tag `rmcp-v3.5.1`): [client.rs][cl], [service.rs][svc], [client/cache.rs][cache], [auth.rs][auth], [VERSIONING.md][ver], [ROADMAP.md][road], [Cargo.toml][cargo]
- rmcp issues: [#784][i784], [#1142][i1142], [#1298][i1298]
- rmcp PRs: [#935][p935], [#995][p995], [#1034][p1034], [#1087][p1087], [#1149][p1149], [#1255][p1255], [#1302][p1302], [#1316][p1316]
- rmcp discussions: [#926][d926], [#969][d969]; advisory [GHSA-c9xm-49cp-xcr9][ghsa-c9xm]
- MCP spec `2026-07-28`: [versioning][spec-ver-compat], [stdio][spec-stdio-compat], [Streamable HTTP][spec-http-compat], [authorization server discovery][spec-asd], [client registration][spec-reg-dcr], [security considerations][spec-sec], [security best practices][spec-ssrf], [learn/versioning][learn-ver], [SDK tiers][tiers]
- RFCs: [RFC 9728][rfc9728] (§3.3, §7.7; text from [WG source][rfc9728-src]), [RFC 7591][rfc7591], [OAuth 2.1 draft-16][oauth21]
- go-sdk (v1.8.0): [oauthex/oauth2.go][gs-https], [oauthex/resource_meta.go][gs-prm], [auth/authorization_code.go][gs-dcr], [#1332][gs-1332], [#1334][gs-1334], [CONTRIBUTING.md][gs-contrib]

[rmcp]: https://github.com/modelcontextprotocol/rust-sdk
[cl]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs
[cl-expect]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L168-L205
[cl-mode]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L637-L655
[cl-const]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L657
[cl-entry]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L738-L758
[cl-auto]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L799-L841
[cl-disc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L950
[cl-peer-disc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client.rs#L1297-L1302
[cache]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service/client/cache.rs
[svc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs
[svc-crm]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L715
[svc-meta]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/service.rs#L1038
[auth]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs
[a-cloud]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L37
[a-sc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L225
[a-so]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1224
[a-ip4]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1228
[a-asurl]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1281
[a-rrm]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1300
[a-setmeta]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1436
[a-init]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1445
[a-cc]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1643
[a-reg]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1728
[a-none]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1754
[a-cid]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1833
[a-res]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L1898
[a-asck]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L2662
[a-www]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/transport/auth.rs#L3050
[ver]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/VERSIONING.md
[road]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/ROADMAP.md
[cargo]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/Cargo.toml#L13
[i784]: https://github.com/modelcontextprotocol/rust-sdk/issues/784
[i1142]: https://github.com/modelcontextprotocol/rust-sdk/issues/1142
[i1298]: https://github.com/modelcontextprotocol/rust-sdk/issues/1298
[p935]: https://github.com/modelcontextprotocol/rust-sdk/pull/935
[p995]: https://github.com/modelcontextprotocol/rust-sdk/pull/995
[p1034]: https://github.com/modelcontextprotocol/rust-sdk/pull/1034
[p1087]: https://github.com/modelcontextprotocol/rust-sdk/pull/1087
[p1149]: https://github.com/modelcontextprotocol/rust-sdk/pull/1149
[p1255]: https://github.com/modelcontextprotocol/rust-sdk/pull/1255
[p1302]: https://github.com/modelcontextprotocol/rust-sdk/pull/1302
[p1316]: https://github.com/modelcontextprotocol/rust-sdk/pull/1316
[d926]: https://github.com/modelcontextprotocol/rust-sdk/discussions/926
[d969]: https://github.com/modelcontextprotocol/rust-sdk/discussions/969
[ghsa-c9xm]: https://github.com/modelcontextprotocol/rust-sdk/security/advisories/GHSA-c9xm-49cp-xcr9
[spec-ver-compat]: https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning#backward-compatibility-with-initialization-based-versions
[spec-stdio-compat]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio#backward-compatibility
[spec-http-compat]: https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http#backward-compatibility
[spec-asd]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/authorization-server-discovery#protected-resource-metadata-discovery-requirements
[spec-reg-dcr]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration#dynamic-client-registration
[spec-reg-bind]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration#authorization-server-binding
[spec-sec]: https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations
[spec-ssrf]: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices#server-side-request-forgery-ssrf
[learn-ver]: https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning
[tiers]: https://modelcontextprotocol.io/community/sdk-tiers
[rfc9728]: https://datatracker.ietf.org/doc/html/rfc9728
[rfc9728-33]: https://datatracker.ietf.org/doc/html/rfc9728#section-3.3
[rfc9728-77]: https://datatracker.ietf.org/doc/html/rfc9728#section-7.7
[rfc9728-src]: https://github.com/oauth-wg/draft-ietf-oauth-resource-metadata/blob/main/draft-ietf-oauth-resource-metadata.xml
[rfc7591]: https://datatracker.ietf.org/doc/html/rfc7591#section-2
[oauth21]: https://datatracker.ietf.org/doc/html/draft-ietf-oauth-v2-1-16#section-4.3.1
[gs-https]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/oauthex/oauth2.go#L88
[gs-dial]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/oauthex/oauth2.go#L105-L170
[gs-prm]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/oauthex/resource_meta.go#L115-L150
[gs-dcr]: https://github.com/modelcontextprotocol/go-sdk/blob/v1.8.0/auth/authorization_code.go#L583
[gs-1332]: https://github.com/modelcontextprotocol/go-sdk/issues/1332
[gs-1334]: https://github.com/modelcontextprotocol/go-sdk/pull/1334
[gs-contrib]: https://github.com/modelcontextprotocol/go-sdk/blob/main/CONTRIBUTING.md
[gs-godebug]: https://github.com/modelcontextprotocol/go-sdk/blob/main/docs/mcpgodebug.md
