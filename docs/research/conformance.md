# Conformance: the official MCP suite against `ncp`

Date: 2026-10-08. Plan milestone 12. Method: [test-harness.md](test-harness.md).

## Versions

| Part | Version | Check |
|---|---|---|
| Node | 22.23.3 (exact, `mise.toml`) | Installed by mise. |
| `@modelcontextprotocol/conformance` | 0.2.0-alpha.12 (exact, `harness/conformance/package.json`) | npm `alpha` tag, 2026-10-01. |
| Integrity of the suite | `sha512-eXtZFgmtZiU5SsvXty+RGXGnyIqEVm3GLU/HZmg1wMjtKOocPZZgD9HQKd30oJhio5UB0wFJmsc6r8u48Q+EYA==` | The same in the registry and in `package-lock.json`. |
| Publisher | The `@modelcontextprotocol` scope. Published by GitHub Actions as a trusted publisher. Repository `github.com/modelcontextprotocol/conformance`. License MIT. | npm registry metadata. |
| Install | `npm ci --ignore-scripts` from the committed `package-lock.json`. Each dependency has an exact version and an integrity hash. | `mise run conformance:install` |

## Method

`mise run conformance` builds the adapter `ncp-conformance` (test kit) and
runs the suite in client mode for `2025-11-25` and `2026-07-28`: once with
the frozen requirement set of the version, and once with all scenarios of
the version. The proxy variables are removed. The suite starts its servers
on localhost; the run uses no other network.

The adapter maps each scenario to `ncp` commands, so that the suite tests
the command line as a black box:

- A temporary home, the file credential store, and the fake browser of
  `ncp-cli`, which follows the redirect of the mock authorization server.
- `tool list`, then one `tool call` for each tool, with the arguments of
  `toolCalls` in the context, or a sample value for each required
  property. The schema scenario gets the input schema of its focal tool
  back, as `ncp tool list` shows it.
- When `ncp` exits with 4, the adapter runs `auth login` and the command
  again, as a user does after the hint. At most 3 logins.
- The suite says that a client derives the lifecycle from the protocol
  version that it gets. `ncp` finds the era itself, so the adapter writes
  the era of the version into the era cache before the first command. No
  option of `ncp` is necessary.

## Result

| Run | Passed checks | Failed checks | Baseline |
|---|---|---|---|
| Requirements 2025-11-25 | 387 | 22 | Passed |
| All scenarios 2025-11-25 | 333 | 7 | Passed |
| Requirements 2026-07-28 | 588 | 19 | Passed |
| All scenarios 2026-07-28 | 532 | 4 | Passed |

Each failure is in `harness/conformance/expected-failures.yaml` with its
reason, or is an extension scenario:

| Scenario | Reason |
|---|---|
| `elicitation-sep1034-client-defaults` | Elicitation (P9) comes after the MVP. With no TTY, `ncp` exits with 5. |
| `sep-2322-client-request-state` | A multi-round tool call (P8): `ncp` gives the `input_required` result to the caller with exit 5. |
| `auth/basic-cimd` (check `cimd-client-id-used`) | CIMD is off until the user decides the place of the document (A7). |
| `auth/pre-registration` (check `pre-registration-auth`) | The scenario needs a client secret. The configuration reads only `oauth.clientId` (design C4). |
| `auth/client-credentials-*`, `auth/enterprise-managed-authorization`, `auth/dpop*`, `auth/wif-jwt-bearer` | Extensions. Out of scope (test-harness.md, section 2.3). The requirement sets list them, and the suite does not count them against the baseline. |

## Changes from the first run

The first run found these defects. Each one is fixed:

- `auth login` did discovery with no `401` challenge. A server that names
  its protected resource metadata only in the challenge
  (`auth/metadata-var3`) failed, and the scopes of the challenge were not
  requested. `auth login` now connects first and uses the challenge for
  discovery and for the scopes.
- A legacy-era server can let `initialize` in with no token and answer
  `401` to the first request. `ncp` read the stored tokens only after a
  `401` at the connect. Now a `401` to a request on a connection with no
  token causes one more try with the stored login.
- From `2026-07-28`, a client mirrors each tool parameter with
  `x-mcp-header` into an `Mcp-Param-*` header (SEP-2243). rmcp knows these
  parameters only from a listed tool, and `tool call` with
  `--no-validate` and a JSON object did not list the tools. Now `tool
  call` lists the tools first on an http server of the modern era (B5).

## Not covered by the suite

The suite has no stdio scenario, no cancellation, no refresh grant and no
static header. The test kit covers these with local fixtures.
