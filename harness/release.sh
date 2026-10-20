#!/bin/sh
# Builds the release archives of ncp, and checks a release tag.
#
# Usage:
#   harness/release.sh             macos, linux for both platforms, pack
#   harness/release.sh check <tag>
#   harness/release.sh macos
#   harness/release.sh linux
#   harness/release.sh pack
#
# "check" stops when the tag is not v followed by the version in
# Cargo.toml, or when CHANGELOG.md has no section for that version. It
# writes the section to dist/notes.md. "macos" builds the two macOS
# binaries on a Mac, and runs them. The x86_64 binary runs through
# Rosetta. "linux" builds the static musl binary for NCP_LINUX_PLATFORM in
# the Linux harness with no network, then runs it in an empty image (no
# libc, no files). "pack" puts the four binaries in dist/ into
# ncp-<version>-<target>.tar.gz with the license files, and writes
# dist/SHA256SUMS for the archives.
#
# The online steps: rustup installs the toolchain and the macOS targets,
# docker builds the harness image, and cargo fetches the crates. This
# script uploads nothing. .github/workflows/release.yaml runs each step in
# its own job.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mkdir -p dist

targets="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl"

check() {
	tag="$1"
	version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
	[ "$tag" = "v$version" ] || {
		echo "release.sh: tag $tag is not v$version, the version in Cargo.toml." >&2
		exit 1
	}
	awk -v head="## $version " '
		index($0, head) == 1 { found = 1; next }
		found && /^## / { exit }
		found { print }
	' CHANGELOG.md >dist/notes.md
	grep -q '[^[:space:]]' dist/notes.md || {
		echo "release.sh: CHANGELOG.md has no section for $version." >&2
		exit 1
	}
}

# rustup installs the toolchain of rust-toolchain.toml before it adds the
# targets, because a new runner does not have it.
macos() {
	rustup toolchain install
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	for target in aarch64-apple-darwin x86_64-apple-darwin; do
		cargo build --release --locked -p ncp --target "$target"
		cp "target/$target/release/ncp" "dist/ncp-$target"
	done
	dist/ncp-aarch64-apple-darwin --version
	arch -x86_64 dist/ncp-x86_64-apple-darwin --version
}

linux() {
	harness/linux/run.sh image
	harness/linux/run.sh cargo fetch --locked
	harness/linux/run.sh release
	harness/linux/run.sh smoke
}

# The archives come from GNU tar in the harness, so that their bytes do
# not depend on the tar of the host. A missing binary stops the script.
pack() {
	for target in $targets; do
		[ -f "dist/ncp-$target" ] || {
			echo "release.sh: dist/ncp-$target is missing." >&2
			exit 1
		}
	done
	harness/linux/run.sh image
	harness/linux/run.sh pack
}

case "${1:-}" in
"")
	macos
	for platform in linux/arm64 linux/amd64; do
		(export NCP_LINUX_PLATFORM="$platform" && linux)
	done
	pack
	;;
check)
	[ "$#" -eq 2 ] || {
		echo "release.sh: use release.sh check <tag>." >&2
		exit 2
	}
	check "$2"
	;;
macos) macos ;;
linux) linux ;;
pack) pack ;;
*)
	echo "usage: release.sh [check <tag> | macos | linux | pack]" >&2
	exit 2
	;;
esac
