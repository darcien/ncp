# Dependencies

Renovate opens pull requests for the updates of the pins. The workflow
`advisories` checks the advisories of the locked crates each day. A
person or an agent updates a transitive crate on the host, because
Renovate moves it only with its parent. rustup-init in
`harness/linux/Dockerfile` stays manual, because Renovate cannot compute
its SHA-256 sums. The maintainer merges each pull request.

## Cooldown

A malicious or broken release is most dangerous in its first days. Thus
a routine update uses only versions that are 7 days old or older.

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
  `cargo update -p CRATE@LOCKED --precise VERSION` with no cooldown.
  For an exact pin, change `crates/*/Cargo.toml` first. If the version
  requirement of a parent crate excludes the fix, update the parent
  first.

## Renovate

A routine update waits 7 days and gets the type `build`. The digest of
the Debian image does not wait, because the tag `trixie-slim` is not a
version and has no release time. A vulnerability fix does not wait. It
gets the type `fix` and the label `security`.

### Merge a Renovate pull request

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

### Hold a dependency at a version

To hold a dependency at a version, add a `packageRules` entry with
`allowedVersions` or `enabled: false` to `.github/renovate.jsonc`. In
its `description`, give the reason and the condition to remove it. Do
not close the pull request in place of a hold, because Renovate opens a
closed group pull request again.

## Advisories

The workflow `advisories` runs `cargo deny check advisories` on `main`
each day. On a failure, it opens the issue "cargo deny check advisories
failed", or adds a comment to the open one. The hook and CI show an
advisory only as a warning, so this issue is the report.

Renovate makes no pull request for a vulnerable transitive crate. Fix
it by hand (see "Cooldown"). To accept an advisory that has no fix, add
its ID to `ignore` in `deny.toml`. Give the reason and the condition to
remove it.

## Files of the dependency process

| File | Function |
|---|---|
| `.github/renovate.jsonc` | The configuration of Renovate: the groups, the cooldown, the commit types and the holds. |
| `mise.toml` | The task `deps:update`. It gives cargo the cooldown of 7 days. |
| `.github/workflows/advisories.yaml` | The daily advisory check. |
| `deny.toml` | The rules of cargo-deny. A new dependency must pass them. |
