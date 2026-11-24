---
state: committed
supersedes: []
---

# 0009: One pnpm workspace for the Node dependencies

## Background

[RFC 0008](0008-dependency-updates.md) brings Renovate to `ncp`, but not
to the workspace packages, that is the directories with a
`package.json`. They are tools of the test harness:

| Workspace package | Package manager | Files |
|---|---|---|
| `harness/conformance` | the npm CLI | `package.json`, `package-lock.json` |
| `harness/interop` | pnpm | `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml` |

`mise.toml` pins Node and pnpm. The sources are pnpm 12.21.0 and
Renovate 44.259.1. This RFC uses the [settings][pnpm-resolution] of
pnpm, [`pnpm audit`][pnpm-audit] and the [cooldown of
Renovate][rn-age] as their documents describe them. These facts are
specific to the design:

- pnpm checks each locked version (a version in the lock file) against
  the cooldown (`minimumReleaseAge`), also in a frozen install
  ([`trustLockfile`][pnpm-resolution]). In a test, an install with
  `--filter` also checked the locked versions of all workspace
  packages. Thus a locked version that is younger than the cooldown of
  a command stops the command. An exclusion (an item of
  `minimumReleaseAgeExclude`) permits a younger version.
- `minimumReleaseAgePackageManagerSync` of RFC 0008 gives pnpm the
  cooldown of 7 days for routine updates and lock file maintenance. A
  security update gets no cooldown from Renovate, so pnpm applies its
  own setting, by default 1 day.
- Renovate reads pnpm files with its manager `npm`. It updates a
  transitive dependency only with lock file maintenance. RFC 0008 keeps
  lock file maintenance off for `Cargo.lock`. The setting can be on for
  one Renovate manager only ([options][rn-options]).
- When a `package.json` pins pnpm, pnpm 12 adds a YAML document to the
  start of the lock file ([lock file][pnpm-lockfile]). The dependency
  graph of GitHub then finds no dependency in the lock file, so
  Dependabot gives no alerts ([Dependabot][gh-pnpm]).
- Renovate runs the pnpm of `mise.toml`, because no `package.json` pins
  pnpm and the lock file version `9.0` permits it
  ([mise manager][rn-mise]).

## Problems

1. Renovate does not read the workspace packages. Their dependencies
   change only by hand, and their alerts stay open.
2. Two package managers give two policies. The npm CLI has no trust
   policy, and its cooldown and its limit on install scripts are off by
   default.
3. Each workspace package has its own lock file. A fix of a shared
   transitive dependency changes each lock file.
4. `report.mjs` works only with the layout of the npm CLI:
   - It imports `yaml`, a dependency of the conformance suite. pnpm
     does not give `report.mjs` access to `yaml`.
   - It reads the suite version from `package-lock.json`.

## Determinations

### 1. One pnpm workspace at the root

All workspace packages of the repository are in one pnpm workspace.
These files are at the root:

- `pnpm-workspace.yaml`:
  - It lists each workspace package by path. Thus a new directory does
    not join by accident.
  - It sets only `minimumReleaseAge: 10080` and
    `trustPolicy: no-downgrade`. All other settings stay at the pnpm
    defaults. An explicit `minimumReleaseAge` makes pnpm strict by
    default ([`minimumReleaseAgeStrict`][pnpm-resolution]).
  - Renovate reads `minimumReleaseAge` and uses the larger of it and
    its own cooldown ([cooldown of Renovate][rn-age]). Thus no value
    needs a copy.
- `package.json`:
  - It has only `"private": true`.
  - For pnpm, GitHub lists `package.json` as an additional file of the
    dependency graph ([ecosystems][gh-graph]). This RFC keeps one at the
    root as a conservative choice. No test shows that it is necessary.
  - It sets no `packageManager`, `devEngines` or `engines`, because
    `mise.toml` pins pnpm and Node.
- `pnpm-lock.yaml`:
  - It replaces the lock files of the workspace packages.
  - Make it with `pnpm install --lockfile-only`.
- `.gitignore` has `node_modules/`, as the
  [Node template][gh-gitignore] of GitHub.

These files go away:

- `harness/conformance/package-lock.json`.
- The `pnpm-lock.yaml`, `pnpm-workspace.yaml` and `.gitignore` of
  `harness/interop`.
- The line `/node_modules` of `harness/conformance/.gitignore`.

Each workspace package has an install task in `mise.toml`:

- The task runs `pnpm install --frozen-lockfile --filter ./DIRECTORY`
  at the root.
- No task gives `--ignore-scripts`, because by default pnpm stops an
  install on a build script ([`strictDepBuilds`][pnpm-build]).

To add a workspace package, do these steps. If a direct dependency is
not from the MCP organization, first get a decision of the maintainer
(`AGENTS.md`, "Stop and ask").

1. Add its directory to `packages:`.
2. Run `pnpm install`.
3. Add its install task.

### 2. The harness

- `conformance:install` and `interop:install`:
  - They keep their names, so the hints in `run.sh` and
    `crates/testkit/tests/interop.rs` stay true.
  - They lose `dir`, and use the filters `./harness/conformance` and
    `./harness/interop`.
- pnpm links each direct dependency into the `node_modules` of its
  workspace package. Thus `run.sh` and
  `crates/testkit/tests/interop.rs` keep their paths.
