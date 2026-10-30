# Release

A tag `vX.Y.Z` on `main` starts `.github/workflows/release.yaml`. The
workflow builds, attests and publishes the release.

## Publish a release

1. Pull `main` and the tags from GitHub: `git pull --ff-only --tags`.
2. Run the live tests: `mise run live`.
3. Do a dry run: `mise run release:tag`. It shows the next version and
   the release notes.
4. Publish: `mise run release:tag --execute`.

To use a different version, give it in steps 3 and 4. Example:
`mise run release:tag 1.0.0 --execute`.

## When the release workflow fails

Do not run `release:tag` again. It adds a second X.Y.Z section to
`CHANGELOG.md`.

If the job `publish` made a draft release, delete the draft:
`gh release delete vX.Y.Z --repo darcien/ncp --yes`.

If the cause is temporary, for example a network fault, run the failed
jobs again: `gh run rerun <run-id> --failed`.

If the cause is not temporary:

1. Delete the tag on GitHub: `git push origin :refs/tags/vX.Y.Z`.
2. Delete the tag on the local machine: `git tag -d vX.Y.Z`.
3. Correct the fault on `main`.
4. If the correction is a `feat` or `fix` commit, add it to the X.Y.Z
   section of `CHANGELOG.md`. Commit the change.
5. Make the tag again: `git tag -a vX.Y.Z -m "ncp X.Y.Z"`.
6. Push `main` and the tag: `git push --atomic origin main vX.Y.Z`.

## Files of the release process

| File | Function |
|---|---|
| `CHANGELOG.md` | One section for each version. The section of a version is its release notes. |
| `cliff.toml` | git-cliff writes the section from the `feat`, `fix` and breaking commits. It also finds the next version from these commits. |
| `release.toml` | cargo-release sets the version, runs git-cliff, commits, makes the tag and pushes. |
| `harness/release.sh` | The steps `check`, `macos`, `linux` and `pack`. `mise run release` runs all steps except `check`. |
| `.github/workflows/release.yaml` | The jobs `check`, `macos`, `linux` and `pack` run the steps of `harness/release.sh`. Then the jobs `attest` and `publish` run. |
