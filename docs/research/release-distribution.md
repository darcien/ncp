# Release and distribution

Date: 2026-10-09. Plan milestone 13, item 4. This report is a proposal.
Nothing in it is set up. Each new account, secret, network destination
and code source in section 7 needs a decision of the user first.

Inputs: [release-build.md](release-build.md), D1, D2, D3, S7, and the
principles in [cli.md](../design/cli.md) §1 and C1b.

## Summary

- Source of truth: the Forgejo repository. A push mirror copies branches
  and tags to a public GitHub repository. GitHub Actions builds a release
  when a tag `vX.Y.Z` arrives.
- Artifacts: four `tar.gz` archives, `SHA256SUMS`, GitHub artifact
  attestations (Sigstore, SLSA provenance), and an immutable release.
- Channels: a Homebrew tap with a formula (not a cask), and the `github:`
  backend of mise. No install script.
- Fallback with no GitHub: `mise run release` on the Mac, a minisign
  signature, and a Forgejo release.

## 1. Source and CI

| Option | Result |
|---|---|
| Push mirror, Forgejo to GitHub | Forgejo runs `git push --mirror` (all branches and tags). It can push at each commit, at an interval, or on "Synchronize Now". It force-pushes ([Forgejo mirror][fj-mirror]). Recommended. |
| Pull mirror, GitHub pulls | GitHub has no pull mirror. Codeberg also disables pull mirrors, because they used too many resources ([Codeberg FAQ][cb-faq]). |
| Two remotes, manual `git push` | Works, but each push needs two commands. Use it only to repair the mirror. |
| Forgejo Actions | Codeberg gives hosted Actions only "in limited fashion" ([Codeberg Actions][cb-actions]). macOS needs a self-hosted runner on the Mac of the user. Fulcio has no Forgejo or Codeberg issuer ([Fulcio config][fulcio-cfg]), so no keyless signature. Not recommended now. |

Mirror credential ([Forgejo mirror][fj-mirror]): use the SSH key that
Forgejo makes, as a GitHub deploy key with write access. It is valid for
one repository and does not expire. A fine-grained token is the
alternative. It needs "Workflows: write" in addition to "Contents:
write", because the mirror pushes `.github/workflows` ([token
permissions][gh-pat]). It expires, and the mirror then stops.

Tag flow: the user pushes a signed, annotated tag `vX.Y.Z` to Forgejo.
The mirror sends it to GitHub. The workflow starts on
`push: tags: ['v*']`. Its first step compares the tag with the version
in `Cargo.toml`, and stops on a difference. Turn off issues and pull
requests on the GitHub mirror. Thus no workflow runs code from a pull
request ([secure use][gh-secure]).

## 2. Build matrix

| Target | Runner | Method |
|---|---|---|
| `aarch64-apple-darwin` | `macos-15` | `cargo build` on the host, then `ncp --version` |
| `x86_64-apple-darwin` | `macos-15-intel` | The same, native |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `harness/linux/run.sh image`, `cargo fetch`, `release`, `smoke` |
| `x86_64-unknown-linux-musl` | `ubuntu-24.04` | The same |

These labels exist for public repositories ([runners][gh-runners]). Each
job runs its binary on the native architecture: no Rosetta, no QEMU.
Homebrew 7.0.0 moves Intel to Tier 3 ([Homebrew 7.0.0][hb-7]). When the
Intel runner goes away, build `x86_64-apple-darwin` on `macos-15`, as
`harness/release.sh` does now. The Linux jobs use the harness as it is:
pinned image, pinned rustup, a build with no network, and the smoke test
in an empty image. Split `harness/release.sh` into one step for each
target and a pack step, so that CI and `mise run release` run the same
commands.

Rules for the workflow:

- Pin each action to a full commit SHA, and turn on the GitHub policy
  that requires SHA pins ([secure use][gh-secure]). Allow only `checkout`,
  `upload-artifact`, `download-artifact` and `attest` of the GitHub
  `actions` organization. Use `gh` (on the runner image) for the release.
- Set `permissions: {}` at the top. Give `contents: read` to the build
  jobs. Give `contents: write`, `id-token: write` and
  `attestations: write` only to the last job ([attestations][gh-attest-use]).
- No cache. A cache can carry data from an earlier run into a release.
- Reproducibility: the Linux build has fixed paths in the container
  (`/work`, `/target`). Pack the archives in one Linux job with GNU tar
  (`--sort=name --owner=0 --group=0 --numeric-owner
  --mtime=@$SOURCE_DATE_EPOCH`) and `gzip -n`. `SOURCE_DATE_EPOCH` is the
  commit time. A check that builds twice and compares the sums is an open
  point.