- `report.mjs`:
  - It loads `yaml` through the suite, with
    `createRequire(realpathSync(join(suiteDir, "package.json")))("yaml")`.
  - `realpathSync` follows the link of pnpm to the suite.
  - It reads the suite version from the `package.json` of the installed
    suite.
- `report.test.mjs` makes the layout of pnpm. The suite is a link, and
  `yaml` is only beside the target of the link. Thus the test fails
  without `realpathSync`.
- The job `conformance`:
  - It installs `node pnpm`.
  - Its `paths` add the three files at the root.
- The comments on the npm CLI, Node and pnpm in `run.sh`, `mise.toml`
  and the job `conformance` change in the same commit.

### 3. Renovate

After section 1, `.github/renovate.jsonc` adds the Renovate manager
`npm` to `enabledManagers`, with this setting:

```jsonc
  "npm": { "lockFileMaintenance": { "enabled": true } },
```

Two comments change:

- The comment that keeps the harness out goes away.
- The comment above `minimumReleaseAgePackageManagerSync` names cargo
  and pnpm.

Then Renovate updates the direct dependencies with the cooldown of 7
days. Each Monday, lock file maintenance updates the transitive
dependencies with the same cooldown.

### 4. Transitive vulnerability fixes

All pnpm updates wait for the cooldown of 7 days. pnpm reads it from
`pnpm-workspace.yaml`, so it applies also to a security update and a
local command. Transitive fixes wait for the weekly lock file
maintenance. No overrides.

[docs/dependencies.md](../dependencies.md) gets a section "pnpm" with
the steps to add a workspace package, and a merge step that runs each
install task and its tests.

## Alternatives

| Alternative | Reason not chosen |
|---|---|
| A lock file for each workspace package, or the npm CLI | Two policies and two lock files. The npm CLI has no trust policy. |
| A workspace in `harness/` | A workspace package outside `harness/`, for example a documentation site, then needs a second workspace. |
| No root `package.json` | pnpm and Renovate work without it. The file has one line, so this RFC keeps it as a conservative choice. |
| A pin of pnpm in `package.json` | The dependency graph then finds no dependency, and Dependabot gives no alerts. `mise.toml` already pins pnpm. |
| `yaml` as a direct dependency | It needs a decision of the maintainer (`AGENTS.md`). Renovate then updates it apart from the suite. |
| `publicHoistPattern: [yaml]` | `report.mjs` then uses a dependency that it does not declare. No test fails when the suite stops using `yaml`. |
| Dependabot security updates | This adds a second bot. Its fix of a transitive dependency changes only the lock file. Then lock file maintenance moves a young fix back. |
| Overrides for a young transitive fix (`pnpm audit --fix`) | Each override needs a hand removal later. A young override stops lock file maintenance. Test tools can wait. |
| The pnpm default cooldown of 1 day | A security update can then lock a version younger than 7 days. That version stops the pnpm updates of Renovate, and each local command needs `--config.minimum-release-age=10080`. |

## Tests

1. The command of section 1 makes one lock file with the importers `.`,
   `harness/conformance` and `harness/interop`. `pnpm audit` finds no
   advisory, or section 4 applies.
2. The install task of each workspace package passes with no
   `allowBuilds` item. `mise run test`, `mise run conformance` and
   `mise run interop` pass.
3. The test of `report.mjs` fails without `realpathSync`.
4. The Dependency Dashboard lists the Renovate manager `npm` with each
   workspace package. The log of lock file maintenance shows
   `--config.minimum-release-age=10080`.
5. After the merge, the dependency graph lists `pnpm-lock.yaml`. No
   Dependabot alert of the old lock files stays open.

## Compatibility

- The output and the files of `ncp` do not change.
- The install tasks need pnpm from `mise.toml`.
- Before the first install, delete the local `harness/*/node_modules`.

[pnpm-resolution]: https://github.com/pnpm/pnpm/blob/b8d388835f9a40ee1ac53fbc2d4b58c3ebfa5600/pnpm/docs/settings/dependency-resolution.md
[pnpm-build]: https://github.com/pnpm/pnpm/blob/b8d388835f9a40ee1ac53fbc2d4b58c3ebfa5600/pnpm/docs/settings/build.md#strictdepbuilds
[pnpm-audit]: https://github.com/pnpm/pnpm/blob/b8d388835f9a40ee1ac53fbc2d4b58c3ebfa5600/pnpm/docs/cli/audit.md
[pnpm-lockfile]: https://github.com/pnpm/pnpm/blob/b8d388835f9a40ee1ac53fbc2d4b58c3ebfa5600/pnpm/docs/lockfile.md
[rn-age]: https://github.com/renovatebot/renovate/blob/44.259.1/docs/usage/key-concepts/minimum-release-age.md
[rn-options]: https://github.com/renovatebot/renovate/blob/44.259.1/docs/usage/configuration-options.md
[rn-mise]: https://github.com/renovatebot/renovate/blob/44.259.1/lib/modules/manager/mise/readme.md
[gh-pnpm]: https://github.com/github/docs/blob/e0b0c900ea2465ce1697fdbef73debd206d0274e/data/reusables/dependabot/supported-package-managers.md
[gh-graph]: https://github.com/github/docs/blob/e0b0c900ea2465ce1697fdbef73debd206d0274e/content/code-security/reference/supply-chain-security/dependency-graph-supported-package-ecosystems.md
[gh-gitignore]: https://github.com/github/gitignore/blob/eeb775f7cbd229e8cd487ef06a1176dc239201d9/Node.gitignore
