#!/bin/sh
# Runs the official MCP conformance suite against ncp, through the adapter
# ncp-conformance, for each protocol version that ncp supports.
#
# Usage:
#   harness/conformance/run.sh
#
# Run "mise run conformance:install" once first: it installs the suite at
# the exact version of package-lock.json. This script uses no network: the
# suite starts its servers on localhost. The proxy variables are removed,
# because the suite uses localhost URLs.
#
# Each version runs four times: the frozen requirement set of the
# version, all scenarios of the version, all scenarios again with the era
# probe of ncp in place of the era that the adapter writes, and all
# scenarios as an agent with no TTY. expected-failures.yaml and
# expected-failures-agent.yaml list the known failures with their reasons.
# The suite fails on a new failure and on an entry that passes.
#
# After the runs, report.mjs writes results/conformance.json from the
# requirements runs, also when a run failed, so that the file shows the
# failure. The exit status still tells if each run succeeded.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
suite="$here/node_modules/@modelcontextprotocol/conformance/dist/index.js"

if [ ! -f "$suite" ]; then
	echo "run.sh: the suite is not installed; run: mise run conformance:install" >&2
	exit 2
fi

# The old results go first, so that a failed build leaves no old file.
results="$here/results"
rm -rf "$results"
cargo build --locked -p ncp-testkit --bin ncp-cli --bin ncp-conformance
adapter="$root/target/debug/ncp-conformance"

run_with() {
	expected="$1"
	shift
	env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u NO_PROXY \
		-u http_proxy -u https_proxy -u all_proxy -u no_proxy \
		node "$suite" client --command "$adapter" \
		--expected-failures "$here/$expected" "$@"
}

run() {
	run_with expected-failures.yaml "$@"
}

versions="2025-11-25 2026-07-28"
status=0
for version in $versions; do
	echo "== requirements $version"
	run --requirements "$version" -o "$results/requirements-$version" || status=1
	echo "== all scenarios $version"
	run --suite all --spec-version "$version" -o "$results/all-$version" || status=1
	echo "== all scenarios $version, with the era probe of ncp"
	# A subshell: in POSIX sh, a variable set before a function call stays
	# set after it.
	(NCP_CONFORMANCE_PROBE=1 run --suite all --spec-version "$version" \
		-o "$results/probe-$version") || status=1
	echo "== all scenarios $version, as an agent with no TTY"
	(NCP_CONFORMANCE_AGENT=1 run_with expected-failures-agent.yaml --suite all \
		--spec-version "$version" -o "$results/agent-$version") || status=1
done
# An assignment stops the script when its command fails. A command
# substitution in an argument does not stop it.
ncp_version="$("$root/target/debug/ncp-cli" --version)"
commit="$(git -C "$root" rev-parse HEAD)"
# In the root, rustup uses the toolchain of rust-toolchain.toml.
target="$(cd "$root" && rustc --print host-tuple)"
# shellcheck disable=SC2086 # Each version is one argument.
node "$here/report.mjs" "${ncp_version#ncp }" "$commit" "$target" $versions || status=1
exit "$status"