## 3. Artifacts and signatures

Names follow uv ([uv release][uv-dist]): the target triple, and the
version only in the tag.

- `ncp-<target>.tar.gz` with one directory `ncp-<target>/` that holds
  `ncp`, `LICENSE-MIT` and `LICENSE-APACHE`. The MIT license requires the
  notice in each copy. mise and Homebrew remove the one top directory
  ([mise github][mise-github]).
- `SHA256SUMS` in the format of `sha256sum`, for the four archives.

| Method | Secret to keep | On Forgejo CI | Verified by | Note |
|---|---|---|---|---|
| `SHA256SUMS` only | None | Yes | mise (checksum file), Homebrew (sum in formula) | Finds a damaged file. Does not prove the publisher: the sums come from the same server. |
| minisign | A long-lived secret key | Yes | `minisign -V`, aqua | A leaked key signs anything. The user must keep and rotate it. |
| cosign keyless | None | No (no Fulcio issuer) | `cosign verify-blob` | The same Sigstore base as the next row, with a less common format. |
| GitHub artifact attestations | None | No | mise `github:` by default, `gh attestation verify` | SLSA v1.0 Build Level 2. Public repositories use the Sigstore public good instance and its transparency log ([attestations][gh-attest]). |
| Immutable release | None | No | `gh release verify-asset` | GitHub locks the tag and the assets, and makes a release attestation ([immutable][gh-immutable]). |
| packslip | None (keyless) or an Ed25519 key | Yes, with a key | mise `packslip:` | Version 1, from the authors of mise and aqua; announced 2026-09 ([packslip][packslip]). Tier 1 in the mise registry ([mise registry][mise-reg]). Too new to depend on now. |

Recommendation: `SHA256SUMS`, GitHub artifact attestations for the four
archives, and immutable releases. No long-lived secret exists: a
short-lived OIDC identity of the workflow signs. mise checks the
attestation with no action of the user ([mise github][mise-github]). The
provenance names the repository, the workflow and the commit. Keep
minisign for the fallback, because it works with no GitHub.

## 4. Distribution channels

### 4a. Homebrew tap

| Choice | Result |
|---|---|
| Formula with the release archive | Homebrew downloads with curl and sets no quarantine attribute. In the Homebrew 7.0.8 source, only cask code uses `com.apple.quarantine`. The arm64 binary runs with its ad-hoc signature. Recommended. |
| Cask | Homebrew quarantines cask downloads. "Casks without codesigning are deprecated", and `--no-quarantine` is deprecated ([Homebrew 5.0.0][hb-5]). GoReleaser then suggests an `xattr` hook, and itself says this "circumvents macOS security protections" ([GoReleaser casks][goreleaser]). Not secure by default. |
| Formula that builds from source | The form of homebrew-core (uv is there). Each user then needs Rust and a long build. Use it only to go to homebrew-core later. |

Tap location: a GitHub repository `OWNER/homebrew-tap` gives one command,
`brew install OWNER/tap/ncp`. A tap on another host needs
`brew tap OWNER/tap <URL>` first ([tap docs][hb-tap]). Since Homebrew
6.0.0, a third-party tap must be trusted before Homebrew runs its code
([Homebrew 6.0.0][hb-6]). An install with the full name trusts that one
formula (Homebrew 7.0.8, `cmd/install.rb`). Keep the tap on Forgejo as
the source, and push-mirror it to GitHub, as for `ncp`.

The formula has `version`, `license any_of: ["MIT", "Apache-2.0"]`, one
`url` and `sha256` for each of `on_macos`/`on_linux` and
`on_arm`/`on_intel`, `bin.install "ncp"`, and a `test` that runs
`ncp --version`. Homebrew on Linux uses the static musl binary. A script
writes the formula from `SHA256SUMS`. The user reviews and commits it.
Thus no workflow needs a token for a second repository.

### 4b. mise

- `mise use ncp` needs an entry in the mise registry. The bar is
  "normally thousands of GitHub stars" ([mise registry][mise-reg]). A new
  tool does not meet it. Do not plan on it.
- `mise use github:OWNER/ncp` works with no registry entry. The backend
  selects the asset by OS, CPU and libc (musl on Linux), checks the
  asset digest or the checksum file, and the GitHub attestation, and `mise lock` records the
  sums ([mise github][mise-github]). The user publishes only the
  release from section 3. Recommended.
