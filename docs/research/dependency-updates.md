# Research: detection of outdated dependencies

Date: 2026-11-18. Sources: the files of uv, mise and their Renovate
presets at the commits below, read through `gh api`.
`cargo update --dry-run --verbose` (cargo 1.99.0) in `ncp`.

| Repository | Commit |
|---|---|
| astral-sh/uv | [`7f39653`][uv] |
| astral-sh/renovate-config | [`cb60d16`][astral-preset] |
| jdx/mise | [`30a2915`][mise] |
| jdx/renovate-config | [`f29cd6d`][jdx-preset] |

uv and mise do not pin their shared preset to a version.

## 1. Tools

| Question | Tool | Part of Cargo |
|---|---|---|
| Can `Cargo.lock` go forward inside the current ranges? | `cargo update --dry-run --verbose` | Yes |
| Does a version outside the range exist? | The same command (`(available: Y)`), `cargo outdated`, `cargo upgrade --dry-run` | Partly |
| Is a dependency yanked, unsafe or unmaintained? | `cargo deny check advisories`, `cargo audit` | No |
| Does the build use two versions of one crate? | `cargo deny check bans`, `cargo tree -d` | Partly |
| Did the MSRV keep an old version? | The MSRV-aware resolver | Yes |
| Is a dependency not used? | `cargo machete`, `cargo udeps` | No |
| Does a check run with no commit? | Renovate, Dependabot, a CI `schedule` | No |

## 2. uv

| Item | Value | Source |
|---|---|---|
| Update bot | Renovate, Monday 00:00 to 03:59 | uv [`renovate.json5`][uv-renovate] |
| Cargo strategy | `update-lockfile`: an update inside the range changes only `Cargo.lock`. An update outside the range uses `replace` and changes `Cargo.toml`. | Preset [`preset.json5`][astral-preset-file], Renovate [`rangeStrategy`][renovate-range] |
| Cooldown in Renovate | 30 days. None for crates of Astral. | Preset |
| Cooldown in CI | `check-cargo-cooldowns` fails on a locked crate younger than 7 days | uv [`ci.yml`][uv-ci], [`cargo-cooldowns.toml`][uv-cooldowns] |
| Lockfile maintenance | Off | Preset |
| Known vulnerabilities | Renovate `vulnerabilityAlerts`, no cooldown, fix strategy `lowest` | Preset |
| Supply-chain audit | `check-cargo-vet` in CI | uv [`ci.yml`][uv-ci] |
| Toolchain and MSRV | Renovate updates `rust-toolchain.toml` and `rust-version`. The MSRV waits 85 days. | uv |
| Holds | `zip` (ownership change), `pyo3` | uv |
| cargo-deny | No `deny.toml` at the root | uv |

## 3. mise

| Item | Value | Source |
|---|---|---|
| Update bot | Renovate, Tuesday | mise [`renovate.json`][mise-renovate], preset [`default.json`][jdx-preset-file] |
| Automerge | Off in mise. The preset turns it on. | mise |
| Cooldown | 7 days. None for crates of jdx. | Preset |
| Lockfile maintenance | On | Preset |
| Groups | One PR for minor and patch updates of crates. Groups for rattler, aube, usage, sigstore, phf. | Preset, mise |
| Holds | `decmpfs <=0.1.0` (0.1.2 does not compile for musl) | mise |
| Advisories on a PR | Only when `Cargo.lock`, `Cargo.toml` or `deny.toml` changes | mise [`test-impl.yml`][mise-test] |
| Advisories each day | Cron on `main`. A failure opens or updates one issue and sends an email. | mise [`deny-advisories.yml`][mise-deny] |
| Unused dependencies | `cargo machete --with-metadata` in CI | mise [`test-impl.yml`][mise-test] |
| Ignored advisories | Each one has a `reason` | mise [`deny.toml`][mise-deny-toml] |

## 4. Findings

- Both projects use Renovate. Neither has a scheduled job that only
  reports outdated versions.
- Both delay new releases: 30 days in uv, 7 days in mise. Their own
  crates and vulnerability fixes have no delay.
- Known vulnerabilities have a separate channel: Renovate alerts in uv,
  a daily cargo-deny run in mise.
- Each hold and each ignored advisory has a written reason.

## 5. `ncp`

- cargo-deny 0.20.2 is the only automated check. It runs in the hook and
  in CI. An advisory gives only a warning.
- No Renovate or Dependabot configuration. No workflow has a `schedule`.
  A new RustSec entry shows only when a commit or a person runs the
  check.
