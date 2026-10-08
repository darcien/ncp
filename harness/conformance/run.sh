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
# Each version runs three times: the frozen requirement set of the
# version, all scenarios of the version, and all scenarios again with the
# era probe of ncp in place of the era that the adapter
# writes. expected-failures.yaml lists the known
# failures with their reasons. The suite fails on a new failure and on an
# entry that passes.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
suite="$here/node_modules/@modelcontextprotocol/conformance/dist/index.js"

if [ ! -f "$suite" ]; then
	echo "run.sh: the suite is not installed; run: mise run conformance:install" >&2
	exit 2
fi

cargo build --locked -p ncp-testkit --bin ncp-cli --bin ncp-conformance
adapter="$root/target/debug/ncp-conformance"
results="$here/results"
rm -rf "$results"

run() {
	env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u NO_PROXY \
		-u http_proxy -u https_proxy -u all_proxy -u no_proxy \
		node "$suite" client --command "$adapter" \
		--expected-failures "$here/expected-failures.yaml" "$@"
}

status=0
for version in 2025-11-25 2026-07-28; do
	echo "== requirements $version"
	run --requirements "$version" -o "$results/requirements-$version" || status=1
	echo "== all scenarios $version"
	run --suite all --spec-version "$version" -o "$results/all-$version" || status=1
	echo "== all scenarios $version, with the era probe of ncp"
	NCP_CONFORMANCE_PROBE=1 run --suite all --spec-version "$version" \
		-o "$results/probe-$version" || status=1
done
exit "$status"