- `forgejo:OWNER/ncp` (Codeberg by default, `api_url` for another host)
  checks only a checksum file. Attestations are "Not available"
  ([mise forgejo][mise-forgejo]). Fallback only.
- `aqua:` needs a pull request to the aqua registry and adds nothing over
  `github:` for this project. `ubi` is deprecated. `http:` needs a URL
  for each platform and has no automatic verification.

### 4c. Install script

`curl … | sh` is one command, and uv offers it. But the shell runs code
from the network before any check, and only TLS protects it. A check in
the script comes from the same server. Homebrew, mise and a manual
download with a verify step cover macOS and Linux. Thus the MVP has no
install script. The release notes give the manual steps of section 5.

## 5. macOS unsigned binary

- The linker ad-hoc signs the arm64 binary (`codesign -dv`:
  `flags=0x20002(adhoc,linker-signed)`). Apple silicon needs this
  signature to run native code. Do not change the file after the link.
  Add `codesign --verify` to the arm64 job. The x86_64 binary has no
  signature; an Intel Mac and Rosetta run it (release-build.md).
- The ad-hoc identity changes at each build. S7 and C1b cover this.
- Quarantine: a browser sets `com.apple.quarantine`. Homebrew formulae,
  mise and curl do not. With the attribute, Gatekeeper stops the first
  run and shows that macOS cannot verify the binary. The user must then
  use System Settings, Privacy & Security, "Open Anyway"
  ([Apple][apple-open]).

Minimum safe instructions for a manual download:

1. Download with curl, or verify before the first run.
2. Verify: `gh attestation verify ncp-aarch64-apple-darwin.tar.gz -R
   OWNER/ncp`, or compare the sum with `SHA256SUMS`.
3. Only after a browser download: `xattr -d com.apple.quarantine ncp` on
   that one verified file. Do not use `-r` on a folder. Do not turn off
   Gatekeeper.

## 6. Versions, changelog and release steps

- Versions: SemVer 2.0.0 ([semver][semver]). The version in `Cargo.toml`
  is the source. Tag: `vX.Y.Z`. Before 1.0, a breaking change increments
  the minor number, as Cargo does ([Cargo semver][cargo-semver]).
- Changelog: `CHANGELOG.md` in the form of Keep a Changelog 1.1.0
  ([changelog][keepachangelog]), written by hand from the Conventional
  Commits. No new tool. The release notes are the section of the
  version.

Steps for one release:

1. Update `CHANGELOG.md` and the version. Run `mise run precommit`.
   Commit `chore(release): X.Y.Z` and push to Forgejo.
2. Push a signed, annotated tag `vX.Y.Z` to Forgejo. The mirror sends it.
3. The workflow checks the version, builds and smoke-tests the four
   targets, packs the archives, writes `SHA256SUMS`, and attests.
4. The last job makes a draft release with all assets, then publishes
   it. GitHub then locks it ([immutable][gh-immutable]).
5. The user runs `gh release verify-asset`, `gh attestation verify`, and
   `mise use github:OWNER/ncp@X.Y.Z` on a clean machine.
6. The user runs the formula script, reviews, and commits to the tap.
   Then `brew install OWNER/tap/ncp` and `brew test ncp`.

## 7. New accounts, secrets and destinations

| Item | Kind | Needed by |
|---|---|---|
| Forgejo host (Codeberg or own) | Account, repositories `ncp`, `homebrew-tap` | Source of truth |
| github.com | Account, public repositories `ncp`, `homebrew-tap` | Mirror, CI, releases, tap |
| Deploy key (or token) in Forgejo for each mirror | Secret | Push mirror |
| GitHub runner images | Code source | CI |
| `actions/checkout`, `upload-artifact`, `download-artifact`, `attest` | Code source | CI |
| Docker Hub (`debian:trixie-slim` by digest), deb.debian.org | Destination (already used by the local harness) | Linux jobs |
| static.rust-lang.org, crates.io | Destination (already allowed) | All builds |
| fulcio.sigstore.dev, rekor.sigstore.dev | Destination | Attestations |
| github.com release assets, Sigstore TUF root | Destination for users | mise and `gh` verify, Homebrew download |
| minisign key pair | Secret on the Mac, and a tool | Fallback only |

No new crate dependency. `deny.toml` does not change.

## 8. Recommendation

Primary: Forgejo, push mirror to GitHub, GitHub Actions, GitHub release
with attestations, Homebrew tap formula, mise `github:` backend.

