#!/bin/sh
# Builds the Linux harness image, and runs cargo for the musl target in it.
#
# Usage:
#   harness/linux/run.sh image
#   harness/linux/run.sh cargo <command> [arguments]
#   harness/linux/run.sh offline cargo <command> [arguments]
#   harness/linux/run.sh secret-service
#   harness/linux/run.sh prune
#   harness/linux/run.sh release
#   harness/linux/run.sh smoke
#   harness/linux/run.sh pack
#
# "release" builds the static release binary of ncp with no network, and
# copies it to dist/ncp-<target>. Run "cargo fetch" first. "smoke" runs
# that binary in an empty image (no shell, no libc, no files) with no
# network: --version, server list, and secret list. "pack" puts each binary
# in dist/ into an archive ncp-<version>-<target>.tar.gz with the license
# files, and writes dist/SHA256SUMS for the archives.
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

# The release build. Only -p ncp: in the workspace, cargo unifies the
# features of rmcp with the server features of the test kit.
release() {
	mkdir -p "$root/dist"
	docker run --rm --init \
		--network none \
		--env CARGO_NET_OFFLINE=true \
		--platform "$platform" \
		--user "$(id -u):$(id -g)" \
		--env HOME=/tmp \
		--volume "$root:/work:ro" \
		--volume "$root/dist:/dist" \
		--volume ncp-harness-cargo-registry:/usr/local/cargo/registry \
		--volume "ncp-harness-target-$name:/target" \
		"$image" \
		sh -c "cargo build --release --locked -p ncp --target $target \
			&& cp /target/$target/release/ncp /dist/ncp-$target"
}

# An image with only the binary, made with docker import: no base image,
# no download. The binary must run with no libc and no files.
smoke() {
	binary="$root/dist/ncp-$target"
	[ -f "$binary" ] || {
		echo "run.sh: $binary does not exist; run: run.sh release" >&2
		exit 2
	}
	work="$(mktemp -d "$root/dist/smoke.XXXXXX")"
	mkdir -p "$work/bin"
	cp "$binary" "$work/bin/ncp"
	tar -C "$work" -c bin | docker import --platform "$platform" - "ncp-smoke:$name" >/dev/null
	rm -rf "$work"
	for command in "--version" "server list" "secret list"; do
		echo "== ncp $command"
		# shellcheck disable=SC2086
		docker run --rm --network none --platform "$platform" \
			--env HOME=/ --env XDG_CONFIG_HOME=/ --env XDG_CACHE_HOME=/ --env XDG_STATE_HOME=/ \
			"ncp-smoke:$name" /bin/ncp $command
	done
	docker image rm "ncp-smoke:$name" >/dev/null
}

# The release archives, made with GNU tar and gzip with no network. Each
# archive has one directory with the binary and the license files: the MIT
# license asks for its notice in each copy, and mise and Homebrew remove
# one top directory. The fixed order, owner and time give the same bytes
# for the same binaries. The time is the time of the last commit.
pack() {
	version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)"
	epoch="$(git -C "$root" log -1 --format=%ct)"
	docker run --rm --init \
		--network none \
		--platform "$platform" \
		--user "$(id -u):$(id -g)" \
		--env HOME=/tmp \
		--env VERSION="$version" \
		--env SOURCE_DATE_EPOCH="$epoch" \
		--volume "$root:/work:ro" \
		--volume "$root/dist:/dist" \
		"$image" \
		sh -eu -c '
			cd /dist
			rm -f ncp-*.tar.gz SHA256SUMS
			stage="$(mktemp -d)"
			for binary in ncp-*-apple-darwin ncp-*-unknown-linux-musl; do
				[ -f "$binary" ] || { echo "run.sh: dist/$binary is missing" >&2; exit 2; }
				target="${binary#ncp-}"
				top="ncp-$VERSION-$target"
				mkdir "$stage/$top"
				cp "$binary" "$stage/$top/ncp"
				cp /work/LICENSE-MIT /work/LICENSE-APACHE "$stage/$top/"
				chmod 0755 "$stage/$top/ncp"
				chmod 0644 "$stage/$top/LICENSE-MIT" "$stage/$top/LICENSE-APACHE"
				tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner \
					--mtime="@$SOURCE_DATE_EPOCH" -cf - "$top" | gzip -n -9 >"$top.tar.gz"
			done
			rm -rf "$stage"
			sha256sum ncp-*.tar.gz >SHA256SUMS
		'
	cat "$root/dist/SHA256SUMS"
}

case "${1:-}" in
image) build_image ;;
pack) pack ;;
release) release ;;
smoke) smoke ;;
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
	echo "usage: run.sh image | run.sh [offline] cargo <command> [arguments] | run.sh secret-service | run.sh prune | run.sh release | run.sh smoke | run.sh pack" >&2
	exit 2
	;;
esac
