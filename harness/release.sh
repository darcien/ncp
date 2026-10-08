#!/bin/sh
# Builds the release binaries of ncp on a Mac, and writes their checksums.
#
# Usage:
#   harness/release.sh
#
# Output in dist/:
#   ncp-aarch64-apple-darwin, ncp-x86_64-apple-darwin
#   ncp-aarch64-unknown-linux-musl, ncp-x86_64-unknown-linux-musl
#   SHA256SUMS
#
# The Linux binaries are static musl builds in the Linux harness, with no
# network. Each one then runs in an empty image (no libc, no files). The
# x86_64 builds run through Rosetta. Online steps: rustup adds the macOS
# x86_64 target, and cargo fetches the crates. This script uploads nothing.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mkdir -p dist

rustup target add aarch64-apple-darwin x86_64-apple-darwin
for target in aarch64-apple-darwin x86_64-apple-darwin; do
	cargo build --release --locked -p ncp --target "$target"
	cp "target/$target/release/ncp" "dist/ncp-$target"
done
dist/ncp-aarch64-apple-darwin --version
arch -x86_64 dist/ncp-x86_64-apple-darwin --version

for platform in linux/arm64 linux/amd64; do
	NCP_LINUX_PLATFORM="$platform" harness/linux/run.sh image
	NCP_LINUX_PLATFORM="$platform" harness/linux/run.sh cargo fetch --locked
	NCP_LINUX_PLATFORM="$platform" harness/linux/run.sh release
	NCP_LINUX_PLATFORM="$platform" harness/linux/run.sh smoke
done

cd dist
shasum -a 256 ncp-aarch64-apple-darwin ncp-x86_64-apple-darwin \
	ncp-aarch64-unknown-linux-musl ncp-x86_64-unknown-linux-musl >SHA256SUMS
cat SHA256SUMS