Fallback, if the user does not want GitHub: `mise run release` on the
Mac, `minisign -S` of `SHA256SUMS`, a Forgejo release, `forgejo:` in
mise, and the tap on Forgejo with `brew tap OWNER/tap <URL>`. The cost:
no provenance, the secret key on the Mac, and two brew commands.

What the user does:

1. Approve the items in section 7.
2. Make the GitHub account (or use one) and two empty public
   repositories: `ncp` and `homebrew-tap`. Turn off issues and pull
   requests. Turn on immutable releases, SHA pin enforcement, "Allow
   specified actions", and a read-only default `GITHUB_TOKEN`.
3. In Forgejo, add a push mirror for each repository with an SSH key.
   Add each key on GitHub as a deploy key with write access.
4. Make the `homebrew-tap` repository on Forgejo.
5. Decide the owner name. It is in all install commands.

What an agent can then do:

1. Split `harness/release.sh` into steps for each target and a pack
   step. Keep `mise run release` for all.
2. Write `.github/workflows/release.yml` (four build jobs, one publish
   job), with pinned SHAs and the permissions of section 2.
3. Write `harness/formula.sh` and `Formula/ncp.rb` from `SHA256SUMS`.
4. Add `CHANGELOG.md`, the install and verify steps for the docs, and D2
   and D3 detail in the requirements.
5. After the first release, add a new report with the result.

## Open points

- Third-party licenses: the MIT and BSD licenses of the dependencies ask
  for their notices in a binary copy. A notice file needs a tool such as
  `cargo-about`, which is a new code source.
- Does a tag push start the sync at each commit of the push mirror?
- Reproducible build: build twice and compare the sums.

## Sources

All checked on 2026-10-09. The links in the text use these sources.

- Forgejo and Codeberg: [repository mirrors, v16.0][fj-mirror],
  [Codeberg FAQ][cb-faq], [Codeberg Actions][cb-actions].
- GitHub: [secure use][gh-secure], [hosted runners][gh-runners],
  [artifact attestations][gh-attest], [use attestations][gh-attest-use],
  [immutable releases][gh-immutable], [token permissions][gh-pat].
- Sigstore: [Fulcio identity config][fulcio-cfg].
- Homebrew: [5.0.0][hb-5], [6.0.0][hb-6], [7.0.0][hb-7], [taps][hb-tap];
  [GoReleaser casks][goreleaser].
- mise: [github backend][mise-github], [forgejo backend][mise-forgejo],
  [registry rules][mise-reg]; [packslip][packslip]; [uv dist
  config][uv-dist].
- Other: [Apple, open an app][apple-open], [SemVer 2.0.0][semver],
  [Cargo SemVer][cargo-semver], [Keep a Changelog 1.1.0][keepachangelog].
- Local: Homebrew 7.0.8 source (`Library/Homebrew/cmd/install.rb`,
  `cask/quarantine.rb`), and `codesign -dv` of the binaries in `dist/`.

[fj-mirror]: https://forgejo.org/docs/v16.0/user/repo-mirror/
[cb-faq]: https://docs.codeberg.org/getting-started/faq/
[cb-actions]: https://docs.codeberg.org/ci/actions/
[fulcio-cfg]: https://github.com/sigstore/fulcio/blob/main/config/identity/config.yaml
[gh-pat]: https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens
[gh-secure]: https://docs.github.com/en/actions/reference/security/secure-use
[gh-runners]: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
[gh-attest]: https://docs.github.com/en/actions/concepts/security/artifact-attestations
[gh-attest-use]: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations
[gh-immutable]: https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases
[uv-dist]: https://github.com/astral-sh/uv/blob/main/dist-workspace.toml
[hb-5]: https://brew.sh/2025/11/12/homebrew-5.0.0/
[hb-6]: https://brew.sh/2026/06/11/homebrew-6.0.0/
[hb-7]: https://brew.sh/2026/09/13/homebrew-7.0.0/
[hb-tap]: https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap
[goreleaser]: https://goreleaser.com/customization/homebrew_casks/
[mise-github]: https://mise.jdx.dev/dev-tools/backends/github.html
[mise-forgejo]: https://mise.jdx.dev/dev-tools/backends/forgejo.html
[mise-reg]: https://mise.jdx.dev/contributing/registry.html
[packslip]: https://packslip.dev
[apple-open]: https://support.apple.com/en-us/102445
[semver]: https://semver.org/spec/v2.0.0.html
[cargo-semver]: https://doc.rust-lang.org/cargo/reference/semver.html
[keepachangelog]: https://keepachangelog.com/en/1.1.0/
