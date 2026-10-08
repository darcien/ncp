#!/bin/sh
# Builds the Linux harness image, and runs cargo for the musl target in it.
#
# Usage:
#   harness/linux/run.sh image
#   harness/linux/run.sh cargo <command> [arguments]
#   harness/linux/run.sh offline cargo <command> [arguments]
#   harness/linux/run.sh secret-service
#   harness/linux/run.sh prune
#
# "offline" runs the container with no network. Run "cargo fetch" first:
# the registry volume then has all crates. Tests use only local servers,
# so a test that tries to reach another host fails and is visible.
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
	stage="${1:-toolchain}"
	tag="$image"
	[ "$stage" = toolchain ] || tag="$image-$stage"
	rust_version="$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$root/rust-toolchain.toml")"
	docker build \
		--platform "$platform" \
		--build-arg "RUST_VERSION=$rust_version" \
		--target "$stage" \
		--tag "$tag" \
		"$root/harness/linux"
}

# The repository is read-only in the container, and the container runs as
# the host user. Thus a run cannot change or add files in the repository.
# The registry and the build output are in named volumes, so the Linux
# output does not mix with the macOS target directory. The build output has
# one volume for each platform, because the build scripts are different.
# --init reaps the child processes that the tests start.
run_cargo() {
	network="$1"
	command="$2"
	shift 2
	offline=false
	[ "$network" = none ] && offline=true
	docker run --rm --init \
		--network "$network" \
		--env "CARGO_NET_OFFLINE=$offline" \
		--platform "$platform" \
		--user "$(id -u):$(id -g)" \
		--env HOME=/tmp \
		--volume "$root:/work:ro" \
		--volume ncp-harness-cargo-registry:/usr/local/cargo/registry \
		--volume "ncp-harness-target-$name:/target" \
		"$image" \
		cargo "$command" --target "$target" "$@"
}

# The Secret Service tests use the build volume of the other runs, with no
# network.
run_secret_service() {
	docker run --rm --init \
		--network none \
		--env CARGO_NET_OFFLINE=true \
		--platform "$platform" \
		--user "$(id -u):$(id -g)" \
		--env HOME=/tmp \
		--volume "$root:/work:ro" \
		--volume ncp-harness-cargo-registry:/usr/local/cargo/registry \
		--volume "ncp-harness-target-$name:/target" \
		"$image-secret-service" \
		sh harness/linux/secret-service.sh "$target"
}

# Cargo never removes old build output, so the target volumes grow with no
# limit. A volume is only a cache, thus a removal costs one full build.
# Docker gives sizes in decimal units, for example 10.39GB. All platforms are
# in the check, because an old volume also uses disk space.
prune() {
	limit_gb=16
	docker system df -v --format '{{range .Volumes}}{{.Name}} {{.Size}}{{println}}{{end}}' |
		awk -v limit="$limit_gb" '
			BEGIN {
				scale["B"] = 1e-9; scale["kB"] = 1e-6; scale["MB"] = 1e-3
				scale["GB"] = 1; scale["TB"] = 1e3
			}
			$1 ~ /^ncp-harness-target-/ {
				unit = $2
				sub(/^[0-9.]+/, "", unit)
				gb = ($2 + 0) * scale[unit]
				printf "%s %s\n", $1, $2 > "/dev/stderr"
				if (gb > limit) print $1
			}' |
		while read -r volume; do
			echo "run.sh: $volume is larger than ${limit_gb}GB. Removing it." >&2
			docker volume rm "$volume" >/dev/null ||
				echo "run.sh: cannot remove $volume. A container uses it." >&2
		done
}

case "${1:-}" in
image) build_image ;;
prune) prune ;;
secret-service)
	build_image secret-service
	run_secret_service
	;;
cargo)
	shift
	[ "$#" -gt 0 ] || {
		echo "run.sh: give a cargo command." >&2
		exit 2
	}
	run_cargo default "$@"
	;;
offline)
	shift
	[ "${1:-}" = cargo ] && [ "$#" -gt 1 ] || {
		echo "run.sh: use run.sh offline cargo <command>." >&2
		exit 2
	}
	shift
	run_cargo none "$@"
	;;
*)
	echo "usage: run.sh image | run.sh [offline] cargo <command> [arguments] | run.sh secret-service | run.sh prune" >&2
	exit 2
	;;
esac
