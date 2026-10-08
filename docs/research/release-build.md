# Release build and start time

Date: 2026-10-09. Plan milestone 13, items 1 to 3. The distribution
channel (D2), CI, and each upload are not part of this report: they need
a decision of the user.

## Method

`mise run release` (script `harness/release.sh`):

1. macOS: `cargo build --release -p ncp` for `aarch64-apple-darwin` and
   `x86_64-apple-darwin` on the host. The x86_64 binary runs through
   Rosetta.
2. Linux: a static musl build for each platform in the Linux harness
   (`run.sh release`), with no network. Then `run.sh smoke` runs the
   binary in an image that has only the binary: no shell, no libc, no
   files, no network. The image comes from `docker import` of a tar, so
   no base image is downloaded. Commands: `--version`, `server list`,
   `secret list`.
3. `dist/SHA256SUMS` with the SHA-256 sum of each binary (D3).

Only `-p ncp` builds: in the workspace, cargo unifies the features of
rmcp with the server features of the test kit. The release profile has
link-time optimization, one code unit and no symbols.

## Result

| Binary | Size | Linked to | Smoke test |
|---|---|---|---|
| `ncp-aarch64-apple-darwin` | 9.3 MB | Security, SystemConfiguration, CoreFoundation, libiconv, libSystem (all part of macOS) | `--version` on the host |
| `ncp-x86_64-apple-darwin` | 10 MB | The same | `--version` through Rosetta |
| `ncp-aarch64-unknown-linux-musl` | 11 MB | Nothing (static) | Passed in the empty image |
| `ncp-x86_64-unknown-linux-musl` | 14 MB | Nothing (static PIE) | Passed in the empty image, through Rosetta |

Thus each binary is one file with no dependency outside the operating
system (D1).

## Start time (F3)

The time from the start of the process to its exit. No network access.
The configuration has three servers. 200 runs for each command.

| Host | Command | Median | Note |
|---|---|---|---|
| macOS 15.7.4, Apple M4, arm64 binary | `ncp --version` | 4.9 ms | `/usr/bin/true` takes 1.3 ms |
| | `ncp server list` | 5.2 ms | |
| macOS, x86_64 binary through Rosetta | `ncp --version` | 17.3 ms | Rosetta translates at each start |
| | `ncp server list` | 17.3 ms | |
| Linux arm64 container (OrbStack), static binary | `ncp --version` | 0.28 ms (mean) | `/bin/true` takes 0.18 ms |
| | `ncp server list` | 0.35 ms (mean) | |

Result: the start is a few milliseconds on macOS and less than 1 ms on
Linux. F3 is met. The macOS time is mostly the start of a process on
macOS (the dynamic loader and the system frameworks), not the work of
`ncp`.

## Open points for the user

- The distribution channel (D2), CI, and signatures. Each adds a network
  destination or an account.
- The macOS binaries are not signed or notarized. A binary that a browser
  downloads gets the quarantine attribute, and Gatekeeper then blocks it.
  A package manager or `curl` does not set the attribute.