- No release cooldown. No check for unused dependencies.

The dry run gave 74 updates inside the current ranges. It did not select
these newer versions. A version requirement holds each one. For the
semver-compatible versions, the declared MSRV is not the cause:
rmcp 3.11.0 declares 1.88, and keyring-core 1.0.1 declares 1.85.
generic-array 0.14.9 declares no MSRV, so Cargo has no MSRV to compare.

| Crate | Version after the update | Available | Requirement that holds it |
|---|---|---|---|
| rmcp | 3.10.0 | 3.11.0 (2026-11-17) | `=3.10.0` in `crates/ncp` and `crates/testkit` |
| keyring-core | 1.0.0 | 1.0.1 (2026-10-13) | `=1.0.0` in `crates/ncp`. zbus-secret-service-keyring-store 1.0.1 accepts `^1`. |
| jsonc-parser | 0.33.2 | 0.34.0 | `=0.33.2` in `crates/ncp` |
| jsonschema | 0.58.6 | 0.61.2 | `0.58.6` in `crates/ncp`. 0.61 is not semver-compatible. |
| sha2 | 0.10.9 | 0.11.1 | `0.10` in `crates/ncp`, the version of oauth2 5.0.0 (`^0.10`). rmcp and secret-service already use sha2 0.11.0. |
| sse-stream | 0.2.6 | 0.3.0 | `0.2` in `crates/ncp`. ncp uses the event type of rmcp, and rmcp 3.10.0 requires `^0.2.4`. |
| nix | 0.31.4 (from 0.31.3) | 0.32.0 | `0.31` in `crates/ncp`. process-wrap 10.0.1 requires `^0.31.1`. |
| generic-array | 0.14.7 | 0.14.9 | crypto-common 0.1.7 requires `=0.14.7`. Chain: ncp → sha2 0.10.9 → digest 0.10.7 → crypto-common 0.1.7. |

`cargo update -p CRATE@LOCKED --precise AVAILABLE --dry-run` fails for
rmcp, keyring-core and generic-array, and names the requirements above.

[rmcp-open-points.md](rmcp-open-points.md) section 6 recommends an exact
pin for rmcp: rmcp had 12 breaking releases in 12 months, and fixes come
only in the newest major.

generic-array 0.14.8 and later deprecate the 0.14 API. crypto-common
0.1.7 pins 0.14.7 to stop the deprecation warnings
([RustCrypto/traits#2088][rc-2088], [#2036][rc-2036]). crypto-common 0.1
comes through sha2 0.10, which ncp and oauth2 5.0.0 require.

[rc-2088]: https://github.com/RustCrypto/traits/pull/2088
[rc-2036]: https://github.com/RustCrypto/traits/issues/2036

[uv]: https://github.com/astral-sh/uv/tree/7f396534d58321e60b5ba204151326712d912ece
[uv-renovate]: https://github.com/astral-sh/uv/blob/7f396534d58321e60b5ba204151326712d912ece/.github/renovate.json5
[uv-ci]: https://github.com/astral-sh/uv/blob/7f396534d58321e60b5ba204151326712d912ece/.github/workflows/ci.yml
[uv-cooldowns]: https://github.com/astral-sh/uv/blob/7f396534d58321e60b5ba204151326712d912ece/.github/cargo-cooldowns.toml
[astral-preset]: https://github.com/astral-sh/renovate-config/tree/cb60d1609c2b153539884743e2266006f64275f7
[astral-preset-file]: https://github.com/astral-sh/renovate-config/blob/cb60d1609c2b153539884743e2266006f64275f7/preset.json5
[mise]: https://github.com/jdx/mise/tree/30a2915723b3266fcaf78d7586b2d61c23f13f8f
[mise-renovate]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/.github/renovate.json
[mise-test]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/.github/workflows/test-impl.yml
[mise-deny]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/.github/workflows/deny-advisories.yml
[mise-deny-toml]: https://github.com/jdx/mise/blob/30a2915723b3266fcaf78d7586b2d61c23f13f8f/deny.toml
[jdx-preset]: https://github.com/jdx/renovate-config/tree/f29cd6d5584e3acad48f18cc6eba09604a5bc6bf
[jdx-preset-file]: https://github.com/jdx/renovate-config/blob/f29cd6d5584e3acad48f18cc6eba09604a5bc6bf/default.json
[renovate-range]: https://github.com/renovatebot/renovate/blob/8549e28afa27ed94665b24fb9cbf1231a7be8175/docs/usage/configuration-options.md#rangestrategy
