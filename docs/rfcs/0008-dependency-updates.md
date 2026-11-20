---
state: committed
supersedes: []
---

# 0008: Dependency updates and vulnerability fixes

## Background

Before this RFC, outside the harness, `ncp` pinned its dependencies in
these files. No automated job updated them.

| File | Pin |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Crates. Some have exact pins. |
| `rust-toolchain.toml` | The Rust toolchain |
| `mise.toml`, `mise.lock` | The tools other than Rust. |
| `hk.pkl` | The hk configuration package. It must agree with `hk` in `mise.toml`. |
| `harness/linux/Dockerfile` | The digest of `debian:trixie-slim`. rustup-init and its SHA-256 sums. |
| `.github/workflows/*.yaml` | Action commit SHAs with a version comment. The `version:` input of mise-action. |

The harness packages (`harness/conformance`, `harness/interop`) first
need a change of their package manager.
[RFC 0009](0009-harness-dependency-updates.md) covers them. It adds to
the Renovate configuration of this RFC and changes no setting of it.

[docs/research/dependency-updates.md](../research/dependency-updates.md)
gives the state of the crates:

- `cargo update --dry-run --verbose` finds updates inside the current
  ranges.
- Version requirements keep some crates at older versions, for example
  the exact pin of `rmcp`.
- Before this RFC, cargo-deny was the only automated check. An advisory
  gave only a warning in the hook and in CI.

### The repository settings

Before this RFC, `gh api` on 2026-11-18 and 2026-11-19 gave these
settings of `darcien/ncp`:

- Pull requests are off (`has_pull_requests` is `false`).
  `pull_request_creation_policy` is `all`.
- The repository has no branch protection and no rulesets.
- Immutable releases are on. They lock the tag and the assets of a
  release only after the release is published.
- Dependabot alerts are on, and Dependabot security updates are off.
- The permitted actions are the GitHub actions, `jdx/mise-action@*` and
  `Swatinem/rust-cache@*`, pinned to full commit SHAs.

CI runs on a push to any branch, on Linux glibc x86_64 only. The hook
is the only check for macOS and Linux musl. A commit that a bot makes
does not run the hook.

### Cooldowns in Cargo, uv and mise

Rust 1.100.0 makes a cooldown stable in Cargo
([config][cargo-config], [resolver][cargo-resolver]).
`registry.global-min-publish-age` sets the cooldown. The default of
`resolver.incompatible-publish-age` is `deny`:

> `deny`: ignore pubtime-incompatible versions unless they already exist
> in the lock file

Rust 1.99, the toolchain before this RFC, warns that it ignores the
key. The lock file exception covers only the crates that a command does
not unlock. A full `cargo update` unlocks all
crates. With `rmcp = "3"` and a lock file at 3.11.0, Rust 1.100.0 gave
on 2026-11-18:

```text
$ CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="7 days" CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=deny cargo +1.100.0 update
 Downgrading rmcp v3.11.0 -> v3.9.0 (available: v3.11.0, published 21 hours ago)
```

Thus a full update with the cooldown moves a version that is
younger than 7 days back. If that version is a vulnerability fix, the
vulnerability comes back. With `rmcp` 3.11.0 and `itoa` 1.0.10 in the
lock file, and the same variables, cargo gave:

```text
$ cargo +1.100.0 update -p itoa
     Locking 1 package to highest Rust 1.100.0 compatible version as of 7 days ago
    Updating itoa v1.0.10 -> v1.0.19
```

`rmcp` stayed at 3.11.0.

uv and mise also have a cooldown for the packages that they resolve:

```text
$ uv --version
uv 0.13.9 (Homebrew 2026-11-17 aarch64-apple-darwin)
$ uv help lock
      --exclude-newer <EXCLUDE_NEWER>
          Limit candidate packages to those that were uploaded prior to the given date.
          [...] a "friendly"
          duration (e.g., `24 hours`, `1 week`, `30 days`), or an ISO 8601 duration (e.g., `PT24H`,
          `P7D`, `P30D`).
$ mise --version
2026.11.9 macos-arm64 (2026-11-16)
$ mise settings ls --all | grep release_age
minimum_release_age_excludes                []
```

