# Release

`mise run release:tag --execute` pushes a release tag `vX.Y.Z`. The tag
starts `.github/workflows/release.yaml`, which builds, attests and
publishes the release on GitHub. Only you do these steps, on this Mac.

## Before each release

1. Run `mise run live` (`docs/live-tests.md`).
2. Update `main` and the tags: `git pull --ff-only --tags`.

## Publish a release

1. Do a dry run: `mise run release:tag`.
2. Read the version and the new section in the output. Make sure that
   each breaking change gives the steps for the user.
3. If the version does not obey the version rule (see "Reference"), do the
   dry run again with the correct version. Example:
   `mise run release:tag 0.3.0`.
4. Run `mise run release:tag --execute`. If you gave a version in step 3,
   give it again.
5. When cargo-release asks, confirm.

## Check a release

1. Find the run: `gh run list --workflow release.yaml --limit 1`.
2. Wait for the end of the run: `gh run watch <run-id> --exit-status`.
3. Check the files of the release:

   ```sh
   gh release download vX.Y.Z --repo darcien/ncp --dir ncp-X.Y.Z
   cd ncp-X.Y.Z
   shasum -a 256 -c SHA256SUMS
   for f in ncp-X.Y.Z-*.tar.gz; do gh attestation verify "$f" --repo darcien/ncp; done
   ```

4. Run the binary from the release. The output must show `X.Y.Z`:

   ```sh
   mise x github:darcien/ncp@X.Y.Z -- ncp --version
   ```

## When a job fails

- If the cause is temporary, for example a runner or network fault, run
  the failed jobs again: `gh run rerun <run-id> --failed`.
- If a job before `publish` fails for a different cause:
  1. Delete the tag on GitHub: `git push origin :refs/tags/vX.Y.Z`.
  2. Delete the tag on this Mac: `git tag -d vX.Y.Z`.
  3. Correct the fault on `main`. If the correction is a `feat` or `fix`
     commit, add it to the X.Y.Z section of `CHANGELOG.md`.
  4. Build the archives: `mise run release`.
  5. Check the version and the section, make the tag again, and push
     `main` and the tag:

     ```sh
     mise run release:check vX.Y.Z
     git tag -a vX.Y.Z -m "ncp X.Y.Z"
     git push --atomic origin main vX.Y.Z
     ```

  Do not run `release:tag` again. It adds a second section for the same
  version.
- If the job `publish` fails, delete its draft before a new run: `gh
  release delete vX.Y.Z --repo darcien/ncp --yes`. This command keeps the
  tag.
- If the release is published, the tag, the archives and `SHA256SUMS`
  cannot change. Correct the fault in a new patch version.

## Reference

### Version rule

A release keeps the commands, the options, the configuration files, the
stored credentials and the JSON output of an earlier release valid. When a
change makes old input or stored data invalid, `ncp` accepts the old form
too, or migrates the data. When neither is possible, the change is a
breaking change, and its release notes give the steps for the user.

Versions follow SemVer 2.0.0. At `0.y`, as in uv, a breaking change gives
the next minor version, and a new feature or a fix gives the next patch
version.

### Parts

| Part | Function |
|---|---|
| `CHANGELOG.md` | One section for each version. The section of a version is its release notes. |
| `cliff.toml` | git-cliff writes a section from the `feat`, `fix` and breaking commits, and finds the next version by the version rule. The `BREAKING CHANGE` footer of a breaking commit gives the steps for the user. |
| `release.toml` | cargo-release sets the version, commits, tags and pushes. Its hook runs git-cliff, so the release commit includes the new section. |
| `mise run changelog` | Shows the next version and its section. |
| `mise run release:tag` | Runs cargo-release. Without `--execute`, it does a dry run. |
| `mise run release:check <tag>` | Stops when the tag does not agree with the version in `Cargo.toml`, or when `CHANGELOG.md` has no section for it. |
| `mise run release` | Builds the archives for all four targets on this Mac, and uploads nothing. |
| `harness/release.sh` | The steps `check`, `macos`, `linux` and `pack`. The workflow jobs of the same names run these steps. |
| `.github/workflows/release.yaml` | Jobs `check`, `macos` (`macos-15`), `linux` (one runner for each architecture), `pack`, `attest` and `publish`. |

A release has `ncp-X.Y.Z-<target>.tar.gz` for `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `aarch64-unknown-linux-musl` and
`x86_64-unknown-linux-musl`, and `SHA256SUMS`.
