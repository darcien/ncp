# Dependencies

Renovate updates the pins. Transitive crates and rustup-init in
`harness/linux/Dockerfile` are updated by hand. The maintainer merges
each pull request.

## Cooldown

A routine update uses only versions that are 7 days old or older.

- Update crates with `mise run deps:update CRATE...`, not
  `cargo update`. A crate with two locked versions needs
  `CRATE@VERSION`.
- Add a crate with `cargo add` and the two variables of the task
  `deps:update` in `mise.toml`. Give the reason in the commit body.
- For a vulnerability fix, run
  `cargo update -p CRATE@LOCKED --precise VERSION`, with no cooldown.
  If the requirement of a parent crate excludes the fix, update the
  parent first.
- If a `Downgrading` line of cargo moves a crate back from a
  vulnerability fix, run the fix again before the commit.

## Renovate

To merge a pull request:

1. Read the release notes and the diff.
2. On the branch, run `mise run precommit`. CI tests only Linux glibc.
   This also tests macOS and Linux musl.
3. Merge with squash. Use the type `build` when a `fix(deps)` pull
   request changes no input of the `ncp` binary.

To hold a dependency, add a `packageRules` entry to
`.github/renovate.jsonc`, with the reason and the condition to remove
it. Do not close the pull request, because Renovate opens it again.

## Advisories

The workflow `advisories` opens one issue for each new advisory ID each
day. The hook and CI only warn.

The workflow skips an ID while its issue is open, so keep the ID at the
start of the title. Close the issue with `Closes #N` in the fix.

Another error fails the run, for example a yanked crate or an unused
entry in `ignore`. In the log, find `"severity":"error"`.

To accept an advisory that has no fix, add its ID to `ignore` in
`deny.toml`, with the reason and the condition to remove it.
