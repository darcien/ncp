# SDK project health: go-sdk and rust-sdk

Date: 2026-10-06. Window: 2025-10-06 to 2026-10-06, unless a row gives a
different window.

Method: `gh api` (repository data, commits, merged PRs, issues, labels,
workflows, organization `MAINTAINERS.md`), MCP SDK tier pages. No code was
cloned or run. Commit counts use GitHub logins, and bots are shown apart.

## 1. Summary

| Item | go-sdk | rust-sdk |
|---|---|---|
| MCP SDK tier | Tier 1 | Tier 1 |
| License | MIT to Apache-2.0 transition (same text) | Same |
| Stars, forks | 5187, 570 | 3979, 651 |
| Contributors, all time | 159 | 201 |
| Commit authors, 12 months (no bots) | 103 | 120 |
| Commits, 12 months | 353 (32 dependabot) | 481 (55 dependabot, 46 github-actions) |
| Releases, 12 months | 23 | 94 (all crates in the workspace) |
| Who merged the last 100 PRs | guglielmo-san 90, yarolegovich 10 | DaleSeo 84, alexhancock 14, jamadeo 2 |
| Median time to merge (last 100 PRs) | 22 h | 62 h |
| Open issues, open PRs | 63, 58 | 21, 33 |
| Open issues with no label | 23 (oldest 2026-04-27) | 0 |
| Open P0, P1 | 0, 0 | 0, 5 |
| Issues closed 12 months: completed, not planned | 236, 20 | 275, 5 |
| Stale bot | No | Yes (`stale.yml`) |
| CI | test, conformance, CodeQL, Scorecard, nightly | ci, conformance, CodeQL, Scorecard, triage, auto-label |
| Companies of active maintainers | Google only | Block, Apollo GraphQL, OpenAI (listed) |

## 2. Maintainers

The organization file `MAINTAINERS.md` lists the SDK maintainers.

| SDK | Listed maintainer | Commits, 12 months | Last commit |
|---|---|---|---|
| go-sdk | guglielmo-san (Google) | 65 | 2026-09-30 |
| go-sdk | maciej-kisiel (Google) | 47 | 2026-05-15 |
| go-sdk | yarolegovich (Google) | 11 | 2026-09-02 |
| go-sdk | jba | 3 | 2026-03-27 |
| rust-sdk | alexhancock (Block) | 31 | 2026-09-26 |
| rust-sdk | jamadeo | 23 | 2026-09-14 |
| rust-sdk | michaelneale | 3 | 2026-07-10 |
| rust-sdk | bolinfest | 3 | 2025-12-18 |
| rust-sdk | baxen | 0 | — |

Other people with a large share:

- go-sdk: findleyr made 45 commits. The last one is from 2026-03-03. findleyr is not on the current list.
- rust-sdk: DaleSeo (Apollo GraphQL) made 147 commits and merged 84 of the last 100 PRs. DaleSeo is not on the list.

## 3. Risks

| Risk | go-sdk | rust-sdk |
|---|---|---|
| One person merges most PRs | Yes (90%) | Yes (84%) |
| Listed maintainers inactive | 2 of 4 have no commit in about 4 months or more | 3 of 5 have 3 or fewer commits |
| Maintainer list matches real work | Mostly | No. The main merger is not listed. |
| One company | Yes (Google) | No |
| Tier 1 triage target (label in 2 business days) | Not met. 23 issues have no label. 10 of them came from one audit on 2026-09-13. | Met |
| Valid bugs closed with no fix | Low risk | The stale bot can close them. Example: #784 (DCR secret) has the `Stale` label. |
| Relegation rule | "issues remain unaddressed for two months" can apply to the unlabeled backlog | No sign |

## 4. Third-party dependencies of the SDK

| SDK | Dependency | Use | Status |
|---|---|---|---|
| go-sdk | `segmentio/encoding` | JSON | 1 commit in 12 months, made by a go-sdk maintainer |
| go-sdk | `yosida95/uritemplate` | Resource templates | No commit since 2022-04-19 |
| go-sdk | `golang-jwt/jwt`, `google/jsonschema-go`, `golang.org/x/oauth2` | JWT, schema, OAuth | Active |
| rust-sdk | `oauth2` (ramosbugs) | OAuth | Active, 1 main maintainer, last push 2026-02-22 |
| rust-sdk | `reqwest`, `jsonwebtoken`, `tokio` | HTTP, JWT, runtime | Active |
| CLI | `zalando/go-keyring` | Credential store (Go) | Active, several authors |
| CLI | `keyring-rs` | Credential store (Rust) | Active |

## 5. Users of the client side

| SDK | MCP client that uses it | Version | Source |
|---|---|---|---|
| rust-sdk | Block goose (agent) | rmcp 3.4.1 | `Cargo.toml` |
| rust-sdk | OpenAI Codex (agent), crate `codex-rmcp-client` | rmcp 3.3.0, pinned to a git revision | `codex-rs/Cargo.toml` |
| go-sdk | Docker mcp-gateway | go-sdk v1.4.1 (no `2026-07-28`) | `go.mod` |
| go-sdk | lydakis/mcpx (small project) | go-sdk v1.7.0 | `go.mod` |

Two large MCP hosts use the rmcp client against many real servers and
authorization servers. No go-sdk client user of the same size was found in
this check.

## 6. Result

No item is a dealbreaker for either SDK. Both SDKs are Tier 1, both have the
same license, and both have one person who merges most PRs.

- go-sdk merges faster. Its weak points are the issue backlog, one company,
  and two small dependencies with little or no maintenance.
- rust-sdk has more contributors and more companies, and it labels all
  issues. Its weak points are the maintainer list that does not match the
  real work, and a stale bot that can close valid bugs.

## Sources

- [MCP SDKs](https://modelcontextprotocol.io/docs/sdk)
- [MCP SDK tiers](https://modelcontextprotocol.io/community/sdk-tiers)
- [Organization MAINTAINERS.md](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/MAINTAINERS.md)
- [go-sdk](https://github.com/modelcontextprotocol/go-sdk)
- [rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)
- [go-sdk unlabeled open issues](https://github.com/modelcontextprotocol/go-sdk/issues?q=is%3Aissue+is%3Aopen+no%3Alabel)
- [rust-sdk #784](https://github.com/modelcontextprotocol/rust-sdk/issues/784)
