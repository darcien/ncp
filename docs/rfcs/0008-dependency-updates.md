---
state: accepted
supersedes: []
---

# 0008: Dependency updates and security fixes

## Background

Outside the harness, `ncp` pins its dependencies in these files. A
person changes each pin by hand. No automated job updates them.

| File | Pin |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Crates. Exact pins: `rmcp`, `jsonc-parser`, `keyring-core`, `zbus-secret-service-keyring-store`, `secret-service` |
| `rust-toolchain.toml` | Rust 1.99.0 |
| `mise.toml`, `mise.lock` | node, cargo-deny, typos, hk, shellcheck, pnpm, git-cliff, cargo-release, cargo-insta |
| `hk.pkl` | The hk 2.5.0 configuration package. It must agree with `hk` in `mise.toml`. |
| `harness/linux/Dockerfile` | The digest of `debian:trixie-slim`. rustup-init 1.29.1 and its SHA-256 sums. |
| `.github/workflows/*.yaml` | Action commit SHAs with a version comment. `version: 2026.11.2` of mise-action. |

The harness packages (`harness/conformance`, `harness/interop`) first
need a change of their package manager.
[RFC 0009](0009-harness-dependency-updates.md) covers them. It adds to
the configuration of this RFC and changes no setting of it.

[docs/research/dependency-updates.md](../research/dependency-updates.md)
(2026-11-18) gives the state of the crates:

- `cargo update --dry-run --verbose` finds 74 updates inside the current
  ranges.
- Version requirements hold back eight crates. For example, the exact
  pin of `rmcp` holds 3.10.0, and 3.11.0 exists (2026-11-17).
- cargo-deny 0.20.2 is the only automated check. An advisory gives only
  a warning in the hook and in CI.

### The repository settings

`gh api` on 2026-11-18 and 2026-11-19 gave these settings of
`darcien/ncp`:

| Setting | Value |
|---|---|
| `has_pull_requests` | `false` |
| `pull_request_creation_policy` | `all` |
| `allow_auto_merge` | `false` |
| Branch protection and rulesets | none |
| Immutable releases | on: a published release locks its tag and assets |
| Dependabot alerts | on |
| Dependabot security updates | off |
| Permitted actions | GitHub actions, `jdx/mise-action@*`, `Swatinem/rust-cache@*`. Full SHA pins required. |

Dependabot has 15 open alerts, all for transitive npm packages of
`harness/interop/pnpm-lock.yaml`. The oldest is from 2026-10-30.
Nothing makes a change from an alert. RFC 0009 fixes them.

CI runs on a push to any branch, on Linux glibc x86_64 only. The hook
is the only check for macOS and Linux musl. A commit that a bot makes
does not run the hook.

### Cooldowns in Cargo, uv and mise

Rust 1.100.0 (2026-11-12) makes a minimum publish age stable in Cargo
([config][cargo-config], [resolver][cargo-resolver]).
`registry.global-min-publish-age` sets the age. The default of
`resolver.incompatible-publish-age` is `deny`:

> `deny`: ignore pubtime-incompatible versions unless they already exist
> in the lock file

Rust 1.99 warns that it ignores the key. The exception covers only the crates
that a command does not unlock. A full `cargo update` unlocks all
crates. With `rmcp = "3"` and a lock file at 3.11.0, Rust 1.100.0 gave
on 2026-11-18:

```text
$ CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="7 days" CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=deny cargo +1.100.0 update
 Downgrading rmcp v3.11.0 -> v3.9.0 (available: v3.11.0, published 21 hours ago)
```

Thus a full update with the age moves a young crate back. If the young
version is a vulnerability fix, the vulnerability comes back. With
`rmcp` 3.11.0 and `itoa` 1.0.10 in the lock file, and the same
variables, cargo gave:

```text
$ cargo +1.100.0 update -p itoa
     Locking 1 package to highest Rust 1.100.0 compatible version as of 7 days ago
    Updating itoa v1.0.10 -> v1.0.19
```

