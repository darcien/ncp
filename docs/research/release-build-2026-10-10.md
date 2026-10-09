# Release build and start time after the decisions build

Date: 2026-10-10. Plan milestone 14, items 1 to 3, at commit 3066c47
(after milestone 13). The method is the same as in
[release-build.md](release-build.md). The distribution channel (D2), CI and
each upload are not part of this report: they need a decision of the user.

## Method

1. `mise run release` (`harness/release.sh`): release builds for
   `aarch64-apple-darwin` and `x86_64-apple-darwin` on the host, and static
   musl builds for `aarch64` and `x86_64` in the Linux harness with no
   network. `dist/SHA256SUMS` has the SHA-256 sum of each binary.
2. Smoke tests: each Linux binary runs in an image that has only the binary
   (no shell, no libc, no files, no network), on `linux/arm64` and on
   `linux/amd64`. Commands: `--version`, `server list`, `secret list`. The
   macOS binaries run `--version`; the `x86_64` one through Rosetta.
3. Start time (F3), with no network. The configuration has three servers.
   macOS: the median of 200 runs, with a Python timer around each process.
   Linux: the mean of 200 runs in the harness image (`linux/arm64`, the
   OrbStack VM on the same Mac).

## Result

All builds and smoke tests passed.

| Binary | Size | Linked to |
|---|---|---|
| `ncp-aarch64-apple-darwin` | 9.9 MB | Security, SystemConfiguration, CoreFoundation, libiconv, libSystem (all part of macOS) |
| `ncp-x86_64-apple-darwin` | 11.1 MB | The same |
| `ncp-aarch64-unknown-linux-musl` | 11.7 MB | Nothing (static) |
| `ncp-x86_64-unknown-linux-musl` | 14.1 MB | Nothing (static PIE) |

Each binary is one file with no dependency outside the operating system
(D1). The binaries are about 0.6 MB larger than at the trial of 2026-10-09,
because of the work of milestone 13.

## Start time (F3)

| Host | Command | Time | Reference |
|---|---|---|---|
| macOS 15.7.4, Apple M4, arm64 binary | `ncp --version` | 3.2 ms (median) | `/usr/bin/true`: 1.3 ms |
| | `ncp server list` | 3.4 ms (median) | |
| macOS, x86_64 binary through Rosetta | `ncp --version` | 12.3 ms (median) | `/usr/bin/true`: 1.2 ms |
| | `ncp server list` | 14.6 ms (median) | |
| Linux arm64 container, static binary | `ncp --version` | 0.24 ms (mean) | `/bin/true`: 0.19 ms |
| | `ncp server list` | 0.35 ms (mean) | |

Result: F3 is met. The start is a few milliseconds on macOS and less than
1 ms on Linux. Most of the macOS time is the start of a process on macOS.
The times are near those of the trial; the Python timer of this report has
less overhead than the timer of the trial, so the macOS numbers are lower.

## Open points for the user

- Item 4 of milestone 14: the distribution channel (D2), CI, signatures,
  and each push or upload. Each adds a network destination or an account.
  The proposal is in [release-distribution.md](release-distribution.md).
- The macOS binaries are not signed or notarized (see the trial report).
