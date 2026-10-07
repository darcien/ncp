#!/bin/sh
# Builds the Linux harness image, and runs cargo for the musl target in it.
#
# Usage:
#   harness/linux/run.sh image
#   harness/linux/run.sh cargo <command> [arguments]
#
# NCP_LINUX_PLATFORM selects the platform: linux/arm64 (default) or
# linux/amd64. On an Apple silicon host, linux/amd64 runs through Rosetta.
set -eu

platform="${NCP_LINUX_PLATFORM:-linux/arm64}"
case "$platform" in
linux/arm64) arch=aarch64 ;;
linux/amd64) arch=x86_64 ;;
*)
	echo "run.sh: unsupported NCP_LINUX_PLATFORM '$platform'." >&2
	echo "run.sh: use linux/arm64 or linux/amd64." >&2
	exit 2
	;;
esac

name="${platform#linux/}"
target="$arch-unknown-linux-musl"
image="ncp-harness-linux:$name"
root="$(cd "$(dirname "$0")/../.." && pwd)"

build_image() {
	rust_version="$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$root/rust-toolchain.toml")"
	docker build \
		--platform "$platform" \
		--build-arg "RUST_VERSION=$rust_version" \
		--target toolchain \
		--tag "$image" \
		"$root/harness/linux"
}

# The repository is read-only in the container, and the container runs as
# the host user. Thus a run cannot change or add files in the repository.
# The registry and the build output are in named volumes, so the Linux
# output does not mix with the macOS target directory. The build output has
# one volume for each platform, because the build scripts are different.
# --init reaps the child processes that the tests start.
run_cargo() {
	command="$1"
	shift
	docker run --rm --init \
		--platform "$platform" \
		--user "$(id -u):$(id -g)" \
		--env HOME=/tmp \
		--volume "$root:/work:ro" \
		--volume ncp-harness-cargo-registry:/usr/local/cargo/registry \
		--volume "ncp-harness-target-$name:/target" \
		"$image" \
		cargo "$command" --target "$target" "$@"
}

case "${1:-}" in
image) build_image ;;
cargo)
	shift
	[ "$#" -gt 0 ] || {
		echo "run.sh: give a cargo command." >&2
		exit 2
	}
	run_cargo "$@"
	;;
*)
	echo "usage: run.sh image | run.sh cargo <command> [arguments]" >&2
	exit 2
	;;
esac