`rmcp` stayed at 3.11.0.

uv and mise have the same control for their own resolution:

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
(`gh pr list --author app/renovate`, 2026-11-18). Their presets give
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
  age to the package manager.
- Renovate does not update a transitive dependency for an alert
  ([minimum release age][rn-age], "What happens to transitive
  dependencies").
- [`vulnerabilityAlerts`][rn-options] reads the Dependabot alerts. A
  fix pull request ignores the schedule and the limits.
- The [app permissions][rn-perms] include write access to code, pull
  requests, issues, checks, commit statuses and workflows.
- The [Community Cloud][rn-mend] is free. It runs each active
  repository every 4 hours.

### Repository rules of uv and mise

`gh api repos/OWNER/REPO/rulesets` on 2026-11-18:

| Repository | Rules on `main` | Rules on tags | Pull request creation |
|---|---|---|---|
| astral-sh/uv | deletion, non-fast-forward, linear history, pull request, required checks | All tags: deletion, update, non-fast-forward. A tag requires the `release` deployment. | `all` |
| jdx/mise | deletion, non-fast-forward, pull request, required checks | none | `collaborators_only` |

## Problems

1. **No detection.** A new version or advisory shows only when a person
   runs a check.
2. **No cooldown.** `cargo update` and `cargo add` select a release that
   is minutes old, also as a new transitive crate. A malicious or broken
   release is most dangerous in its first days.
3. **One path for all updates.** A vulnerability fix waits until a
   person starts an update. An advisory with no fix shows only as a
   hook warning, after a commit that changes a dependency file.
4. **Linked pins.** Some pins must change together: `rmcp` with
   `sse-stream`, the three crates of the Secret Service store, and `hk`
   in `mise.toml` with `hk.pkl`.
5. **The repository cannot take a bot change safely.** Pull requests
   are off. Nothing protects `main`, or a tag before its release is
   published. A tag `vX.Y.Z` builds, attests and publishes a release.
   Thus write access to code is enough to publish.
6. **CI does not cover all targets.** A dependency can break the macOS
   or musl build, and a bot commit does not run the hook.
7. **Release notes.** git-cliff lists each `fix` commit. By default,
   Renovate gives `fix(deps)` to normal dependencies
   (`:semanticPrefixFixDepsChoreOthers`). Each routine update then makes
   a patch release.

## Determinations

### 1. Renovate as the Mend GitHub App

The maintainer installs the Mend Renovate Community Cloud app only for
`darcien/ncp`, as uv and mise do.

### 2. Repository settings

Before the app runs, set these:

```sh
gh api -X PATCH repos/darcien/ncp \
  -F has_pull_requests=true \
  -f pull_request_creation_policy=collaborators_only \
  -F delete_branch_on_merge=true
```

`collaborators_only` follows mise. `ncp` takes no outside pull requests
now, and this setting keeps that.

Add two rulesets with
`gh api -X POST repos/darcien/ncp/rulesets --input FILE`. The repository
admin role bypasses both, so the maintainer pushes to `main` and makes
tags as now. The app has no bypass.

```json
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "update" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
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

The write access of the app can push a tag or merge a pull request.
The rules stop both:

- `creation` on all tags stops the app from making a tag. It covers all
  tags, because the commit of a tag supplies its own workflow.
- `update` on tags stops the app from moving a tag before publication.
  Immutable releases lock a tag only after publication.
- `update` on `main` stops the app from merging its own pull request.
  A rule of 0 approvals does not stop it.

Write access also permits the app to create a release on an existing
tag. No ruleset stops this, and `ncp` accepts the risk. The `publish`
job then fails, because the release exists, so the maintainer sees it.

### 3. Two channels

| | Routine updates | Security fixes |
|---|---|---|
| Source | The registries | Dependabot alerts, and a daily `cargo deny check advisories` |
| When | Monday 00:00 to 03:59 UTC | A direct dependency: the next Renovate run, within 4 hours. A transitive crate: a manual change. |
| Cooldown | 7 days (section 4) | None |
| Shape | Groups (section 5) | One pull request for each vulnerable direct dependency, with the lowest fixed version |
| Commit | `build(deps): ...` | `fix(deps): ... [SECURITY]`, so the release notes list it |
| Label | none | `security` |

Renovate makes no pull request for a vulnerable transitive crate. The
daily advisory issue and the Dependabot alerts show it. Its fix is
`cargo update -p CRATE@LOCKED --precise VERSION`, with no age. If a
parent crate excludes the version, move the parent first.

Dependabot alerts stay on, because Renovate reads them. Dependabot
security updates stay off, so two bots do not change one lock file.

The new workflow `.github/workflows/advisories.yaml` runs
`cargo deny --locked check advisories` each day, as mise does
([deny-advisories.yml][mise-deny]). It finds what Renovate does not
fix: an unmaintained, unsound or yanked crate, an advisory with no fix,
and a vulnerable transitive crate. On a failure, the job opens one issue
or adds a comment to the open one. Then the run fails, and GitHub
notifies the maintainer.

```yaml
# Checks the advisories of the locked crates each day. The RustSec database
# changes when the repository does not, and the check of each push only warns.
name: advisories

on:
  schedule:
    - cron: "17 5 * * *"
  workflow_dispatch:

permissions: {}

concurrency:
  group: advisories
  cancel-in-progress: false

jobs:
  advisories:
    runs-on: ubuntu-24.04
    timeout-minutes: 15
    permissions:
      contents: read
      issues: write
    env:
      MISE_AUTO_INSTALL: "false"
      MISE_LOCKED: "1"
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false
      # cargo-deny runs cargo metadata, so the job needs the Rust toolchain.
      - run: rustup toolchain install
      - uses: jdx/mise-action@8418fedb1110381a0d3ba51a7a415d7c1b884550 # v5.2.2
        with:
          version: 2026.11.2
          install_args: cargo-deny
          cache: false
      - id: deny
        run: |
          set +e
          cargo deny --locked check advisories > "$RUNNER_TEMP/deny.txt" 2>&1
          echo "status=$?" >> "$GITHUB_OUTPUT"
          cat "$RUNNER_TEMP/deny.txt"
      # An advisory can stay for many days. Thus a failure adds a comment to the
      # open issue, and does not open a new issue each day.
      - if: steps.deny.outputs.status != '0'
        env:
          GH_TOKEN: ${{ github.token }}
          RUN_URL: ${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}
        run: |
          set -eu
          title="cargo deny check advisories failed"
          {
            echo "\`cargo deny check advisories\` failed on \`$GITHUB_REF_NAME\` ([run]($RUN_URL))."
            echo
            echo '```'
            head -c 50000 "$RUNNER_TEMP/deny.txt"
            echo '```'
          } > "$RUNNER_TEMP/body.md"
          number="$(gh issue list --repo "$GITHUB_REPOSITORY" --state open \
            --limit 100 --json number,title \
            --jq "map(select(.title == \"$title\")) | .[0].number // empty")"
          if [ -n "$number" ]; then
            gh issue comment "$number" --repo "$GITHUB_REPOSITORY" --body-file "$RUNNER_TEMP/body.md"
          else
            gh issue create --repo "$GITHUB_REPOSITORY" --title "$title" --body-file "$RUNNER_TEMP/body.md"
          fi
      - if: steps.deny.outputs.status != '0'
        run: exit 1
```

The checkout takes the ref of the event: `main` for the schedule, and
the selected branch for a manual run. The advisory check of the hook
and CI stays a warning (CONTRIBUTING.md, "Checks").

### 4. A cooldown of 7 days for routine updates

Renovate delays each routine update for 7 days: crates, GitHub Actions,
the mise tools and Rust. The Debian digest is the exception (section
5). A vulnerability fix does not wait, as in uv and mise. 7 days follows
mise and the CI check of uv.

`minimumReleaseAgePackageManagerSync` gives the age to cargo for a
routine update of `Cargo.lock`. Renovate sets
`CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE=7 days` and
`CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=deny`. Thus a new transitive
crate also waits 7 days. A vulnerability fix gets no age. Neither uv nor
mise gives an age to cargo, and Renovate marks this option as
experimental.

`Cargo.lock` gets no lock file maintenance, as in uv. A full update with
the age moves each young crate back, also a young fix (Background).
Thus a transitive crate moves only with an update of its parent, with
`mise run deps:update`, or with the manual fix of section 3.

Set `rust-toolchain.toml` to Rust 1.100.0, because older cargo ignores
both variables.

On the host, a person or an agent updates crates with a new mise task
that gives cargo the same age:

```toml
# cargo selects no crate version that is younger than 7 days, also for a
# new transitive crate. A malicious or broken release is most dangerous
# in its first days. The task needs one or more crates, because a run of
# cargo update with no crate unlocks all crates. Such a run moves each
# locked crate that is younger than 7 days back to an older version. If
# the newer version is a vulnerability fix, the vulnerability comes back.
# The value deny of CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE is the cargo
# default. The task sets it so that an allow in a cargo configuration
# cannot turn off the age.
[tasks."deps:update"]
description = "Update crates of Cargo.lock inside the version ranges, with versions that are 7 days old or older"
usage = 'arg "<crate>..." var=#true help="The crates to update. For a crate with two locked versions, give CRATE@VERSION."'
env = { CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE = "7 days", CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE = "deny" }
run = 'cargo update $(printf -- "-p %s " $usage_crate)'
```

cargo prints `Downgrading` for each crate that it moves back. Before a
commit, the person makes sure that no such line is a vulnerability fix.

### 5. The Renovate configuration

The file is `.github/renovate.jsonc`. Renovate reads this file name
([config file names][rn-filenames]), and its docs recommend `.jsonc` for
a configuration with comments ([options][rn-options]). The file sets
only values that differ from the defaults. These defaults apply:
`internalChecksFilter: "strict"`,
`minimumReleaseAgeBehaviour: "timestamp-required"`,
`semanticCommitScope: "deps"` and `lockFileMaintenance.enabled: false`.
Thus an update gets no branch before its cooldown, and an update with no
release time waits.

```jsonc
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  // Changes from config:best-practices:
  // - :pinDevDependencies is left out. It also matches the cargo dep type
  //   dev-dependencies, so it replaces each version range of the cargo
  //   dev-dependencies with an exact pin.
  // - security:minimumReleaseAgeNpm is left out. It sets 3 days in a
  //   package rule for npm packages, and that rule replaces the 7 days of
  //   this file.
  // - :maintainLockFilesWeekly is left out. With the age, a full update of
  //   Cargo.lock moves each crate younger than 7 days back to an older
  //   version, also a vulnerability fix.
  // - helpers:pinGitHubActionDigestsToSemver replaces
  //   helpers:pinGitHubActionDigests, so the comment of each action digest
  //   is a full version, for example v7.0.1.
  "extends": [
    "config:recommended",
    "docker:pinDigests",
    "helpers:pinGitHubActionDigestsToSemver",
    ":configMigration",
    "abandonments:recommended"
  ],
  // The harness packages stay out, because they first need a change of
  // their package manager.
  "enabledManagers": [
    "cargo",
    "rust-toolchain",
    "mise",
    "github-actions",
    "dockerfile",
    "custom.regex"
  ],
  // A vulnerability fix ignores the schedule.
  "schedule": ["* 0-3 * * 1"],
  // A routine update is a build change. It does not go into the release
  // notes, and it does not make a new version. config:recommended gives
  // the type fix to updates of normal dependencies, so this preset is off.
  "ignorePresets": [":semanticPrefixFixDepsChoreOthers"],
  "semanticCommits": "enabled",
  "semanticCommitType": "build",
  "minimumReleaseAge": "7 days",
  // Renovate gives the age to cargo when it updates Cargo.lock, so new
  // transitive crates wait too. A vulnerability fix gets no age.
  "minimumReleaseAgePackageManagerSync": true,
  "vulnerabilityAlerts": {
    // The type fix puts a vulnerability fix into the release notes.
    "semanticCommitType": "fix",
    "labels": ["security"]
  },
  "packageRules": [
    {
      "description": "Cargo treats a minor update of a 0.y crate as breaking, so such an update gets its own pull request",
      "matchManagers": ["cargo"],
      "matchUpdateTypes": ["minor", "patch"],
      "matchIsBreaking": false,
      "groupName": "rust crates"
    },
    {
      "matchManagers": ["mise"],
      "matchUpdateTypes": ["minor", "patch"],
      "groupName": "mise tools"
    },
    {
      "matchFileNames": [".github/workflows/**"],
      "matchUpdateTypes": ["minor", "patch", "digest", "pinDigest"],
      "groupName": "github actions"
    },
    {
      "description": "ncp gives the event type of sse-stream to rmcp, so ncp must use the sse-stream version of rmcp",
      "matchManagers": ["cargo"],
      "matchPackageNames": ["rmcp", "sse-stream"],
      "groupName": "rmcp"
    },
    {
      "description": "rmcp requires sse-stream 0.2. Remove this hold when rmcp accepts sse-stream 0.3.",
      "matchManagers": ["cargo"],
      "matchPackageNames": ["sse-stream"],
      "allowedVersions": "<0.3"
    },
    {
      "description": "The three crates have exact pins. ncp uses the secret-service version of the store and the keyring-core trait of the store, so the pins move together.",
      "matchManagers": ["cargo"],
      "matchPackageNames": ["keyring-core", "zbus-secret-service-keyring-store", "secret-service"],
      "groupName": "Secret Service store"
    },
    {
      "description": "hk.pkl loads the configuration package of the hk release in mise.toml",
      "matchPackageNames": ["jdx/hk"],
      "groupName": "hk"
    },
    {
      "description": "Renovate cannot find the release time of a digest of the tag trixie-slim, because the tag is not a version. Without this rule, the update waits forever. With it, the update gets no cooldown.",
      "matchManagers": ["dockerfile"],
      "matchUpdateTypes": ["digest"],
      "minimumReleaseAgeBehaviour": "timestamp-optional"
    }
  ],
  "customManagers": [
    {
      "customType": "regex",
      "managerFilePatterns": ["/^hk\\.pkl$/"],
      "matchStrings": [
        "package://github\\.com/jdx/hk/releases/download/v(?<currentValue>\\d+\\.\\d+\\.\\d+)/hk@\\d+\\.\\d+\\.\\d+#"
      ],
      "depNameTemplate": "hk",
      "packageNameTemplate": "jdx/hk",
      "datasourceTemplate": "github-releases",
      "extractVersionTemplate": "^v(?<version>.+)$",
      "autoReplaceStringTemplate": "package://github.com/jdx/hk/releases/download/v{{{newValue}}}/hk@{{{newValue}}}#"
    }
  ]
}
```

- A later rule replaces the group of an earlier rule. Thus an `rmcp`
  update goes into `rmcp`, not into `rust crates`.
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

A new document, `docs/dependencies.md`, gives the dependency rules, and
the task table of CONTRIBUTING.md links it.

`docs/dependencies.md` gets these steps for a Renovate pull request:

1. Make sure that the pull request changes only pin files:
   `crates/*/Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`,
   `mise.toml`, `mise.lock`, `hk.pkl`, `harness/linux/Dockerfile` or
   `.github/workflows/*.yaml` (`gh pr diff N --name-only`). Make sure
   that each commit is from `renovate[bot]`
   (`gh pr view N --json commits`). Do this before step 3, because
   step 3 runs the code of the branch.
2. Read the release notes of the updated packages and the change of the
   lock file. A new crate needs a reason, as a new direct dependency
   does. Do not merge a pull request that moves a package back to an
   older version.
3. Run `gh pr checkout N`, then `mise run precommit`. This runs the
   checks and tests on macOS and in the Linux musl container. CI tests
   only Linux glibc.
4. Merge with squash. If a `fix(deps)` pull request changes no input of
   the `ncp` binary, use the type `build` for the squash commit. The
   release notes then do not list it.

A separate change adds a CI job that runs the macOS tests and the musl
container tests. After that change, the job can replace step 3.

`docs/dependencies.md` also gets the cooldown rules, the rule for a hold
(section 7) and the daily advisory job (section 3). The cooldown rules
are:

- To update crates on the host, run `mise run deps:update CRATE...`,
  not `cargo update`. To find candidates, run
  `cargo update --dry-run --verbose`. A crate with two locked versions,
  for example `sha2`, needs `CRATE@VERSION`.
- cargo prints `Downgrading` for each crate that it moves back. If such
  a line moves back a vulnerability fix, run the command for a
  vulnerability fix below before the commit.
- To add a crate, run `cargo add` with the two variables of the task
  `deps:update` in `mise.toml`.
- For a vulnerability fix, run
  `cargo update -p CRATE@LOCKED --precise VERSION` with no age. For an
  exact pin, change `crates/*/Cargo.toml` first. If the version
  requirement of a parent crate excludes the fix, update the parent
  first.

### 7. Holds

A hold is a `packageRules` entry with `allowedVersions` or
`enabled: false`, as in uv and mise. Its `description` gives the reason
and the condition to remove it. This RFC adds one hold: `sse-stream`
stays below 0.3 until `rmcp` accepts 0.3. To close a pull request is
not a hold, because Renovate opens a closed group pull request again.

## Alternatives

| Alternative | Reason not chosen |
|---|---|
| Dependabot for all updates | It does not support mise ([list][dependabot-eco]). `mise.toml`, `mise.lock` and `hk.pkl` stay manual. |
| Dependabot security updates with Renovate | Two bots change one lock file. Renovate already reads the same alerts. |
| Self-hosted Renovate in GitHub Actions | It needs a token with write access as a secret, and a new permitted action. A push with `GITHUB_TOKEN` does not start CI. |
| Pull requests off, and Renovate merges branches itself | The bot needs push access to `main`, and CI does not cover macOS and musl. |
| A weekly report job and updates by hand | All file kinds stay manual. |
| One RFC with the harness packages | The harness first needs a change of its package manager. Two RFCs let each part land on its own. |
| A cooldown of 30 days, as uv for Renovate | `rmcp` gives fixes only in its newest major, and 30 days delays each spec change of the SDK. |
| A cooldown only in Renovate, with no age for cargo | It does not delay a new transitive crate. |
| `registry.global-min-publish-age` in `.cargo/config.toml` | It also covers a plain `cargo add`. But cargo then refuses a vulnerability fix younger than 7 days, and the maintainer must override the age. |
| The CI check of uv (`check-cargo-cooldowns`) | It needs a new tool, and it fails a young vulnerability fix unless the fix gets an exclusion. |
| Lock file maintenance of `Cargo.lock`, as mise | A full update with the age brings a young vulnerability fix back to the vulnerable version. |
| Automerge of minor and patch updates | CI does not cover macOS and musl. mise also turns it off. |
| The dependency rules in CONTRIBUTING.md, "Dependencies" | The steps of a task with its own document go into that document, as the release steps go into `docs/release.md`. |
| An advisory error that stops CI | The RustSec database changes when the repository does not. The daily job reports it. |
| An email from the advisory job, as mise | A new network destination and two secrets. GitHub already notifies the maintainer. |
| No semantic commits, as uv | CONTRIBUTING.md requires Conventional Commits. |
| `chore(deps)` for routine updates, as mise | A dependency is a build input, so `build` names it better. Both stay out of the release notes. |
| No bypass on `main` | The maintainer pushes to `main` directly, and the hook is the gate. A push of the maintainer then skips the required check. |
| No tag rule `creation`, as mise | The app can then push a tag `vX.Y.Z` and publish a release. |
| `13-slim` in place of `trixie-slim`. It has the same digest, and it is a version. | No Renovate run shows that Renovate ages a digest of `13-slim`. `trixie-slim` with `timestamp-optional` works now, and its warning shows each update. |
| Renovate updates `rust-version`, with an MSRV build in CI, as uv ([renovate.json5][uv-renovate]) | Out of scope of this RFC. `rust-version` stays manual, and CI gets no MSRV build. |
| `.github/renovate.json5`, as uv | The Renovate docs recommend `.jsonc` for a configuration with comments ([options][rn-options]). A config migration writes a `.json5` file with JSON5 and drops its comments. It keeps the comments of a `.jsonc` file ([migrated content][rn-migrated]). |
| `osvVulnerabilityAlerts`, which reads the OSV database for direct dependencies | Renovate marks it as experimental, and neither uv nor mise uses it. This RFC shows no advisory that OSV has and the Dependabot alerts do not. |
| `cargo vet` (uv), `cargo machete` (mise) | They solve other problems: audits of crate code, and unused crates. Each needs its own decision. |

## Tests

This RFC changes no code of `ncp`. These checks show that the design
works:

1. **`deps:update`.** After the change to Rust 1.100.0, run
   `mise run deps:update itoa`. cargo prints `as of 7 days ago` on its
   `Locking` line, also when it locks 0 packages. A run with no crate
   stops with a usage error. Thus the task gives cargo the age, and it
   cannot unlock all crates.
2. **Configuration.** Run `renovate-config-validator --strict` of
   Renovate 44.251.0 on `.github/renovate.jsonc`. This runs code from
   npm, so it needs the approval of the maintainer. Without it, the
   first run of the app shows each error in the Dependency Dashboard.
3. **Detection.** After the first run, the Dependency Dashboard lists
   `cargo` (two manifests), `rust-toolchain` and `mise` (nine tools). It
   also lists `github-actions` (five workflows, and the mise version of
   three), `dockerfile`, and `custom.regex` (`hk.pkl`). It lists no
   harness package.
4. **Security channel.** A Dependabot alert for a direct crate gives a
   Renovate pull request with the label `security` and the type `fix`.
5. **Advisory job.** Push a test branch that removes the `ignore` list
   of `deny.toml`. Run `gh workflow run advisories.yaml --ref BRANCH`.
   The run fails and opens one issue. A second run adds a comment. Then
   delete the branch and close the issue.
6. **Rulesets.** `gh api repos/darcien/ncp/rules/branches/main` lists
   `deletion`, `non_fast_forward`, `update`, `pull_request`, and
   `required_status_checks` with `check` from integration 15368. The
   first Renovate pull request shows that `collaborators_only` permits
   the app.

## Compatibility

- The output, the files and the stored credentials of `ncp` do not
  change. No release note is necessary.
- Rust 1.100.0 can add clippy warnings. The change that sets the
  toolchain fixes them.
- `rust-version` stays 1.89. After the change to Rust 1.100.0, no check
  builds with 1.89.
- A plain `cargo update` or `cargo add` still works, with no age.
  Nothing finds a young crate that such a command adds.
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
[dependabot-eco]: https://docs.github.com/en/code-security/dependabot/ecosystems-supported-by-dependabot/supported-ecosystems-and-repositories
[uv-renovate]: https://github.com/astral-sh/uv/blob/7f396534d58321e60b5ba204151326712d912ece/.github/renovate.json5
[mise-deny]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/.github/workflows/deny-advisories.yml
[astral-preset]: https://github.com/astral-sh/renovate-config/blob/cb60d1609c2b153539884743e2266006f64275f7/preset.json5
[jdx-preset]: https://github.com/jdx/renovate-config/blob/f29cd6d5584e3acad48f18cc6eba09604a5bc6bf/default.json