The [`minimum_release_age`][mise-settings] setting of mise has a default
of `24h`. It does not filter exact versions or `mise.lock`. All tools of
`ncp` have exact versions.

### Renovate

uv and mise use the Mend Renovate GitHub App
(`gh pr list --author app/renovate`, 2026-11-18). Their presets set
their cooldown and lock file maintenance ([uv preset][astral-preset],
[mise preset][jdx-preset]). mise turns off the automerge of its preset.
The facts below come from the documentation of Renovate 44.251.0:

- The [`mise` manager][rn-mise] updates `mise.toml` and runs `mise lock`.
- The [`rust-toolchain` manager][rn-toolchain] updates
  `rust-toolchain.toml`. The `cargo` manager uses that Rust for
  `cargo update`.
- The [`cargo` manager][rn-cargo] changes only `Cargo.lock` for an
  update inside the range.
- [`minimumReleaseAge`][rn-age] delays an update. A vulnerability fix
  does not wait. `minimumReleaseAgePackageManagerSync` also gives the
  cooldown to the package manager.
- Renovate does not update a transitive dependency for an alert
  ([minimum release age][rn-age], "What happens to transitive
  dependencies").
- [`vulnerabilityAlerts`][rn-options] reads the Dependabot alerts. The
  pull request of a vulnerability fix ignores the schedule and the limits.
- The [permissions][rn-perms] of the Renovate app include write access
  to code, pull requests, issues, checks, commit statuses and workflows.
- The [Community Cloud][rn-mend] is free. It schedules each repository
  by its Renovate status ([job scheduling][rn-schedule]).

### Repository rules of uv and mise

`gh api repos/OWNER/REPO/rulesets` on 2026-11-18:

| Repository | Rules on `main` | Rules on tags | Pull request creation |
|---|---|---|---|
| astral-sh/uv | deletion, non-fast-forward, linear history, pull request, required checks | All tags: deletion, update, non-fast-forward. A tag requires the `release` deployment. | `all` |
| jdx/mise | deletion, non-fast-forward, pull request, required checks | none | `collaborators_only` |

## Problems

1. **No detection.** A new version or advisory shows only when a person
   runs a check.
2. **No cooldown.** `cargo update` and `cargo add` select a version that
   is minutes old, also for a new transitive crate. A malicious or broken
   release is most dangerous in its first days.
3. **One path for all updates.** A vulnerability fix waits until a person
   starts an update. An advisory with no fix shows only as a warning of
   the hook, after a commit that changes a dependency file.
4. **Linked pins.** Some pins must change together: `rmcp` with
   `sse-stream`, the three crates of the Secret Service store, and `hk`
   in `mise.toml` with `hk.pkl`.
5. **The repository cannot accept bot changes safely.** Pull requests
   are off. Nothing protects `main`, or a tag before its release is
   published. A tag `vX.Y.Z` builds, attests and publishes a release.
   Thus write access to code is enough to publish a release.
6. **CI does not cover all targets.** A dependency can break the macOS
   or musl build, and a bot commit does not run the hook.
7. **Release notes.** git-cliff lists each `fix` commit. By default,
   Renovate gives the type `fix` to normal dependencies
   (`:semanticPrefixFixDepsChoreOthers`). Each routine update then makes
   a patch release.

## Determinations

### 1. Renovate as the Mend GitHub App

The maintainer installs the Mend Renovate Community Cloud app only for
`darcien/ncp`, as uv and mise do.

### 2. Repository settings

Before Renovate runs, set these settings:

```sh
gh api -X PATCH repos/darcien/ncp \
  -F has_pull_requests=true \
  -f pull_request_creation_policy=collaborators_only \
  -F delete_branch_on_merge=true
```

`collaborators_only` follows mise. `ncp` takes no outside pull requests
now, and this setting keeps that.

Add two rulesets with
`gh api -X POST repos/darcien/ncp/rulesets --input FILE`. The `main`
ruleset does not apply to the repository admin role (`bypass_mode`
`exempt`). Thus the maintainer pushes to `main` and merges into `main`
with no extra step. With `always`, GitHub applies the rules to the
maintainer too. It then blocks each merge until the maintainer bypasses
the rules, also when the required check passes. The UI steps in the
GitHub docs name only "Always allow" and "For pull requests only"
([ruleset docs][gh-rules-ui]). Thus set `exempt` with the API. With
`exempt`, "a bypass audit entry will not be created"
([REST docs][gh-rules-api]). The `tags` ruleset uses `always`, because a
tag has no merge step. Thus the maintainer makes tags as now. Renovate
has no bypass.

```json
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "exempt" }],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "update" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "required_reviewers": [],
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
        "require_extra_approval_for_unattributed_changes": true,
        "allowed_merge_methods": ["squash"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "do_not_enforce_on_create": false,
        "required_status_checks": [{ "context": "check", "integration_id": 15368 }]
      }
    }
  ]
}
```

```json
{
  "name": "tags",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~ALL"], "exclude": [] } },
  "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }],
  "rules": [
    { "type": "creation" },
    { "type": "update" },
    { "type": "deletion" },
    { "type": "non_fast_forward" }
  ]
}
```

The write access of Renovate can push a tag or merge a pull request.
The rules stop both:

- `creation` on all tags stops Renovate from making a tag. It covers all
  tags, because the commit of a tag supplies its own workflow.
- `update` on tags stops Renovate from moving a tag before the release
  is published. Immutable releases lock a tag only after the release is
  published.
- `update` on `main` stops Renovate from merging its own pull request.
  A rule of 0 approvals does not stop it.

Write access also lets Renovate create a release on an existing tag.
No ruleset stops this, and `ncp` accepts the risk. The `publish` job
then fails, because the release exists, so the maintainer sees it.

### 3. Two channels

| | Routine updates | Vulnerability fixes |
|---|---|---|
| Source | The registries | Dependabot alerts, and a daily `cargo deny check advisories` |
| When | The weekly `schedule` of Renovate | A direct dependency: the next Renovate run. A transitive crate: a manual update. |
| Cooldown | 7 days (section 4) | None |
| Pull requests | Groups (section 5) | One pull request for each vulnerable direct dependency, with the lowest fixed version |
| Commit | `build(deps): ...` | `fix(deps): ... [SECURITY]`, so the release notes list it |
| Label | none | `security` |

Renovate makes no pull request for a vulnerable transitive crate. The
daily advisory issue and the Dependabot alerts show it. A person updates
it with `cargo update --precise` and no cooldown
([docs/dependencies.md](../dependencies.md), "Cooldown").

Dependabot alerts stay on, because Renovate reads them. Dependabot
security updates stay off, so two bots do not change one lock file.

The workflow
[.github/workflows/advisories.yaml](../../.github/workflows/advisories.yaml)
runs `cargo deny --locked check advisories` each day, as mise does
([deny-advisories.yml][mise-deny]). It finds what Renovate does not
fix: an unmaintained, unsound or yanked crate, an advisory with no fix,
and a vulnerable transitive crate. On a failure, the job opens one issue
or adds a comment to the open one. Then the run fails, and GitHub
notifies the maintainer. The checkout takes the ref of the event:
`main` for the schedule, and the selected branch for a manual run. The
advisory check of the hook and CI stays a warning (CONTRIBUTING.md,
"Checks").

### 4. A cooldown of 7 days for routine updates

Renovate delays each routine update for 7 days: crates, GitHub Actions,
the mise tools and Rust. The Debian digest is the exception (section
5). A vulnerability fix does not wait, as in uv and mise. 7 days follows
mise and the CI check of uv.

`minimumReleaseAgePackageManagerSync` gives the cooldown to cargo for a
routine update of `Cargo.lock`. Renovate sets
`CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE=7 days` and
`CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=deny`. Thus a new transitive
crate also waits 7 days. A vulnerability fix gets no cooldown in cargo.
Neither uv nor mise gives a cooldown to cargo, and Renovate marks this
option as experimental.

`Cargo.lock` gets no lock file maintenance, as in uv. A full update with
the cooldown moves each version younger than 7 days back, also a
vulnerability fix (Background). Thus a transitive crate changes only
with an update of its parent, with `mise run deps:update`, or with the
manual update of section 3.

Set `rust-toolchain.toml` to Rust 1.100.0, because older cargo ignores
both variables.

On the host, a person or an agent updates crates with the task
`deps:update` of [mise.toml](../../mise.toml). It gives cargo the same
cooldown. Its comment gives the reasons for its variables and for its
required crate arguments. cargo prints `Downgrading` for each crate that
it moves back. Before a commit, the person or the agent makes sure that
no such line is a vulnerability fix.

### 5. The Renovate configuration

The file is [.github/renovate.jsonc](../../.github/renovate.jsonc).
Renovate reads this file name ([config file names][rn-filenames]), and
its docs recommend `.jsonc` for a configuration with comments
([options][rn-options]). The file sets only values that differ from the
defaults. Its comments and rule descriptions give the reasons for the
changes from the presets, the commit types, the linked pins and the
holds. These defaults apply:
`internalChecksFilter: "strict"`,
`minimumReleaseAgeBehaviour: "timestamp-required"`,
`semanticCommitScope: "deps"` and `lockFileMaintenance.enabled: false`.
Thus Renovate makes no branch for an update before its cooldown ends,
and an update with no release time waits.

- A later rule replaces the group of an earlier rule. Thus an `rmcp`
  update goes into the group `rmcp`, not into `rust crates`.
- A major update gets its own pull request. So does a minor update of a
  `0.y` crate, because it can need a change of code.
- Renovate reads the `version:` input of `jdx/mise-action` itself.
- The Debian digest has no cooldown
  ([minimum release age][rn-age], "Digest updates"). The image builds
  the Linux test container and the Linux release binaries
  (`harness/release.sh linux`). Renovate logs a warning for each such
  update, and the Dependency Dashboard shows it.
- rustup-init in the Dockerfile stays manual, because Renovate cannot
  compute its SHA-256 sums.
- The configuration does not change `rust-version` in `Cargo.toml`.
- The Dependency Dashboard stays on, as in uv. It shows the updates that
  wait for their cooldown, the errors, and a checkbox to start an update.

### 6. No automerge, and a check on the host before a merge

Renovate does not merge, as in uv and mise. CI does not build for macOS
or musl, so CI alone cannot accept an update.

[docs/dependencies.md](../dependencies.md) gives the dependency rules,
and the task table of CONTRIBUTING.md links it. It holds the cooldown
rules, the rule for a hold and the daily advisory job. It also gives
the steps to merge a Renovate pull request. In these steps, the
maintainer checks the files and the author of each commit, and reads
the release notes. Then the maintainer runs
`mise run precommit` on the host, and merges with squash.

A separate change adds a CI job that runs the macOS tests and the musl
container tests. After that change, the job can replace the check on
the host.

### 7. Holds

A hold is a `packageRules` entry, as in uv and mise
([docs/dependencies.md](../dependencies.md), "Hold a dependency at a
version"). This RFC adds one hold: `sse-stream` stays below 0.3 until
`rmcp` accepts 0.3.

## Alternatives

| Alternative | Reason not chosen |
|---|---|
| Dependabot for all updates | It does not support mise ([list][dependabot-eco]). `mise.toml`, `mise.lock` and `hk.pkl` stay manual. |
| Dependabot security updates with Renovate | Two bots change one lock file. Renovate already reads the same alerts. |
| Self-hosted Renovate in GitHub Actions | It needs a token with write access as a secret, and a new permitted action. A push with `GITHUB_TOKEN` does not start CI. |
| Pull requests off, and Renovate merges branches itself | Renovate needs push access to `main`, and CI does not cover macOS and musl. |
| A weekly report job and updates by hand | All file kinds stay manual. |
| One RFC with the harness packages | The harness first needs a change of its package manager. With two RFCs, each part can go to `main` separately. |
| A cooldown of 30 days, as uv for Renovate | `rmcp` gives fixes only in its newest major version, and 30 days delays each spec change of the SDK. |
| A cooldown only in Renovate, and none in cargo | It does not delay a new transitive crate. |
| `registry.global-min-publish-age` in `.cargo/config.toml` | It also covers a plain `cargo add`. But cargo then refuses a vulnerability fix younger than 7 days, and the maintainer must override the cooldown. |
| The CI check of uv (`check-cargo-cooldowns`) | It needs a new tool, and it fails a vulnerability fix younger than 7 days unless the fix gets an exclusion. |
| Lock file maintenance of `Cargo.lock`, as mise | A full update with the cooldown moves a vulnerability fix younger than 7 days back to the vulnerable version. |
| Automerge of minor and patch updates | CI does not cover macOS and musl. mise also turns it off. |
| The dependency rules in CONTRIBUTING.md, "Dependencies" | The steps of a task with its own document go into that document, as the release steps go into `docs/release.md`. |
| An advisory error that stops CI | The RustSec database changes when the repository does not. The daily job reports it. |
| An email from the advisory job, as mise | A new network destination and two secrets. GitHub already notifies the maintainer. |
| No semantic commits, as uv | CONTRIBUTING.md requires Conventional Commits. |
| `chore(deps)` for routine updates, as mise | A dependency is a build input, so `build` names it better. Both stay out of the release notes. |
| No bypass on `main` | The maintainer pushes to `main` directly, and the hook checks each commit. A push of the maintainer then skips the required check. |
| `bypass_mode` `always` on `main` | Both modes stop Renovate in the same way, and the maintainer is the only admin. With `always`, GitHub blocks each merge of the maintainer until the maintainer bypasses the rules. |
| No tag rule `creation`, as mise | Renovate can then push a tag `vX.Y.Z` and publish a release. |
| `13-slim` in place of `trixie-slim`. It has the same digest, and it is a version. | No Renovate run shows that Renovate applies the cooldown to a digest of `13-slim`. `trixie-slim` with `timestamp-optional` works now, and its warning shows each update. |
| Renovate updates `rust-version`, with an MSRV build in CI, as uv ([renovate.json5][uv-renovate]) | Out of scope of this RFC. `rust-version` stays manual, and CI gets no MSRV build. |
| `.github/renovate.json5`, as uv | The Renovate docs recommend `.jsonc` for a configuration with comments ([options][rn-options]). A configuration migration writes a `.json5` file with JSON5 and drops its comments. It keeps the comments of a `.jsonc` file ([migrated content][rn-migrated]). |
| `osvVulnerabilityAlerts`, which reads the OSV database for direct dependencies | Renovate marks it as experimental, and neither uv nor mise uses it. This RFC shows no advisory that OSV has and the Dependabot alerts do not. |
| `cargo vet` (uv), `cargo machete` (mise) | They solve other problems: audits of crate code, and unused crates. Each needs its own decision. |

## Tests

This RFC changes no code of `ncp`. These checks show that the design
works:

1. **`deps:update`.** Run `mise run deps:update itoa`. cargo prints
   `as of 7 days ago` on its `Locking` line, also when it locks 0
   packages. A run with no crate stops with a usage error. Thus the task
   gives cargo the cooldown, and it cannot unlock all crates.
2. **Configuration.** Run `renovate-config-validator --strict` of
   Renovate 44.251.0 on `.github/renovate.jsonc`. This runs code from
   npm, so it needs the approval of the maintainer. On a configuration
   error, Renovate opens the issue "Action Required: Fix Renovate
   Configuration" and stops its pull requests.
3. **Detection.** The Dependency Dashboard lists each manager of
   `enabledManagers` in `.github/renovate.jsonc` (`custom.regex` as
   `regex`), and no harness package. For each workflow with
   `jdx/mise-action`, the `github-actions` list also shows its
   `version:` input as `jdx/mise`.
4. **Vulnerability fixes.** A Dependabot alert for a direct crate gives a
   Renovate pull request with the label `security` and the type `fix`.
5. **Advisory job.** Push a test branch that removes the `ignore` list
   of `deny.toml`. Run `gh workflow run advisories.yaml --ref BRANCH`.
   The run fails and opens one issue. A second run adds a comment to
   it. Then delete the branch and close the issue.
6. **Rulesets.** `gh api repos/darcien/ncp/rules/branches/main` lists
   the rules of the `main` ruleset of section 2. Renovate can open a
   pull request with `collaborators_only`.

## Compatibility

- The output, the files and the stored credentials of `ncp` do not
  change. No release note is necessary.
- Rust 1.100.0 can add clippy warnings. The change that sets the
  toolchain fixes them.
- `rust-version` stays 1.89. After the change to Rust 1.100.0, no check
  builds with 1.89.
- A plain `cargo update` or `cargo add` still works, with no cooldown.
  Nothing finds a crate younger than 7 days that such a command adds.
- Pull requests become possible, only for collaborators.
- The maintainer pushes to `main`, and makes and deletes tags as now
  (`docs/release.md`).
- The harness packages do not change until RFC 0009.

[cargo-config]: https://github.com/rust-lang/cargo/blob/3acc79c0c841844675bc53a605922dbfc441e750/doc/book/src/reference/config.md#registryglobal-min-publish-age
[cargo-resolver]: https://github.com/rust-lang/cargo/blob/3acc79c0c841844675bc53a605922dbfc441e750/doc/book/src/reference/resolver.md#publish-age
[mise-settings]: https://github.com/jdx/mise/blob/af6f6e57c930af2250b0e6bc02809b5247e48308/settings.toml
[rn-mise]: https://github.com/renovatebot/renovate/blob/44.251.0/lib/modules/manager/mise/readme.md
[rn-toolchain]: https://github.com/renovatebot/renovate/blob/44.251.0/lib/modules/manager/rust-toolchain/readme.md
[rn-cargo]: https://github.com/renovatebot/renovate/blob/44.251.0/lib/modules/manager/cargo/readme.md
[rn-age]: https://github.com/renovatebot/renovate/blob/44.251.0/docs/usage/key-concepts/minimum-release-age.md
[rn-options]: https://github.com/renovatebot/renovate/blob/44.251.0/docs/usage/configuration-options.md
[rn-migrated]: https://github.com/renovatebot/renovate/blob/44.251.0/lib/config/migrated-content.ts
[rn-filenames]: https://github.com/renovatebot/renovate/blob/44.251.0/lib/config/app-strings.ts
[rn-perms]: https://github.com/renovatebot/renovate/blob/44.251.0/docs/usage/security-and-permissions.md
[rn-mend]: https://github.com/renovatebot/renovate/blob/44.251.0/docs/usage/mend-hosted/overview.md
[rn-schedule]: https://github.com/renovatebot/renovate/blob/44.251.0/docs/usage/mend-hosted/job-scheduling.md
[dependabot-eco]: https://docs.github.com/en/code-security/dependabot/ecosystems-supported-by-dependabot/supported-ecosystems-and-repositories
[uv-renovate]: https://github.com/astral-sh/uv/blob/7f396534d58321e60b5ba204151326712d912ece/.github/renovate.json5
[mise-deny]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/.github/workflows/deny-advisories.yml
[astral-preset]: https://github.com/astral-sh/renovate-config/blob/cb60d1609c2b153539884743e2266006f64275f7/preset.json5
[jdx-preset]: https://github.com/jdx/renovate-config/blob/f29cd6d5584e3acad48f18cc6eba09604a5bc6bf/default.json
[gh-rules-api]: https://docs.github.com/en/rest/repos/rules
[gh-rules-ui]: https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/creating-rulesets-for-a-repository
